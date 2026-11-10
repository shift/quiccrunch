//! Transport seam for Raft RPCs.
//!
//! distri-kv can implement its replication loop against [`RaftTransport`]
//! instead of tonic. [`QuicRaftTransport`](super::quic::QuicRaftTransport)
//! is the QuicCrunch implementation; [`InMemoryRaftTransport`] is the reference
//! implementation used by tests and by applications that colocate partitions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::types::*;

#[derive(Debug, Default)]
pub struct RaftMetrics {
    pub append_entries_total: AtomicU64,
    pub request_vote_total: AtomicU64,
    pub snapshot_chunks_total: AtomicU64,
    pub queue_depth: AtomicU64,
    pub max_queue_depth: AtomicU64,
    pub shed_total: AtomicU64,
    pub timeout_total: AtomicU64,
    pub reconnect_total: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub not_leader_total: AtomicU64,
}

impl RaftMetrics {
    pub fn snapshot(&self) -> RaftMetricsSnapshot {
        RaftMetricsSnapshot {
            append_entries_total: self.append_entries_total.load(Ordering::Relaxed),
            request_vote_total: self.request_vote_total.load(Ordering::Relaxed),
            snapshot_chunks_total: self.snapshot_chunks_total.load(Ordering::Relaxed),
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            max_queue_depth: self.max_queue_depth.load(Ordering::Relaxed),
            shed_total: self.shed_total.load(Ordering::Relaxed),
            timeout_total: self.timeout_total.load(Ordering::Relaxed),
            reconnect_total: self.reconnect_total.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            not_leader_total: self.not_leader_total.load(Ordering::Relaxed),
        }
    }

    pub(crate) fn enter_queue(&self) {
        let depth = self.queue_depth.fetch_add(1, Ordering::Relaxed) + 1;
        self.max_queue_depth.fetch_max(depth, Ordering::Relaxed);
    }

    pub(crate) fn leave_queue(&self) {
        self.queue_depth.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn shed(&self) {
        self.shed_total.fetch_add(1, Ordering::Relaxed);
    }
}

/// Server-side dispatch surface. Distri-kv's `PartitionRaft` implementation
/// plugs in here.
#[async_trait]
pub trait RaftService: Send + Sync {
    async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse>;

    async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<RequestVoteResponse>;

    /// Accept one snapshot frame. Implementations must retain the partial
    /// snapshot keyed by `(partition_id, last_index, last_term)` so a sender
    /// can resume at `next_offset` after a reconnect.
    async fn install_snapshot_chunk(
        &self,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck>;

    /// Current leader for a partition, used to enrich `NotLeader` errors.
    fn leader_hint(&self, _partition_id: u64) -> Option<LeaderHint> {
        None
    }
}

/// Node-to-node Raft transport. Calls are addressed by peer, so one client
/// instance can serve every partition in a node.
#[async_trait]
pub trait RaftTransport: Send + Sync {
    async fn append_entries(
        &self,
        peer: &RaftPeer,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse>;

    async fn request_vote(
        &self,
        peer: &RaftPeer,
        request: RequestVoteRequest,
    ) -> RaftResult<RequestVoteResponse>;

    async fn install_snapshot_chunk(
        &self,
        peer: &RaftPeer,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck>;

    fn metrics(&self) -> RaftMetricsSnapshot {
        RaftMetricsSnapshot::default()
    }

    /// Stream a whole snapshot. Chunks are sent one at a time and every chunk
    /// is acknowledged, so an interrupted call resumes from
    /// `last_ack.next_offset` without re-sending accepted bytes.
    async fn install_snapshot(
        &self,
        peer: &RaftPeer,
        upload: SnapshotUpload,
    ) -> RaftResult<InstallSnapshotAck> {
        let chunk_size = upload.chunk_size.clamp(1, MAX_SNAPSHOT_CHUNK_BYTES);
        let total = upload.data.len() as u64;
        let mut offset = upload.start_offset.min(total);
        let mut retries = 0u32;
        loop {
            let end = (offset + chunk_size as u64).min(total);
            let chunk = upload.data[offset as usize..end as usize].to_vec();
            let done = end == total;
            let request = InstallSnapshotChunk {
                partition_id: upload.partition_id,
                term: upload.term,
                leader_id: upload.leader_id,
                last_index: upload.last_index,
                last_term: upload.last_term,
                offset,
                chunk,
                done,
            };
            match self.install_snapshot_chunk(peer, request).await {
                Ok(ack) => {
                    if ack.next_offset < offset {
                        return Err(RaftError::snapshot_stale(
                            "receiver acknowledged an offset before the sent offset",
                        ));
                    }
                    if ack.next_offset == offset && !ack.done {
                        return Err(RaftError::internal("snapshot receiver made no progress"));
                    }
                    offset = ack.next_offset;
                    retries = 0;
                    if ack.done {
                        return Ok(ack);
                    }
                }
                Err(err) if err.retryable && retries < 3 => {
                    retries += 1;
                    let delay = err
                        .retry_after_ms
                        .map(Duration::from_millis)
                        .unwrap_or_else(|| Duration::from_millis(25 * (1 << retries)));
                    tokio::time::sleep(delay).await;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

/// In-process reference transport. Useful for unit tests and for deployments
/// where partitions share a process.
pub struct InMemoryRaftTransport {
    services: RwLock<HashMap<u64, Arc<dyn RaftService>>>,
    metrics: Arc<RaftMetrics>,
}

impl Default for InMemoryRaftTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRaftTransport {
    pub fn new() -> Self {
        Self {
            services: RwLock::new(HashMap::new()),
            metrics: Arc::new(RaftMetrics::default()),
        }
    }

    pub async fn register(&self, node_id: u64, service: Arc<dyn RaftService>) {
        self.services.write().await.insert(node_id, service);
    }

    pub async fn unregister(&self, node_id: u64) {
        self.services.write().await.remove(&node_id);
    }

    async fn lookup(&self, peer: &RaftPeer) -> RaftResult<Arc<dyn RaftService>> {
        self.services
            .read()
            .await
            .get(&peer.node_id)
            .cloned()
            .ok_or_else(|| {
                RaftError::unavailable(format!("no in-process service for node {}", peer.node_id))
            })
    }
}

#[async_trait]
impl RaftTransport for InMemoryRaftTransport {
    async fn append_entries(
        &self,
        peer: &RaftPeer,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse> {
        self.metrics
            .append_entries_total
            .fetch_add(1, Ordering::Relaxed);
        self.lookup(peer).await?.append_entries(request).await
    }

    async fn request_vote(
        &self,
        peer: &RaftPeer,
        request: RequestVoteRequest,
    ) -> RaftResult<RequestVoteResponse> {
        self.metrics
            .request_vote_total
            .fetch_add(1, Ordering::Relaxed);
        self.lookup(peer).await?.request_vote(request).await
    }

    async fn install_snapshot_chunk(
        &self,
        peer: &RaftPeer,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck> {
        self.metrics
            .snapshot_chunks_total
            .fetch_add(1, Ordering::Relaxed);
        self.lookup(peer)
            .await?
            .install_snapshot_chunk(request)
            .await
    }

    fn metrics(&self) -> RaftMetricsSnapshot {
        self.metrics.snapshot()
    }
}
