#![allow(dead_code)]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use quiccrunch_core::raft::{
    serve_listener, AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotAck,
    InstallSnapshotChunk, LeaderHint, LogEntry, QuicRaftConfig, RaftError, RaftPeer, RaftResult,
    RaftService, RaftTransport, RequestVoteRequest, RequestVoteResponse, SnapshotUpload,
};
use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicServer};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    IsCa, KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub struct TestTls {
    _dir: tempfile::TempDir,
    pub ca_file: String,
    pub cert_file: String,
    pub key_file: String,
}

impl TestTls {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut ca_params = CertificateParams::new(vec!["Raft Test CA".to_string()]);
        ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca_params.alg = &PKCS_ECDSA_P256_SHA256;
        ca_params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::CrlSign,
        ];
        ca_params.not_before = date_time_ymd(2020, 1, 1);
        ca_params.not_after = date_time_ymd(2050, 1, 1);
        ca_params.distinguished_name = DistinguishedName::new();
        ca_params
            .distinguished_name
            .push(DnType::CommonName, "Raft Test CA");
        let ca = Certificate::from_params(ca_params).expect("CA cert");

        let mut leaf_params =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]);
        leaf_params.distinguished_name = DistinguishedName::new();
        leaf_params
            .distinguished_name
            .push(DnType::CommonName, "raft-test-node");
        leaf_params.is_ca = IsCa::NoCa;
        leaf_params.alg = &PKCS_ECDSA_P256_SHA256;
        leaf_params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        leaf_params.not_before = date_time_ymd(2020, 1, 1);
        leaf_params.not_after = date_time_ymd(2050, 1, 1);
        leaf_params
            .subject_alt_names
            .push(SanType::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        leaf_params
            .subject_alt_names
            .push(SanType::DnsName("127.0.0.1".to_string()));
        let leaf = Certificate::from_params(leaf_params).expect("leaf cert");

        let ca_path = dir.path().join("ca.pem");
        let cert_path = dir.path().join("node.pem");
        let key_path = dir.path().join("node.key");
        std::fs::write(&ca_path, ca.serialize_pem().unwrap()).unwrap();
        std::fs::write(&cert_path, leaf.serialize_pem_with_signer(&ca).unwrap()).unwrap();
        std::fs::write(&key_path, leaf.serialize_private_key_pem()).unwrap();

        Self {
            _dir: dir,
            ca_file: ca_path.to_string_lossy().into_owned(),
            cert_file: cert_path.to_string_lossy().into_owned(),
            key_file: key_path.to_string_lossy().into_owned(),
        }
    }

    pub fn client_options(&self) -> RuntimeOptions {
        RuntimeOptions {
            verify_server_cert: true,
            ca_cert_file: Some(self.ca_file.clone()),
            ..RuntimeOptions::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Default)]
struct Inner {
    term: u64,
    voted_for: Option<u64>,
    role: Role,
    leader_id: Option<u64>,
    log: Vec<LogEntry>,
    commit_index: u64,
    snapshot: Vec<u8>,
    snapshot_last_index: u64,
    snapshot_last_term: u64,
    partial_snapshots: HashMap<(u64, u64, u64), Vec<u8>>,
    next_index: HashMap<u64, u64>,
    append_delay: HashMap<u64, Duration>,
    vote_delay: Duration,
    redirect_append: bool,
    fail_snapshot_once: bool,
}

impl Default for Role {
    fn default() -> Self {
        Role::Follower
    }
}

pub struct TestNode {
    pub id: u64,
    pub partition_id: u64,
    pub addr: String,
    pub peers: Vec<RaftPeer>,
    inner: Mutex<Inner>,
    pub fail_next_snapshot: AtomicBool,
}

impl TestNode {
    pub fn new(id: u64, partition_id: u64, addr: String, peers: Vec<RaftPeer>) -> Arc<Self> {
        Arc::new(Self {
            id,
            partition_id,
            addr,
            peers,
            inner: Mutex::new(Inner::default()),
            fail_next_snapshot: AtomicBool::new(false),
        })
    }

    pub async fn set_append_delay(&self, partition_id: u64, delay: Duration) {
        self.inner
            .lock()
            .await
            .append_delay
            .insert(partition_id, delay);
    }

    pub async fn set_vote_delay(&self, delay: Duration) {
        self.inner.lock().await.vote_delay = delay;
    }

    pub async fn set_redirect(&self, redirect: bool) {
        self.inner.lock().await.redirect_append = redirect;
    }

    pub async fn log_len(&self) -> usize {
        self.inner.lock().await.log.len()
    }

    pub async fn log_data(&self) -> Vec<Vec<u8>> {
        self.inner
            .lock()
            .await
            .log
            .iter()
            .map(|entry| entry.data.clone())
            .collect()
    }

    pub async fn role(&self) -> Role {
        self.inner.lock().await.role
    }

    pub async fn term(&self) -> u64 {
        self.inner.lock().await.term
    }

    pub async fn snapshot(&self) -> Vec<u8> {
        self.inner.lock().await.snapshot.clone()
    }

    pub async fn snapshot_base(&self) -> (u64, u64) {
        let inner = self.inner.lock().await;
        (inner.snapshot_last_index, inner.snapshot_last_term)
    }

    pub async fn leader_hint_for(&self, partition_id: u64) -> Option<LeaderHint> {
        let inner = self.inner.lock().await;
        let id = inner.leader_id?;
        self.peers
            .iter()
            .find(|peer| peer.node_id == id)
            .map(|peer| LeaderHint::new(peer.node_id, peer.addr.clone()))
            .or_else(|| {
                if id == self.id {
                    Some(LeaderHint::new(self.id, self.addr.clone()))
                } else {
                    None
                }
            })
            .inspect(|_| {
                let _ = partition_id;
            })
    }

    fn last_index(inner: &Inner) -> u64 {
        inner.log.last().map(|entry| entry.index).unwrap_or(0)
    }

    fn last_term(inner: &Inner) -> u64 {
        inner.log.last().map(|entry| entry.term).unwrap_or(0)
    }

    fn term_at(inner: &Inner, index: u64) -> Option<u64> {
        if index == 0 {
            return Some(0);
        }
        inner
            .log
            .iter()
            .find(|entry| entry.index == index)
            .map(|entry| entry.term)
    }

    pub async fn campaign(&self, transport: &dyn RaftTransport, peers: &[RaftPeer]) -> bool {
        let (term, last_index, last_term) = {
            let mut inner = self.inner.lock().await;
            inner.term += 1;
            inner.role = Role::Candidate;
            inner.voted_for = Some(self.id);
            (
                inner.term,
                Self::last_index(&inner),
                Self::last_term(&inner),
            )
        };
        let mut votes = 1;
        for peer in peers {
            let request = RequestVoteRequest {
                partition_id: self.partition_id,
                term,
                candidate_id: self.id,
                last_log_index: last_index,
                last_log_term: last_term,
            };
            if let Ok(response) = transport.request_vote(peer, request).await {
                if response.vote_granted {
                    votes += 1;
                }
            }
        }
        let won = votes * 2 > peers.len() + 1;
        let mut inner = self.inner.lock().await;
        if won {
            inner.role = Role::Leader;
            inner.leader_id = Some(self.id);
            let next = Self::last_index(&inner) + 1;
            for peer in peers {
                inner.next_index.insert(peer.node_id, next);
            }
        }
        won
    }

    pub async fn propose(
        &self,
        transport: &dyn RaftTransport,
        peers: &[RaftPeer],
        data: Vec<u8>,
    ) -> RaftResult<()> {
        let index = {
            let mut inner = self.inner.lock().await;
            if inner.role != Role::Leader {
                return Err(RaftError::not_leader(LeaderHint::new(
                    self.id,
                    self.addr.clone(),
                )));
            }
            let index = Self::last_index(&inner) + 1;
            let term = inner.term;
            inner.log.push(LogEntry { term, index, data });
            index
        };
        self.replicate_round(transport, peers).await?;
        let mut inner = self.inner.lock().await;
        inner.commit_index = index;
        Ok(())
    }

    pub async fn replicate_round(
        &self,
        transport: &dyn RaftTransport,
        peers: &[RaftPeer],
    ) -> RaftResult<()> {
        for peer in peers {
            let mut attempts = 0;
            loop {
                attempts += 1;
                let request = {
                    let mut inner = self.inner.lock().await;
                    let last = Self::last_index(&inner);
                    let next = *inner.next_index.entry(peer.node_id).or_insert(last + 1);
                    let next = next.max(1);
                    let prev_log_index = next - 1;
                    let prev_log_term = Self::term_at(&inner, prev_log_index).unwrap_or(0);
                    let entries: Vec<LogEntry> = inner
                        .log
                        .iter()
                        .filter(|entry| entry.index >= next)
                        .cloned()
                        .collect();
                    AppendEntriesRequest {
                        partition_id: self.partition_id,
                        term: inner.term,
                        leader_id: self.id,
                        prev_log_index,
                        prev_log_term,
                        entries,
                        leader_commit: inner.commit_index,
                    }
                };
                match transport.append_entries(peer, request).await {
                    Ok(response) if response.success => {
                        self.inner
                            .lock()
                            .await
                            .next_index
                            .insert(peer.node_id, response.match_index + 1);
                        break;
                    }
                    Ok(response) => {
                        let mut inner = self.inner.lock().await;
                        let next = if response.conflict_index > 0 {
                            response.conflict_index
                        } else {
                            inner
                                .next_index
                                .get(&peer.node_id)
                                .copied()
                                .unwrap_or(2)
                                .saturating_sub(1)
                                .max(1)
                        };
                        inner.next_index.insert(peer.node_id, next);
                        if attempts >= 8 {
                            return Err(RaftError::internal(
                                "replication conflict retries exhausted",
                            ));
                        }
                    }
                    Err(err) if err.code == quiccrunch_core::raft::ErrorCode::NotLeader => break,
                    Err(err) => return Err(err),
                }
            }
        }
        Ok(())
    }

    pub async fn install_snapshot_from(
        &self,
        transport: &dyn RaftTransport,
        peer: &RaftPeer,
        data: Vec<u8>,
        last_index: u64,
        last_term: u64,
    ) -> RaftResult<InstallSnapshotAck> {
        let (term, leader_id) = {
            let inner = self.inner.lock().await;
            (inner.term, self.id)
        };
        let mut upload = SnapshotUpload::new(
            self.partition_id,
            term,
            leader_id,
            last_index,
            last_term,
            data,
        );
        upload.chunk_size = 16 * 1024;
        transport.install_snapshot(peer, upload).await
    }
}

#[async_trait]
impl RaftService for TestNode {
    async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse> {
        let delay = {
            self.inner
                .lock()
                .await
                .append_delay
                .get(&request.partition_id)
                .copied()
        };
        if let Some(delay) = delay {
            tokio::time::sleep(delay).await;
        }

        let mut inner = self.inner.lock().await;
        if request.term < inner.term {
            return Ok(AppendEntriesResponse {
                term: inner.term,
                success: false,
                match_index: 0,
                conflict_index: Self::last_index(&inner) + 1,
                conflict_term: 0,
            });
        }
        if request.term > inner.term {
            inner.term = request.term;
            inner.voted_for = None;
            inner.role = Role::Follower;
        }
        if inner.redirect_append && request.leader_id != self.id {
            let hint = inner
                .leader_id
                .and_then(|id| {
                    self.peers
                        .iter()
                        .find(|peer| peer.node_id == id)
                        .map(|peer| LeaderHint::new(id, peer.addr.clone()))
                })
                .unwrap_or_default();
            return Err(RaftError::not_leader(hint));
        }
        inner.leader_id = Some(request.leader_id);
        inner.role = Role::Follower;

        let prev_term = Self::term_at(&inner, request.prev_log_index);
        match prev_term {
            None => {
                return Ok(AppendEntriesResponse {
                    term: inner.term,
                    success: false,
                    match_index: 0,
                    conflict_index: Self::last_index(&inner) + 1,
                    conflict_term: 0,
                });
            }
            Some(term) if term != request.prev_log_term => {
                let conflict_term = term;
                let conflict_index = inner
                    .log
                    .iter()
                    .find(|entry| entry.term == conflict_term)
                    .map(|entry| entry.index)
                    .unwrap_or(request.prev_log_index);
                return Ok(AppendEntriesResponse {
                    term: inner.term,
                    success: false,
                    match_index: 0,
                    conflict_index,
                    conflict_term,
                });
            }
            Some(_) => {}
        }

        for entry in request.entries {
            if let Some(existing) = inner.log.iter_mut().find(|e| e.index == entry.index) {
                if existing.term != entry.term {
                    inner.log.retain(|e| e.index < entry.index);
                    inner.log.push(entry);
                }
            } else {
                inner.log.push(entry);
            }
        }
        inner.log.sort_by_key(|entry| entry.index);
        let last = Self::last_index(&inner);
        inner.commit_index = inner.commit_index.max(request.leader_commit.min(last));
        Ok(AppendEntriesResponse {
            term: inner.term,
            success: true,
            match_index: last,
            conflict_index: 0,
            conflict_term: 0,
        })
    }

    async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<RequestVoteResponse> {
        let delay = self.inner.lock().await.vote_delay;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let mut inner = self.inner.lock().await;
        if request.term < inner.term {
            return Ok(RequestVoteResponse {
                term: inner.term,
                vote_granted: false,
            });
        }
        if request.term > inner.term {
            inner.term = request.term;
            inner.voted_for = None;
            inner.role = Role::Follower;
            inner.leader_id = None;
        }
        let up_to_date = request.last_log_term > Self::last_term(&inner)
            || (request.last_log_term == Self::last_term(&inner)
                && request.last_log_index >= Self::last_index(&inner));
        let can_vote = inner.voted_for.is_none() || inner.voted_for == Some(request.candidate_id);
        let granted = can_vote && up_to_date;
        if granted {
            inner.voted_for = Some(request.candidate_id);
        }
        Ok(RequestVoteResponse {
            term: inner.term,
            vote_granted: granted,
        })
    }

    async fn install_snapshot_chunk(
        &self,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck> {
        let mut inner = self.inner.lock().await;
        if request.term < inner.term {
            return Ok(InstallSnapshotAck {
                term: inner.term,
                success: false,
                bytes_accepted: 0,
                next_offset: 0,
                done: false,
            });
        }
        if request.term > inner.term {
            inner.term = request.term;
            inner.voted_for = None;
        }

        let key = (request.partition_id, request.last_index, request.last_term);
        let accepted = inner.partial_snapshots.get(&key).map(Vec::len).unwrap_or(0) as u64;
        if request.offset > accepted {
            return Err(RaftError::snapshot_stale(format!(
                "offset {} is beyond accepted {}",
                request.offset, accepted
            )));
        }
        if request.offset == accepted {
            inner
                .partial_snapshots
                .entry(key)
                .or_default()
                .extend_from_slice(&request.chunk);
        }
        let bytes_accepted = inner.partial_snapshots.get(&key).map(Vec::len).unwrap_or(0) as u64;
        let done = request.done;

        if done {
            let data = inner.partial_snapshots.remove(&key).unwrap_or_default();
            inner.snapshot = data;
            inner.snapshot_last_index = request.last_index;
            inner.snapshot_last_term = request.last_term;
            inner.log.retain(|entry| entry.index > request.last_index);
            inner.commit_index = inner.commit_index.max(request.last_index);
        }

        if self.fail_next_snapshot.swap(false, Ordering::SeqCst) {
            return Err(RaftError::unavailable(
                "injected mid-stream snapshot failure",
            ));
        }

        Ok(InstallSnapshotAck {
            term: inner.term,
            success: true,
            bytes_accepted,
            next_offset: bytes_accepted,
            done,
        })
    }

    fn leader_hint(&self, _partition_id: u64) -> Option<LeaderHint> {
        let inner = self.inner.try_lock().ok()?;
        let id = inner.leader_id?;
        if id == self.id {
            return Some(LeaderHint::new(self.id, self.addr.clone()));
        }
        self.peers
            .iter()
            .find(|peer| peer.node_id == id)
            .map(|peer| LeaderHint::new(id, peer.addr.clone()))
    }
}

pub struct RunningNode {
    pub addr: String,
    pub node: Arc<TestNode>,
    handle: JoinHandle<()>,
    _server: TquicServer,
}

impl RunningNode {
    pub async fn stop(self) {
        self.handle.abort();
        let _ = self.handle.await;
        drop(self._server);
    }
}

pub async fn spawn_node(
    tls: &TestTls,
    addr: &str,
    node: Arc<TestNode>,
    config: QuicRaftConfig,
) -> RunningNode {
    let socket: SocketAddr = addr.parse().expect("socket addr");
    let server = TquicServer::new(socket, tls.cert_file.clone(), tls.key_file.clone(), None)
        .await
        .expect("server");
    let listener = server.listen(addr).await.expect("listener");
    let service: Arc<dyn RaftService> = node.clone();
    let handle = tokio::spawn(async move {
        let _ = serve_listener(listener, service, config).await;
    });
    RunningNode {
        addr: addr.to_string(),
        node,
        handle,
        _server: server,
    }
}

pub fn peer(node: &Arc<TestNode>) -> RaftPeer {
    RaftPeer::new(node.id, node.addr.clone())
}
