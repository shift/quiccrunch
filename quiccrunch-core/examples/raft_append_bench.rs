//! AppendEntries round-trip benchmark for the QuicCrunch Raft transport.
//!
//! Three servers on loopback with TLS enabled. The service is a transport
//! echo: it performs a Raft-shaped append-ack without a durable log, so the
//! numbers describe the RPC surface, not disk/consensus work.
//!
//! Output is one CSV row per run:
//! `run,load1,ops,p50_us,p95_us,p99_us,errors`

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

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

struct EchoService;

#[async_trait]
impl RaftService for EchoService {
    async fn append_entries(
        &self,
        request: AppendEntriesRequest,
    ) -> RaftResult<AppendEntriesResponse> {
        let _ = request;
        Ok(AppendEntriesResponse {
            term: 1,
            success: true,
            match_index: 1,
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

fn write_certs(dir: &std::path::Path) -> (String, String, String) {
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

    let ca_path = dir.join("ca.pem");
    let cert_path = dir.join("node.pem");
    let key_path = dir.join("node.key");
    std::fs::write(&ca_path, ca.serialize_pem().unwrap()).unwrap();
    std::fs::write(&cert_path, leaf.serialize_pem_with_signer(&ca).unwrap()).unwrap();
    std::fs::write(&key_path, leaf.serialize_private_key_pem()).unwrap();
    (
        ca_path.to_string_lossy().into_owned(),
        cert_path.to_string_lossy().into_owned(),
        key_path.to_string_lossy().into_owned(),
    )
}

fn free_port() -> u16 {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let duration = std::env::var("BENCH_DURATION_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(5));
    let payload_len = std::env::var("BENCH_PAYLOAD")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(512);

    let dir = tempfile::tempdir().unwrap();
    let (ca_file, cert_file, key_file) = write_certs(dir.path());
    let addrs: Vec<String> = (0..3)
        .map(|_| format!("127.0.0.1:{}", free_port()))
        .collect();

    let server_config = QuicRaftConfig::default();
    let mut servers = Vec::new();
    for addr in &addrs {
        let socket: SocketAddr = addr.parse().unwrap();
        let server = TquicServer::new(socket, cert_file.clone(), key_file.clone(), None)
            .await
            .unwrap();
        let listener = server.listen(addr).await.unwrap();
        let service: Arc<dyn RaftService> = Arc::new(EchoService);
        let server_config = server_config.clone();
        tokio::spawn(async move {
            let _ = serve_listener(listener, service, server_config).await;
        });
        servers.push(server);
    }
    tokio::time::sleep(Duration::from_millis(100)).await;

    let transport = QuicRaftTransport::with_runtime_options(
        RuntimeOptions {
            verify_server_cert: true,
            ca_cert_file: Some(ca_file),
            ..RuntimeOptions::default()
        },
        QuicRaftConfig {
            rpc_timeout: Duration::from_secs(5),
            ..QuicRaftConfig::default()
        },
    )
    .await
    .unwrap();
    let peers: Vec<RaftPeer> = addrs
        .iter()
        .enumerate()
        .map(|(id, addr)| RaftPeer::new(id as u64, addr.clone()))
        .collect();

    // Warm up TLS, connection pooling and the token bucket of the server.
    for peer in &peers {
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
    tokio::time::sleep(Duration::from_millis(250)).await;

    let mut histogram = Histogram::<u64>::new(3).unwrap();
    let mut ops = 0u64;
    let mut errors = 0u64;
    let deadline = Instant::now() + duration;
    let mut next_peer = 0usize;
    while Instant::now() < deadline {
        let peer = &peers[next_peer % peers.len()];
        next_peer += 1;
        let request = AppendEntriesRequest {
            partition_id: 0,
            term: 1,
            leader_id: 0,
            prev_log_index: 0,
            prev_log_term: 0,
            entries: vec![LogEntry {
                term: 1,
                index: ops + 1,
                data: vec![0x5a; payload_len],
            }],
            leader_commit: 0,
        };
        let started = Instant::now();
        match transport.append_entries(peer, request).await {
            Ok(_) => {
                let micros = started.elapsed().as_micros().max(1) as u64;
                let _ = histogram.record(micros);
                ops += 1;
            }
            Err(_) => errors += 1,
        }
    }

    let load = std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "?".into());
    let rate = ops as f64 / duration.as_secs_f64();
    println!(
        "quic,{load},{ops},{rate:.0},{p50},{p95},{p99},{errors}",
        p50 = histogram.value_at_quantile(0.50),
        p95 = histogram.value_at_quantile(0.95),
        p99 = histogram.value_at_quantile(0.99)
    );
    let _ = servers;
}
