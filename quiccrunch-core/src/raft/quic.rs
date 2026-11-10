//! QuicCrunch implementation of the Raft transport seam.
//!
//! One QUIC connection is reused per peer. Every RPC opens its own
//! bidirectional stream, so partitions sharing a connection cannot block each
//! other. The client keeps a bounded number of in-flight RPCs per peer and
//! sheds excess work with a retryable `Backpressure` error.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use rand::Rng;
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::schema::raft_capnp::RpcKind;
use crate::transport::adapter::{
    RuntimeOptions, TquicClient, TquicConnection, TquicListener, TquicStream,
};

use super::codec::{
    decode_request, decode_response, encode_append_entries, encode_append_entries_response,
    encode_error_response, encode_request_vote, encode_request_vote_response, encode_snapshot_ack,
    encode_snapshot_chunk, peek_kind, RequestMessage, ResponseMessage,
};
use super::transport::{RaftMetrics, RaftService, RaftTransport};
use super::types::*;

#[derive(Debug, Clone)]
pub struct QuicRaftConfig {
    pub rpc_timeout: Duration,
    pub connect_timeout: Duration,
    pub max_in_flight_per_peer: usize,
    pub queue_wait: Duration,
    pub max_connect_attempts: u32,
    pub reconnect_base: Duration,
    pub reconnect_max: Duration,
}

impl Default for QuicRaftConfig {
    fn default() -> Self {
        Self {
            rpc_timeout: DEFAULT_RPC_TIMEOUT,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            max_in_flight_per_peer: DEFAULT_MAX_IN_FLIGHT_PER_PEER,
            queue_wait: DEFAULT_QUEUE_WAIT,
            max_connect_attempts: DEFAULT_MAX_CONNECT_ATTEMPTS,
            reconnect_base: DEFAULT_RECONNECT_BASE,
            reconnect_max: DEFAULT_RECONNECT_MAX,
        }
    }
}

struct PeerState {
    addr: String,
    connection: Mutex<Option<Arc<TquicConnection>>>,
    in_flight: Semaphore,
}

impl PeerState {
    fn new(addr: String, max_in_flight: usize) -> Self {
        Self {
            addr,
            connection: Mutex::new(None),
            in_flight: Semaphore::new(max_in_flight),
        }
    }
}

pub struct QuicRaftTransport {
    client: TquicClient,
    config: QuicRaftConfig,
    peers: RwLock<HashMap<String, Arc<PeerState>>>,
    metrics: Arc<RaftMetrics>,
}

impl QuicRaftTransport {
    pub fn new(client: TquicClient, config: QuicRaftConfig) -> Self {
        Self {
            client,
            config,
            peers: RwLock::new(HashMap::new()),
            metrics: Arc::new(RaftMetrics::default()),
        }
    }

    pub async fn with_runtime_options(
        options: RuntimeOptions,
        config: QuicRaftConfig,
    ) -> RaftResult<Self> {
        let client = TquicClient::with_options(options)
            .await
            .map_err(|e| RaftError::unavailable(format!("client init failed: {e}")))?;
        Ok(Self::new(client, config))
    }

    async fn peer_state(&self, peer: &RaftPeer) -> Arc<PeerState> {
        if let Some(state) = self.peers.read().await.get(&peer.addr) {
            return state.clone();
        }
        let mut peers = self.peers.write().await;
        peers
            .entry(peer.addr.clone())
            .or_insert_with(|| {
                Arc::new(PeerState::new(
                    peer.addr.clone(),
                    self.config.max_in_flight_per_peer.max(1),
                ))
            })
            .clone()
    }

    async fn invalidate(&self, state: &PeerState) {
        *state.connection.lock().await = None;
    }

    async fn connection(&self, state: &PeerState) -> RaftResult<Arc<TquicConnection>> {
        if let Some(connection) = state.connection.lock().await.clone() {
            return Ok(connection);
        }

        // Only one task performs the handshake; the lock is held across the
        // backoff loop so callers queue instead of stampeding the peer.
        let mut slot = state.connection.lock().await;
        if let Some(connection) = slot.clone() {
            return Ok(connection);
        }

        let mut attempt = 0u32;
        loop {
            let result = tokio::time::timeout(
                self.config.connect_timeout,
                self.client.connect(&state.addr),
            )
            .await;

            match result {
                Ok(Ok(connection)) => {
                    let connection = Arc::new(connection);
                    *slot = Some(connection.clone());
                    self.metrics.reconnect_total.fetch_add(1, Ordering::Relaxed);
                    return Ok(connection);
                }
                Ok(Err(err)) => {
                    attempt += 1;
                    if attempt >= self.config.max_connect_attempts {
                        return Err(RaftError::unavailable(format!(
                            "connect to {} failed after {} attempts: {err}",
                            state.addr, attempt
                        )));
                    }
                }
                Err(_) => {
                    attempt += 1;
                    if attempt >= self.config.max_connect_attempts {
                        return Err(RaftError::unavailable(format!(
                            "connect to {} timed out after {} attempts",
                            state.addr, attempt
                        )));
                    }
                }
            }

            let delay = backoff_delay(
                self.config.reconnect_base,
                self.config.reconnect_max,
                attempt,
            );
            tokio::time::sleep(delay).await;
        }
    }

    async fn round_trip(&self, state: &PeerState, frame: Vec<u8>) -> RaftResult<Vec<u8>> {
        let connection = self.connection(state).await?;
        let stream = match connection.open_stream().await {
            Ok(stream) => stream,
            Err(err) => {
                self.invalidate(state).await;
                return Err(RaftError::unavailable(format!("open stream failed: {err}")));
            }
        };
        if let Err(err) = stream.send_bytes(Bytes::from(frame), true).await {
            self.invalidate(state).await;
            return Err(RaftError::unavailable(format!("send failed: {err}")));
        }
        match read_frame(&stream).await {
            Ok(frame) => {
                self.metrics
                    .bytes_received
                    .fetch_add(frame.len() as u64, Ordering::Relaxed);
                Ok(frame)
            }
            Err(err) => {
                // A closed/reset stream may mean the pooled connection is
                // dead. Drop it; the request itself is not retried.
                self.invalidate(state).await;
                Err(err)
            }
        }
    }

    async fn unary(&self, peer: &RaftPeer, frame: Vec<u8>) -> RaftResult<ResponseMessage> {
        let state = self.peer_state(peer).await;
        self.metrics.enter_queue();
        let permit =
            match tokio::time::timeout(self.config.queue_wait, state.in_flight.acquire()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(_)) => {
                    self.metrics.leave_queue();
                    return Err(RaftError::unavailable("peer queue closed"));
                }
                Err(_) => {
                    self.metrics.leave_queue();
                    self.metrics.shed();
                    return Err(RaftError::backpressure(format!(
                        "peer {} in-flight queue full",
                        peer.addr
                    ))
                    .with_retry_after(self.config.queue_wait.as_millis() as u64));
                }
            };
        self.metrics.leave_queue();

        let outcome = tokio::time::timeout(self.config.rpc_timeout, async {
            let response = self.round_trip(&state, frame).await;
            match response {
                Ok(frame) => decode_response(&frame),
                Err(err) => Err(err),
            }
        })
        .await;
        drop(permit);

        match outcome {
            Ok(result) => {
                if let Err(err) = &result {
                    if err.code == ErrorCode::NotLeader {
                        self.metrics
                            .not_leader_total
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                result
            }
            Err(_) => {
                self.metrics.timeout_total.fetch_add(1, Ordering::Relaxed);
                Err(RaftError::deadline(format!(
                    "RPC to {} exceeded {:?}",
                    peer.addr, self.config.rpc_timeout
                )))
            }
        }
    }
}

fn backoff_delay(base: Duration, max: Duration, attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(10);
    let exp = base.saturating_mul(1u32 << shift);
    let capped = exp.min(max);
    let jitter_ms = if capped.as_millis() == 0 {
        0
    } else {
        rand::thread_rng().gen_range(0..=capped.as_millis() as u64)
    };
    capped + Duration::from_millis(jitter_ms)
}

async fn read_frame(stream: &TquicStream) -> RaftResult<Vec<u8>> {
    let mut buffer = Vec::new();
    loop {
        if buffer.len() >= 4 {
            let len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
            if len > MAX_FRAME_BYTES {
                return Err(RaftError::frame_too_large(len, MAX_FRAME_BYTES));
            }
            if buffer.len() >= 4 + len {
                buffer.truncate(4 + len);
                return Ok(buffer);
            }
        }
        let chunk = stream
            .receive(MAX_FRAME_BYTES)
            .await
            .map_err(|e| RaftError::unavailable(format!("receive failed: {e}")))?;
        if chunk.is_empty() {
            return Err(RaftError::unavailable("peer closed the stream"));
        }
        buffer.extend_from_slice(&chunk);
        if buffer.len() > MAX_FRAME_BYTES + 4 {
            return Err(RaftError::frame_too_large(
                buffer.len(),
                MAX_FRAME_BYTES + 4,
            ));
        }
    }
}

#[async_trait]
impl RaftTransport for QuicRaftTransport {
    async fn append_entries(
        &self,
        peer: &RaftPeer,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse> {
        self.metrics
            .append_entries_total
            .fetch_add(1, Ordering::Relaxed);
        let frame = encode_append_entries(&request)?;
        self.metrics
            .bytes_sent
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
        match self.unary(peer, frame).await? {
            ResponseMessage::AppendEntries(response) => Ok(response),
            _ => Err(RaftError::protocol("unexpected response type")),
        }
    }

    async fn request_vote(
        &self,
        peer: &RaftPeer,
        request: RequestVoteRequest,
    ) -> RaftResult<RequestVoteResponse> {
        self.metrics
            .request_vote_total
            .fetch_add(1, Ordering::Relaxed);
        let frame = encode_request_vote(&request)?;
        self.metrics
            .bytes_sent
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
        match self.unary(peer, frame).await? {
            ResponseMessage::RequestVote(response) => Ok(response),
            _ => Err(RaftError::protocol("unexpected response type")),
        }
    }

    async fn install_snapshot_chunk(
        &self,
        peer: &RaftPeer,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck> {
        self.metrics
            .snapshot_chunks_total
            .fetch_add(1, Ordering::Relaxed);
        let frame = encode_snapshot_chunk(&request)?;
        self.metrics
            .bytes_sent
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
        match self.unary(peer, frame).await? {
            ResponseMessage::InstallSnapshot(ack) => Ok(ack),
            _ => Err(RaftError::protocol("unexpected response type")),
        }
    }

    /// Streams a whole snapshot over a single QUIC stream. If the stream dies
    /// mid-transfer, the next attempt resumes at the last acknowledged offset;
    /// the receiver treats a replayed offset as idempotent.
    async fn install_snapshot(
        &self,
        peer: &RaftPeer,
        upload: SnapshotUpload,
    ) -> RaftResult<InstallSnapshotAck> {
        let state = self.peer_state(peer).await;
        let chunk_size = upload.chunk_size.clamp(1, MAX_SNAPSHOT_CHUNK_BYTES);
        let total = upload.data.len() as u64;
        let mut offset = upload.start_offset.min(total);
        let mut stream_retries = 0u32;

        self.metrics.enter_queue();
        let permit =
            match tokio::time::timeout(self.config.queue_wait, state.in_flight.acquire()).await {
                Ok(Ok(permit)) => permit,
                Ok(Err(_)) => {
                    self.metrics.leave_queue();
                    return Err(RaftError::unavailable("peer queue closed"));
                }
                Err(_) => {
                    self.metrics.leave_queue();
                    self.metrics.shed();
                    return Err(RaftError::backpressure("peer in-flight queue full"));
                }
            };
        self.metrics.leave_queue();

        let result = async {
            loop {
                let connection = self.connection(&state).await?;
                let stream = match connection.open_stream().await {
                    Ok(stream) => stream,
                    Err(err) => {
                        self.invalidate(&state).await;
                        if stream_retries >= 3 {
                            return Err(RaftError::unavailable(format!(
                                "open stream failed: {err}"
                            )));
                        }
                        stream_retries += 1;
                        continue;
                    }
                };

                let mut send_error = None;
                while offset < total || (total == 0 && offset == 0) {
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
                    let frame = encode_snapshot_chunk(&request)?;
                    self.metrics
                        .snapshot_chunks_total
                        .fetch_add(1, Ordering::Relaxed);
                    self.metrics
                        .bytes_sent
                        .fetch_add(frame.len() as u64, Ordering::Relaxed);

                    if let Err(err) = stream.send_bytes(Bytes::from(frame), false).await {
                        self.invalidate(&state).await;
                        send_error = Some(RaftError::unavailable(format!("send failed: {err}")));
                        break;
                    }
                    match read_frame(&stream).await {
                        Ok(frame) => match decode_response(&frame) {
                            Ok(ResponseMessage::InstallSnapshot(ack)) => {
                                if ack.next_offset < offset {
                                    return Err(RaftError::snapshot_stale(
                                        "acknowledged offset moved backwards",
                                    ));
                                }
                                if ack.next_offset == offset && !ack.done {
                                    return Err(RaftError::internal(
                                        "snapshot receiver made no progress",
                                    ));
                                }
                                offset = ack.next_offset;
                                if ack.done {
                                    return Ok(ack);
                                }
                            }
                            Ok(_) => return Err(RaftError::protocol("unexpected response type")),
                            Err(err) if err.retryable => {
                                self.invalidate(&state).await;
                                send_error = Some(err);
                                break;
                            }
                            Err(err) => return Err(err),
                        },
                        Err(err) => {
                            self.invalidate(&state).await;
                            send_error = Some(err);
                            break;
                        }
                    }
                    if done {
                        // A done frame without a done ack is a protocol error;
                        // fall through and let the receiver decide.
                    }
                }

                match send_error {
                    Some(err) if err.retryable && stream_retries < 3 => {
                        stream_retries += 1;
                        let delay = backoff_delay(
                            self.config.reconnect_base,
                            self.config.reconnect_max,
                            stream_retries,
                        );
                        tokio::time::sleep(delay).await;
                    }
                    Some(err) => return Err(err),
                    None if total == 0 && offset == 0 => {
                        // Zero-length snapshot: send one done frame at 0.
                        let request = InstallSnapshotChunk {
                            partition_id: upload.partition_id,
                            term: upload.term,
                            leader_id: upload.leader_id,
                            last_index: upload.last_index,
                            last_term: upload.last_term,
                            offset: 0,
                            chunk: Vec::new(),
                            done: true,
                        };
                        return self.install_snapshot_chunk(peer, request).await;
                    }
                    None => {
                        return Ok(InstallSnapshotAck {
                            term: upload.term,
                            success: true,
                            bytes_accepted: offset,
                            next_offset: offset,
                            done: true,
                        })
                    }
                }
            }
        }
        .await;
        drop(permit);
        result
    }

    fn metrics(&self) -> RaftMetricsSnapshot {
        self.metrics.snapshot()
    }
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// Accept connections until the listener fails. Each connection is served on
/// its own task; each RPC gets its own task and deadline.
pub async fn serve_listener(
    mut listener: TquicListener,
    service: Arc<dyn RaftService>,
    config: QuicRaftConfig,
) -> RaftResult<()> {
    loop {
        let connection = listener
            .accept()
            .await
            .map_err(|e| RaftError::unavailable(format!("accept failed: {e}")))?;
        let service = service.clone();
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(err) = serve_connection(connection, service, config).await {
                tracing::debug!("raft connection closed: {err}");
            }
        });
    }
}

pub async fn serve_connection(
    connection: TquicConnection,
    service: Arc<dyn RaftService>,
    config: QuicRaftConfig,
) -> RaftResult<()> {
    loop {
        let stream = match connection.accept_stream().await {
            Ok(stream) => stream,
            Err(err) => {
                return Err(RaftError::unavailable(format!(
                    "accept stream failed: {err}"
                )))
            }
        };
        let service = service.clone();
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_stream(stream, service, config).await {
                tracing::debug!("raft stream closed: {err}");
            }
        });
    }
}

async fn handle_stream(
    stream: TquicStream,
    service: Arc<dyn RaftService>,
    config: QuicRaftConfig,
) -> RaftResult<()> {
    loop {
        let frame = match tokio::time::timeout(config.rpc_timeout, read_frame(&stream)).await {
            Ok(Ok(frame)) => frame,
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err(RaftError::deadline("request read deadline")),
        };

        let kind = peek_kind(&frame).unwrap_or(RpcKind::AppendEntries);
        let request = match decode_request(&frame) {
            Ok(request) => request,
            Err(err) => {
                let response = encode_error_response(kind, &err)?;
                let _ = stream.send(&response, true).await;
                return Ok(());
            }
        };

        let partition_id = match &request {
            RequestMessage::AppendEntries(req) => req.partition_id,
            RequestMessage::RequestVote(req) => req.partition_id,
            RequestMessage::InstallSnapshot(req) => req.partition_id,
        };
        let is_snapshot_done = matches!(
            &request,
            RequestMessage::InstallSnapshot(req) if req.done
        );

        let outcome = tokio::time::timeout(config.rpc_timeout, async {
            match request {
                RequestMessage::AppendEntries(req) => service
                    .append_entries(req)
                    .await
                    .map(ResponseMessage::AppendEntries),
                RequestMessage::RequestVote(req) => service
                    .request_vote(req)
                    .await
                    .map(ResponseMessage::RequestVote),
                RequestMessage::InstallSnapshot(req) => service
                    .install_snapshot_chunk(req)
                    .await
                    .map(ResponseMessage::InstallSnapshot),
            }
        })
        .await;

        let response = match outcome {
            Ok(Ok(ResponseMessage::AppendEntries(response))) => {
                encode_append_entries_response(&response)?
            }
            Ok(Ok(ResponseMessage::RequestVote(response))) => {
                encode_request_vote_response(&response)?
            }
            Ok(Ok(ResponseMessage::InstallSnapshot(response))) => encode_snapshot_ack(&response)?,
            Ok(Err(mut err)) => {
                if err.code == ErrorCode::NotLeader && err.leader_hint.is_none() {
                    err.leader_hint = service.leader_hint(partition_id);
                }
                encode_error_response(kind, &err)?
            }
            Err(_) => {
                encode_error_response(kind, &RaftError::deadline("service deadline exceeded"))?
            }
        };

        let fin = is_snapshot_done || !matches!(kind, RpcKind::InstallSnapshot);
        stream
            .send(&response, fin)
            .await
            .map_err(|e| RaftError::unavailable(format!("send response failed: {e}")))?;
        if fin {
            return Ok(());
        }
    }
}
