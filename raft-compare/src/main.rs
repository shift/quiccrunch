//! AppendEntries benchmark: tonic (HTTP/2 over TLS) vs QuicCrunch, same
//! logical Raft contract, same bincode command bytes, same 3-node loopback
//! topology and client instrumentation.
//!
//! Usage: `raft-compare <tonic|quic> [duration_ms]`
//! Emits one CSV row per (payload, depth) cell.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use hdrhistogram::Histogram;
use quiccrunch_core::raft::{
    serve_listener, AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotAck,
    InstallSnapshotChunk, LogEntry, QuicRaftConfig, QuicRaftTransport, RaftPeer, RaftResult,
    RaftService, RaftTransport, RequestVoteRequest, RequestVoteResponse,
};
use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicServer};
use rcgen::{
    date_time_ymd, BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    IsCa, KeyUsagePurpose, SanType, PKCS_ECDSA_P256_SHA256,
};
use serde::{Deserialize, Serialize};
use tonic::transport::{
    Certificate as TonicCertificate, Channel, ClientTlsConfig, Identity, Server, ServerTlsConfig,
};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("raftbench");
}

#[derive(Serialize, Deserialize, Clone)]
struct BenchCommand {
    seq: u64,
    term: u64,
    payload: Vec<u8>,
}

struct ServiceStats {
    hist: Mutex<Histogram<u64>>,
}

impl ServiceStats {
    fn new() -> Self {
        Self {
            hist: Mutex::new(Histogram::<u64>::new(3).unwrap()),
        }
    }

    fn record(&self, elapsed: Duration) {
        let _ = self
            .hist
            .lock()
            .unwrap()
            .record(elapsed.as_micros().max(1) as u64);
    }

    fn reset(&self) {
        self.hist.lock().unwrap().clear();
    }

    fn p99(&self) -> u64 {
        self.hist.lock().unwrap().value_at_quantile(0.99)
    }
}

struct Certs {
    _dir: tempfile::TempDir,
    ca_path: String,
    cert_path: String,
    key_path: String,
    ca_pem: Vec<u8>,
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
}

fn make_certs() -> Certs {
    let dir = tempfile::tempdir().unwrap();
    let mut ca_params = CertificateParams::new(vec!["Raft Bench CA".to_string()]);
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.alg = &PKCS_ECDSA_P256_SHA256;
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::CrlSign,
    ];
    ca_params.not_before = date_time_ymd(2020, 1, 1);
    ca_params.not_after = date_time_ymd(2050, 1, 1);
    let ca = Certificate::from_params(ca_params).unwrap();

    let mut leaf_params =
        CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()]);
    leaf_params.distinguished_name = DistinguishedName::new();
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "raft-bench-node");
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
    let leaf = Certificate::from_params(leaf_params).unwrap();

    let ca_path = dir.path().join("ca.pem");
    let cert_path = dir.path().join("node.pem");
    let key_path = dir.path().join("node.key");
    let ca_pem = ca.serialize_pem().unwrap();
    let cert_pem = leaf.serialize_pem_with_signer(&ca).unwrap();
    let key_pem = leaf.serialize_private_key_pem();
    std::fs::write(&ca_path, &ca_pem).unwrap();
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, &key_pem).unwrap();
    Certs {
        _dir: dir,
        ca_path: ca_path.to_string_lossy().into_owned(),
        cert_path: cert_path.to_string_lossy().into_owned(),
        key_path: key_path.to_string_lossy().into_owned(),
        ca_pem: ca_pem.into_bytes(),
        cert_pem: cert_pem.into_bytes(),
        key_pem: key_pem.into_bytes(),
    }
}

fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn load1() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "?".into())
}

fn command_bytes(target: usize, seq: u64) -> Vec<u8> {
    let overhead = 24;
    let mut bytes = bincode::serialize(&BenchCommand {
        seq,
        term: 1,
        payload: vec![0x5a; target.saturating_sub(overhead)],
    })
    .unwrap();
    bytes.resize(target, 0);
    bytes
}

struct EchoQuic {
    stats: Arc<ServiceStats>,
}

#[async_trait]
impl RaftService for EchoQuic {
    async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse> {
        let started = Instant::now();
        let last = request
            .entries
            .last()
            .map(|entry| entry.index)
            .unwrap_or(request.prev_log_index);
        self.stats.record(started.elapsed());
        Ok(AppendEntriesResponse {
            term: request.term,
            success: true,
            match_index: last,
            conflict_index: 0,
            conflict_term: 0,
        })
    }

    async fn request_vote(&self, request: RequestVoteRequest) -> RaftResult<RequestVoteResponse> {
        Ok(RequestVoteResponse {
            term: request.term,
            vote_granted: true,
        })
    }

    async fn install_snapshot_chunk(
        &self,
        request: InstallSnapshotChunk,
    ) -> RaftResult<InstallSnapshotAck> {
        let next = request.offset + request.chunk.len() as u64;
        Ok(InstallSnapshotAck {
            term: request.term,
            success: true,
            bytes_accepted: next,
            next_offset: next,
            done: request.done,
        })
    }
}

struct EchoTonic {
    stats: Arc<ServiceStats>,
}

#[tonic::async_trait]
impl pb::raft_bench_server::RaftBench for EchoTonic {
    async fn append_entries(
        &self,
        request: Request<pb::AppendEntriesRequest>,
    ) -> Result<Response<pb::AppendEntriesResponse>, Status> {
        let started = Instant::now();
        let req = request.into_inner();
        let last = req
            .entries
            .last()
            .map(|entry| entry.index)
            .unwrap_or(req.prev_log_index);
        self.stats.record(started.elapsed());
        Ok(Response::new(pb::AppendEntriesResponse {
            term: req.term,
            success: true,
            match_index: last,
            conflict_index: 0,
            conflict_term: 0,
        }))
    }

    async fn request_vote(
        &self,
        request: Request<pb::RequestVoteRequest>,
    ) -> Result<Response<pb::RequestVoteResponse>, Status> {
        let req = request.into_inner();
        Ok(Response::new(pb::RequestVoteResponse {
            term: req.term,
            vote_granted: true,
        }))
    }
}

struct ServerHandles {
    quic: Vec<(TquicServer, tokio::task::JoinHandle<()>)>,
    tonic: Vec<tokio::task::JoinHandle<()>>,
}

impl ServerHandles {
    fn stop(self) {
        for (_, handle) in self.quic {
            handle.abort();
        }
        for handle in self.tonic {
            handle.abort();
        }
    }
}

async fn start_quic_servers(
    certs: &Certs,
    addrs: &[String],
    stats: Arc<ServiceStats>,
) -> Result<Vec<(TquicServer, tokio::task::JoinHandle<()>)>> {
    let mut out = Vec::new();
    for addr in addrs {
        let socket: SocketAddr = addr.parse().unwrap();
        let server = TquicServer::new(
            socket,
            certs.cert_path.clone(),
            certs.key_path.clone(),
            None,
        )
        .await
        .context("quic server")?;
        let listener = server.listen(addr).await.context("quic listen")?;
        let service: Arc<dyn RaftService> = Arc::new(EchoQuic {
            stats: stats.clone(),
        });
        let config = QuicRaftConfig::default();
        let handle = tokio::spawn(async move {
            let _ = serve_listener(listener, service, config).await;
        });
        out.push((server, handle));
    }
    Ok(out)
}

async fn start_tonic_servers(
    certs: &Certs,
    addrs: &[String],
    stats: Arc<ServiceStats>,
) -> Result<Vec<tokio::task::JoinHandle<()>>> {
    let mut out = Vec::new();
    for addr in addrs {
        let socket: SocketAddr = addr.parse().unwrap();
        let tls = ServerTlsConfig::new().identity(Identity::from_pem(
            certs.cert_pem.clone(),
            certs.key_pem.clone(),
        ));
        let service = pb::raft_bench_server::RaftBenchServer::new(EchoTonic {
            stats: stats.clone(),
        });
        let handle = tokio::spawn(async move {
            let _ = Server::builder()
                .tls_config(tls)
                .unwrap()
                .initial_stream_window_size(16 * 1024 * 1024)
                .initial_connection_window_size(128 * 1024 * 1024)
                .add_service(service)
                .serve(socket)
                .await;
        });
        out.push(handle);
    }
    Ok(out)
}

struct Cell {
    payload: usize,
    depth: usize,
}

struct RunAccum {
    ops: u64,
    errors: u64,
    rtt: Histogram<u64>,
    e2e: Histogram<u64>,
    enqueue: Histogram<u64>,
}

impl RunAccum {
    fn new() -> Self {
        Self {
            ops: 0,
            errors: 0,
            rtt: Histogram::<u64>::new(3).unwrap(),
            e2e: Histogram::<u64>::new(3).unwrap(),
            enqueue: Histogram::<u64>::new(3).unwrap(),
        }
    }
}

async fn run_quic_cell(
    transport: Arc<QuicRaftTransport>,
    peers: Vec<RaftPeer>,
    command: Arc<Vec<u8>>,
    depth: usize,
    duration: Duration,
    sched_late: Arc<Mutex<Histogram<u64>>>,
    seq: Arc<AtomicU64>,
) -> RunAccum {
    let deadline = Instant::now() + duration;
    let mut workers = Vec::new();
    for worker in 0..depth {
        let transport = transport.clone();
        let peer = peers[worker % peers.len()].clone();
        let command = command.clone();
        let seq = seq.clone();
        workers.push(tokio::spawn(async move {
            let mut acc = RunAccum::new();
            let mut client_seq = 0u64;
            while Instant::now() < deadline {
                client_seq += 1;
                let index = seq.fetch_add(1, Ordering::Relaxed) + 1;
                let enqueue = Instant::now();
                let request = AppendEntriesRequest {
                    partition_id: 0,
                    term: 1,
                    leader_id: 0,
                    prev_log_index: index.saturating_sub(1),
                    prev_log_term: 0,
                    entries: vec![LogEntry {
                        term: 1,
                        index,
                        data: (*command).clone(),
                    }],
                    leader_commit: index.saturating_sub(1),
                };
                let write = Instant::now();
                let result = transport.append_entries(&peer, request).await;
                let response = Instant::now();
                match result {
                    Ok(_) => {
                        acc.ops += 1;
                        let _ = acc.rtt.record((response - write).as_micros().max(1) as u64);
                        let _ = acc
                            .e2e
                            .record((response - enqueue).as_micros().max(1) as u64);
                        let _ = acc.enqueue.record((write - enqueue).as_micros() as u64);
                    }
                    Err(_) => acc.errors += 1,
                }
                let _ = client_seq;
            }
            acc
        }));
    }

    let heartbeat = spawn_scheduler_heartbeat(sched_late);
    let mut total = RunAccum::new();
    for worker in workers {
        let acc = worker.await.unwrap();
        total.ops += acc.ops;
        total.errors += acc.errors;
        total.rtt.add(&acc.rtt).unwrap();
        total.e2e.add(&acc.e2e).unwrap();
        total.enqueue.add(&acc.enqueue).unwrap();
    }
    heartbeat.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(100)).await;
    total
}

async fn run_tonic_cell(
    channels: Vec<Channel>,
    command: Arc<Vec<u8>>,
    depth: usize,
    duration: Duration,
    sched_late: Arc<Mutex<Histogram<u64>>>,
    seq: Arc<AtomicU64>,
) -> RunAccum {
    let deadline = Instant::now() + duration;
    let mut workers = Vec::new();
    for worker in 0..depth {
        let channel = channels[worker % channels.len()].clone();
        let command = command.clone();
        let seq = seq.clone();
        workers.push(tokio::spawn(async move {
            let mut client = pb::raft_bench_client::RaftBenchClient::new(channel);
            let mut acc = RunAccum::new();
            while Instant::now() < deadline {
                let index = seq.fetch_add(1, Ordering::Relaxed) + 1;
                let enqueue = Instant::now();
                let request = pb::AppendEntriesRequest {
                    partition_id: 0,
                    term: 1,
                    leader_id: 0,
                    prev_log_index: index.saturating_sub(1),
                    prev_log_term: 0,
                    entries: vec![pb::RaftLogEntry {
                        term: 1,
                        index,
                        command: (*command).clone(),
                    }],
                    leader_commit: index.saturating_sub(1),
                };
                let write = Instant::now();
                let result = client.append_entries(request).await;
                let response = Instant::now();
                match result {
                    Ok(_) => {
                        acc.ops += 1;
                        let _ = acc.rtt.record((response - write).as_micros().max(1) as u64);
                        let _ = acc
                            .e2e
                            .record((response - enqueue).as_micros().max(1) as u64);
                        let _ = acc.enqueue.record((write - enqueue).as_micros() as u64);
                    }
                    Err(_) => acc.errors += 1,
                }
            }
            acc
        }));
    }

    let heartbeat = spawn_scheduler_heartbeat(sched_late);
    let mut total = RunAccum::new();
    for worker in workers {
        let acc = worker.await.unwrap();
        total.ops += acc.ops;
        total.errors += acc.errors;
        total.rtt.add(&acc.rtt).unwrap();
        total.e2e.add(&acc.e2e).unwrap();
        total.enqueue.add(&acc.enqueue).unwrap();
    }
    heartbeat.store(false, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(100)).await;
    total
}

fn spawn_scheduler_heartbeat(late: Arc<Mutex<Histogram<u64>>>) -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let flag = running.clone();
    tokio::spawn(async move {
        while flag.load(Ordering::Relaxed) {
            let started = Instant::now();
            tokio::time::sleep(Duration::from_millis(1)).await;
            let elapsed = started.elapsed();
            let late_us = elapsed.saturating_sub(Duration::from_millis(1)).as_micros() as u64;
            let _ = late.lock().unwrap().record(late_us.max(1));
        }
    });
    running
}

fn print_header() {
    println!(
        "repeat,transport,payload_bytes,depth,ops,rate_per_s,p50_us,p95_us,p99_us,e2e_p99_us,enqueue_p99_us,server_p99_us,sched_late_p99_us,load_start,load_end,errors"
    );
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let transport = args.get(1).map(String::as_str).unwrap_or("both");
    let duration_ms = args
        .get(2)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(3000);
    let duration = Duration::from_millis(duration_ms);
    let certs = make_certs();
    let addrs: Vec<String> = (0..3)
        .map(|_| format!("127.0.0.1:{}", free_port()))
        .collect();
    let stats = Arc::new(ServiceStats::new());
    let sched_late = Arc::new(Mutex::new(Histogram::<u64>::new(3).unwrap()));
    let seq = Arc::new(AtomicU64::new(0));
    let cells = [
        Cell {
            payload: 512,
            depth: 1,
        },
        Cell {
            payload: 512,
            depth: 3,
        },
        Cell {
            payload: 512,
            depth: 5,
        },
        Cell {
            payload: 4096,
            depth: 1,
        },
        Cell {
            payload: 4096,
            depth: 3,
        },
        Cell {
            payload: 4096,
            depth: 5,
        },
    ];

    let mut handles = ServerHandles {
        quic: Vec::new(),
        tonic: Vec::new(),
    };
    if transport == "quic" {
        handles.quic = start_quic_servers(&certs, &addrs, stats.clone()).await?;
    } else if transport == "tonic" {
        handles.tonic = start_tonic_servers(&certs, &addrs, stats.clone()).await?;
    } else {
        anyhow::bail!("usage: raft-compare <tonic|quic> [duration_ms]");
    }
    tokio::time::sleep(Duration::from_millis(250)).await;

    let quic_transport = if transport == "quic" {
        Some(Arc::new(
            QuicRaftTransport::with_runtime_options(
                RuntimeOptions {
                    verify_server_cert: true,
                    ca_cert_file: Some(certs.ca_path.clone()),
                    ..RuntimeOptions::default()
                },
                QuicRaftConfig {
                    rpc_timeout: Duration::from_secs(5),
                    ..QuicRaftConfig::default()
                },
            )
            .await?,
        ))
    } else {
        None
    };
    let quic_peers: Vec<RaftPeer> = addrs
        .iter()
        .enumerate()
        .map(|(id, addr)| RaftPeer::new(id as u64, addr.clone()))
        .collect();

    let mut channels = Vec::new();
    if transport == "tonic" {
        for addr in &addrs {
            let tls = ClientTlsConfig::new()
                .ca_certificate(TonicCertificate::from_pem(certs.ca_pem.clone()))
                .domain_name("localhost");
            let channel = Channel::from_shared(format!("https://{addr}"))?
                .tls_config(tls)?
                .initial_stream_window_size(16 * 1024 * 1024)
                .initial_connection_window_size(128 * 1024 * 1024)
                .connect()
                .await
                .context("tonic connect")?;
            channels.push(channel);
        }
    }

    print_header();
    for cell in &cells {
        let load_start = load1();
        stats.reset();
        sched_late.lock().unwrap().clear();
        let command = Arc::new(command_bytes(cell.payload, 0));

        // Warm both clocks and the TLS connection set.
        if let Some(transport) = &quic_transport {
            for peer in &quic_peers {
                let request = AppendEntriesRequest {
                    partition_id: 0,
                    term: 1,
                    leader_id: 0,
                    prev_log_index: 0,
                    prev_log_term: 0,
                    entries: vec![],
                    leader_commit: 0,
                };
                let _ = transport.append_entries(peer, request).await;
            }
        }
        if transport == "tonic" {
            for channel in &channels {
                let mut client = pb::raft_bench_client::RaftBenchClient::new(channel.clone());
                let _ = client
                    .append_entries(pb::AppendEntriesRequest {
                        partition_id: 0,
                        term: 1,
                        leader_id: 0,
                        prev_log_index: 0,
                        prev_log_term: 0,
                        entries: vec![],
                        leader_commit: 0,
                    })
                    .await;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;

        let acc = if let Some(transport) = &quic_transport {
            run_quic_cell(
                transport.clone(),
                quic_peers.clone(),
                command,
                cell.depth,
                duration,
                sched_late.clone(),
                seq.clone(),
            )
            .await
        } else {
            run_tonic_cell(
                channels.clone(),
                command,
                cell.depth,
                duration,
                sched_late.clone(),
                seq.clone(),
            )
            .await
        };

        let load_end = load1();
        let rate = acc.ops as f64 / duration.as_secs_f64();
        let sched_p99 = sched_late.lock().unwrap().value_at_quantile(0.99);
        println!(
            "{repeat},{transport},{payload},{depth},{ops},{rate:.0},{p50},{p95},{p99},{e2e},{enqueue},{server},{sched},{load_start},{load_end},{errors}",
            repeat = std::env::var("BENCH_REPEAT").unwrap_or_else(|_| "0".into()),
            payload = cell.payload,
            depth = cell.depth,
            ops = acc.ops,
            rate = rate,
            p50 = acc.rtt.value_at_quantile(0.50),
            p95 = acc.rtt.value_at_quantile(0.95),
            p99 = acc.rtt.value_at_quantile(0.99),
            e2e = acc.e2e.value_at_quantile(0.99),
            enqueue = acc.enqueue.value_at_quantile(0.99),
            server = stats.p99(),
            sched = sched_p99,
            load_start = load_start,
            load_end = load_end,
            errors = acc.errors,
        );
    }
    handles.stop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_payload_is_exact_and_round_trips() {
        for size in [512usize, 4096] {
            let bytes = command_bytes(size, 42);
            assert_eq!(bytes.len(), size);
            let decoded: BenchCommand = bincode::deserialize(&bytes).unwrap();
            assert_eq!(decoded.seq, 42);
            assert_eq!(decoded.term, 1);
        }
    }

    #[test]
    fn scheduler_lateness_histogram_is_usable() {
        let hist = Histogram::<u64>::new(3).unwrap();
        assert_eq!(hist.value_at_quantile(0.99), 0);
    }
}
