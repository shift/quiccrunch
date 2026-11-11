mod raft_common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use quiccrunch_core::raft::{
    AppendEntriesRequest, ErrorCode, InstallSnapshotChunk, QuicRaftConfig, QuicRaftTransport,
    RaftPeer, RaftTransport, RequestVoteRequest,
};
use raft_common::{peer, spawn_node, RunningNode, TestNode, TestTls};
use tokio::time::sleep;

fn base_config() -> QuicRaftConfig {
    QuicRaftConfig {
        rpc_timeout: Duration::from_secs(2),
        connect_timeout: Duration::from_secs(1),
        max_in_flight_per_peer: 32,
        queue_wait: Duration::from_millis(25),
        max_connect_attempts: 8,
        reconnect_base: Duration::from_millis(10),
        reconnect_max: Duration::from_millis(200),
    }
}

fn free_port() -> u16 {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind udp");
    socket.local_addr().unwrap().port()
}

async fn start_cluster(
    tls: &TestTls,
    nodes: usize,
    server_config: QuicRaftConfig,
    client_config: QuicRaftConfig,
) -> (Vec<RunningNode>, Arc<QuicRaftTransport>, Vec<RaftPeer>) {
    let addrs: Vec<String> = (0..nodes)
        .map(|_| format!("127.0.0.1:{}", free_port()))
        .collect();
    let all_peers: Vec<RaftPeer> = addrs
        .iter()
        .enumerate()
        .map(|(id, addr)| RaftPeer::new(id as u64, addr.clone()))
        .collect();
    let mut running = Vec::new();
    for (id, addr) in addrs.iter().enumerate().take(nodes) {
        let others = all_peers
            .iter()
            .filter(|peer| peer.node_id != id as u64)
            .cloned()
            .collect();
        let node = TestNode::new(id as u64, 0, addr.clone(), others);
        running.push(spawn_node(tls, addr, node, server_config.clone()).await);
    }
    sleep(Duration::from_millis(100)).await;
    let transport = Arc::new(
        QuicRaftTransport::with_runtime_options(tls.client_options(), client_config)
            .await
            .expect("transport"),
    );
    (running, transport, all_peers)
}

fn others(all: &[RaftPeer], id: u64) -> Vec<RaftPeer> {
    all.iter()
        .filter(|peer| peer.node_id != id)
        .cloned()
        .collect()
}

async fn replicate_with_reconnect(
    leader: &Arc<TestNode>,
    transport: &dyn RaftTransport,
    peers: &[RaftPeer],
) {
    for _ in 0..10 {
        match leader.replicate_round(transport, peers).await {
            Ok(()) => return,
            Err(err) if err.retryable => sleep(Duration::from_millis(100)).await,
            Err(err) => panic!("replication failed: {err}"),
        }
    }
    panic!("replication did not converge");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_node_election_append_and_follower_restart() {
    let tls = TestTls::new();
    let (mut running, transport, all_peers) =
        start_cluster(&tls, 3, base_config(), base_config()).await;

    let leader = running[0].node.clone();
    let leader_id = leader.id;
    assert!(
        leader
            .campaign(transport.as_ref(), &others(&all_peers, leader_id))
            .await
    );
    assert_eq!(leader.role().await, raft_common::Role::Leader);

    for i in 0..5u8 {
        let data = vec![i; 128];
        leader
            .propose(transport.as_ref(), &others(&all_peers, leader_id), data)
            .await
            .expect("propose");
    }
    for node in &running[1..] {
        assert_eq!(node.node.log_len().await, 5);
    }

    // Restart follower 2 with an empty log on a fresh listener address. A new
    // address avoids a loopback SO_REUSEPORT race with the old runtime socket;
    // reconnect-after-failure is covered separately by the snapshot test.
    let stopped = running.remove(2);
    stopped.stop().await;
    sleep(Duration::from_millis(100)).await;

    let addr = format!("127.0.0.1:{}", free_port());
    let peers_for_restart = others(&all_peers, 2);
    let restarted = TestNode::new(2, 0, addr.clone(), peers_for_restart);
    let restarted_node = restarted.clone();
    let restarted_handle = spawn_node(&tls, &addr, restarted, base_config()).await;
    sleep(Duration::from_millis(100)).await;

    let replication_targets: Vec<RaftPeer> = all_peers
        .iter()
        .filter(|peer| peer.node_id != leader_id && peer.node_id != 2)
        .cloned()
        .chain(std::iter::once(RaftPeer::new(2, addr.clone())))
        .collect();

    // Wait until the restarted listener is reachable.
    let restarted_peer = RaftPeer::new(2, addr.clone());
    let mut ready = false;
    for _ in 0..30 {
        let probe = RequestVoteRequest {
            partition_id: 0,
            term: leader.term().await,
            candidate_id: 99,
            last_log_index: 0,
            last_log_term: 0,
        };
        if transport.request_vote(&restarted_peer, probe).await.is_ok() {
            ready = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(ready, "restarted follower never became reachable");

    match leader
        .propose(
            transport.as_ref(),
            &replication_targets,
            b"after-restart".to_vec(),
        )
        .await
    {
        Ok(()) => {}
        Err(err) if err.retryable => {}
        Err(err) => panic!("propose after restart failed: {err}"),
    }
    replicate_with_reconnect(&leader, transport.as_ref(), &replication_targets).await;

    assert_eq!(
        restarted_node.log_len().await,
        6,
        "restarted follower log: {:?}",
        restarted_node.log_data().await
    );
    assert_eq!(
        restarted_node.log_data().await.last().unwrap(),
        &b"after-restart".to_vec()
    );
    restarted_handle.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn snapshot_stream_resumes_after_injected_mid_stream_failure() {
    let tls = TestTls::new();
    let (running, transport, all_peers) =
        start_cluster(&tls, 2, base_config(), base_config()).await;

    let leader = running[0].node.clone();
    let follower = running[1].node.clone();
    let leader_id = leader.id;
    assert!(
        leader
            .campaign(transport.as_ref(), &others(&all_peers, leader_id))
            .await
    );

    // 200 KiB of a deterministic pattern makes truncation/offset corruption
    // visible.
    let data: Vec<u8> = (0..200 * 1024).map(|i| (i % 251) as u8).collect();
    follower
        .fail_next_snapshot
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let ack = leader
        .install_snapshot_from(transport.as_ref(), &peer(&follower), data.clone(), 500, 7)
        .await
        .expect("snapshot install should resume after the injected failure");

    assert!(ack.success);
    assert!(ack.done);
    assert_eq!(ack.next_offset, data.len() as u64);
    assert_eq!(follower.snapshot().await, data);
    assert_eq!(follower.snapshot_base().await, (500, 7));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn not_leader_returns_leader_hint_and_retry_succeeds() {
    let tls = TestTls::new();
    let (running, transport, all_peers) =
        start_cluster(&tls, 2, base_config(), base_config()).await;

    let leader = running[0].node.clone();
    let follower = running[1].node.clone();
    let leader_id = leader.id;
    assert!(
        leader
            .campaign(transport.as_ref(), &others(&all_peers, leader_id))
            .await
    );
    leader
        .propose(
            transport.as_ref(),
            &others(&all_peers, leader_id),
            b"heartbeat".to_vec(),
        )
        .await
        .expect("heartbeat");

    follower.set_redirect(true).await;
    let request = AppendEntriesRequest {
        partition_id: 0,
        term: leader.term().await,
        leader_id: 99,
        prev_log_index: follower.log_len().await as u64,
        prev_log_term: 1,
        entries: vec![],
        leader_commit: 0,
    };
    let err = transport
        .append_entries(&peer(&follower), request.clone())
        .await
        .expect_err("follower must redirect");
    assert_eq!(err.code, ErrorCode::NotLeader);
    let hint = err.leader_hint.expect("leader hint");
    assert_eq!(hint.leader_id, leader_id);
    assert_eq!(hint.leader_addr, running[0].addr);

    // Retry directly against the hinted leader without a discovery round trip.
    let retry_peer = RaftPeer::new(hint.leader_id, hint.leader_addr);
    let response = transport
        .append_entries(&retry_peer, request)
        .await
        .expect("retry to leader");
    assert!(response.success);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deadlines_are_enforced_on_client_and_server() {
    let tls = TestTls::new();
    let slow_server = QuicRaftConfig {
        rpc_timeout: Duration::from_millis(100),
        ..base_config()
    };
    let patient_client = QuicRaftConfig {
        rpc_timeout: Duration::from_secs(2),
        ..base_config()
    };
    let (running, transport, all_peers) = start_cluster(&tls, 2, slow_server, patient_client).await;
    let target = running[1].node.clone();
    target.set_vote_delay(Duration::from_millis(500)).await;

    let started = Instant::now();
    let err = transport
        .request_vote(
            &peer(&target),
            RequestVoteRequest {
                partition_id: 0,
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
        )
        .await
        .expect_err("server deadline");
    assert_eq!(err.code, ErrorCode::DeadlineExceeded);
    assert!(started.elapsed() < Duration::from_millis(400));

    // Client-side deadline: the server is patient, the caller is not.
    let client_config = QuicRaftConfig {
        rpc_timeout: Duration::from_millis(100),
        ..base_config()
    };
    let impatient = QuicRaftTransport::with_runtime_options(tls.client_options(), client_config)
        .await
        .expect("client");
    let started = Instant::now();
    let err = impatient
        .request_vote(
            &peer(&running[1].node),
            RequestVoteRequest {
                partition_id: 0,
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
        )
        .await
        .expect_err("client deadline");
    assert_eq!(err.code, ErrorCode::DeadlineExceeded);
    assert!(started.elapsed() < Duration::from_millis(400));
    let _ = all_peers;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_peer_sheds_with_backpressure_and_reports_queue_metrics() {
    let tls = TestTls::new();
    let config = QuicRaftConfig {
        max_in_flight_per_peer: 1,
        queue_wait: Duration::from_millis(20),
        rpc_timeout: Duration::from_secs(2),
        ..base_config()
    };
    let (running, transport, all_peers) = start_cluster(&tls, 2, config.clone(), config).await;
    let target = running[1].node.clone();
    target.set_append_delay(0, Duration::from_millis(300)).await;

    // Warm the shared connection and prove the fast path.
    transport
        .request_vote(
            &peer(&target),
            RequestVoteRequest {
                partition_id: 0,
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
        )
        .await
        .expect("warmup");

    let request = AppendEntriesRequest {
        partition_id: 0,
        term: 1,
        leader_id: 0,
        prev_log_index: 0,
        prev_log_term: 0,
        entries: vec![],
        leader_commit: 0,
    };
    let first = {
        let transport = transport.clone();
        let target = peer(&target);
        let request = request.clone();
        tokio::spawn(async move { transport.append_entries(&target, request).await })
    };
    sleep(Duration::from_millis(40)).await;
    let second = transport
        .append_entries(&peer(&target), request)
        .await
        .expect_err("second call should be shed");
    assert_eq!(second.code, ErrorCode::Backpressure);
    assert!(second.retryable);

    let metrics = transport.metrics();
    assert!(metrics.shed_total >= 1);
    assert!(metrics.max_queue_depth >= 1);
    assert!(first.await.unwrap().is_ok());
    let _ = all_peers;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_partitions_share_a_connection_without_hol_blocking() {
    let tls = TestTls::new();
    let (running, transport, all_peers) =
        start_cluster(&tls, 2, base_config(), base_config()).await;
    let target = running[1].node.clone();
    target.set_append_delay(1, Duration::from_millis(350)).await;

    // Warm the connection.
    transport
        .request_vote(
            &peer(&target),
            RequestVoteRequest {
                partition_id: 0,
                term: 1,
                candidate_id: 0,
                last_log_index: 0,
                last_log_term: 0,
            },
        )
        .await
        .expect("warmup");

    let request = |partition_id| AppendEntriesRequest {
        partition_id,
        term: 1,
        leader_id: 0,
        prev_log_index: 0,
        prev_log_term: 0,
        entries: vec![],
        leader_commit: 0,
    };

    let transport_slow = transport.clone();
    let slow_peer = peer(&target);
    let slow_request = request(1);
    let slow = tokio::spawn(async move {
        let started = Instant::now();
        let result = transport_slow
            .append_entries(&slow_peer, slow_request)
            .await;
        (result, started.elapsed())
    });
    sleep(Duration::from_millis(30)).await;

    let started = Instant::now();
    transport
        .append_entries(&peer(&target), request(2))
        .await
        .expect("fast partition");
    let fast_elapsed = started.elapsed();

    let (slow_result, slow_elapsed) = slow.await.unwrap();
    slow_result.expect("slow partition");
    assert!(
        fast_elapsed < Duration::from_millis(200),
        "fast partition blocked behind slow one: {fast_elapsed:?}"
    );
    assert!(
        slow_elapsed >= Duration::from_millis(300),
        "slow partition did not actually delay: {slow_elapsed:?}"
    );
    let _ = all_peers;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn large_append_entries_round_trip_over_tls() {
    let tls = TestTls::new();
    let (running, transport, all_peers) =
        start_cluster(&tls, 2, base_config(), base_config()).await;
    let leader = running[0].node.clone();
    let follower = running[1].node.clone();
    let leader_id = leader.id;
    assert!(
        leader
            .campaign(transport.as_ref(), &others(&all_peers, leader_id))
            .await
    );

    let payload: Vec<u8> = (0..1024 * 1024).map(|i| (i % 253) as u8).collect();
    leader
        .propose(
            transport.as_ref(),
            &others(&all_peers, leader_id),
            payload.clone(),
        )
        .await
        .expect("1 MiB entry");
    assert_eq!(follower.log_data().await[0], payload);

    let oversized = InstallSnapshotChunk {
        partition_id: 0,
        term: 1,
        leader_id: 0,
        last_index: 0,
        last_term: 0,
        offset: 0,
        chunk: vec![0u8; quiccrunch_core::raft::MAX_SNAPSHOT_CHUNK_BYTES + 1],
        done: false,
    };
    let err = transport
        .install_snapshot_chunk(&peer(&follower), oversized)
        .await
        .expect_err("oversized chunk must not be truncated");
    assert_eq!(err.code, ErrorCode::FrameTooLarge);
}
