mod messaging_common;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use messaging_common::{identity, pool, retention, spawn_server, RunningServer, TestTls};
use quiccrunch_core::messaging::client::{
    ClientMetricsSnapshot, FrameIo, MessagingClient, MessagingPool, PolicyWatcher, PoolConfig,
};
use quiccrunch_core::messaging::retention::PolicyStore;
use quiccrunch_core::messaging::server::{MessagingServer, MessagingService, RequestContext};
use quiccrunch_core::messaging::{
    decode, encode, BackpressureConfig, Body, Event, EventIngest, EventIngested, Heartbeat,
    HeartbeatAck, LeaseAcquire, LeaseAcquired, LeaseRelease, LeaseReleased, Message,
    MessagingError, MessagingMetrics, PolicyBundle, PolicyGet, PolicyWatch, RequestPayload,
    ResponsePayload, RetentionConfig,
};
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

async fn publish_range(server: &RunningServer, from: u64, to: u64) {
    for version in from..to {
        server
            .server
            .publish("default", format!("bundle-{version}").into_bytes(), "sum");
    }
}

async fn next(watcher: &mut PolicyWatcher) -> quiccrunch_core::messaging::PolicyWatchEventDto {
    timeout(TEST_TIMEOUT, watcher.next_event())
        .await
        .expect("watch event timed out")
        .expect("watch event failed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn watch_resume_after_disconnect_has_no_gaps_or_duplicates() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        retention(2048, 8 * 1024 * 1024),
        metrics.clone(),
    ));
    let node_a = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        BackpressureConfig::default(),
        identity(),
    )
    .await;
    let node_b = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        BackpressureConfig::default(),
        identity(),
    )
    .await;

    // Publish on node A.
    publish_range(&node_a, 0, 5).await;

    let pool = pool(&tls, PoolConfig::default()).await;
    let mut watcher = PolicyWatcher::new(
        pool.clone(),
        node_b.url(),
        None,
        identity(),
        HashMap::new(),
        "default",
        0,
    );
    let mut first = Vec::new();
    for _ in 0..5 {
        first.push(next(&mut watcher).await.version);
    }
    assert_eq!(first, (1..=5).collect::<Vec<_>>());
    assert_eq!(watcher.last_applied(), 5);

    // Disconnect node B's subscriber, then publish 100 more on node A.
    drop(watcher);
    publish_range(&node_a, 5, 105).await;

    // Reconnect with the last applied version.
    let mut watcher = PolicyWatcher::new(
        pool.clone(),
        node_b.url(),
        None,
        identity(),
        HashMap::new(),
        "default",
        5,
    );
    let mut resumed = Vec::new();
    for _ in 0..100 {
        resumed.push(next(&mut watcher).await.version);
    }
    assert_eq!(resumed, (6..=105).collect::<Vec<_>>());
    assert!(
        resumed.windows(2).all(|pair| pair[0] < pair[1]),
        "versions must be strictly increasing"
    );
    assert_eq!(watcher.last_applied(), 105);

    node_a.stop().await;
    node_b.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eviction_returns_resume_too_old_and_snapshot_converges() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        retention(10, 1024 * 1024),
        metrics.clone(),
    ));
    let node = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        BackpressureConfig::default(),
        identity(),
    )
    .await;

    publish_range(&node, 0, 30).await;
    assert_eq!(store.earliest_available("default"), 21);

    let pool = pool(&tls, PoolConfig::default()).await;
    let mut watcher = PolicyWatcher::new(
        pool.clone(),
        node.url(),
        None,
        identity(),
        HashMap::new(),
        "default",
        0,
    );

    // The watcher must hit ResumeTooOld, take a snapshot at version 30, then
    // continue live. Publish until it observes an event so the test is not
    // sensitive to snapshot timing on a loaded machine.
    let mut handle = tokio::spawn(async move {
        let event = watcher.next_event().await;
        (watcher, event)
    });
    for _ in 0..50 {
        if handle.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        node.server
            .publish("default", b"bundle-live".to_vec(), "sum");
    }
    let (watcher, event) = timeout(TEST_TIMEOUT, &mut handle)
        .await
        .expect("watch event timed out")
        .expect("watch task panicked");
    let event = event.expect("watch event failed");

    assert!(event.version > 30, "expected a live event, got {event:?}");
    assert_eq!(watcher.last_applied(), event.version);
    let snapshot = watcher.last_snapshot().expect("snapshot fallback");
    assert!(snapshot.version >= 30, "snapshot at {}", snapshot.version);
    assert!(!snapshot.bundle.is_empty());

    let snapshot_metrics = metrics.snapshot();
    assert_eq!(snapshot_metrics.resume_too_old_total, 1);
    assert!(snapshot_metrics.watch_evictions_total >= 20);
    let client = pool.metrics_snapshot();
    assert_eq!(client.snapshot_fallbacks_total, 1);

    node.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_subscriber_is_bounded_and_server_stays_responsive() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        retention(4096, 16 * 1024 * 1024),
        metrics.clone(),
    ));
    let backpressure = BackpressureConfig {
        watch_queue_entries: 4,
        watch_queue_bytes: 4096,
        subscriber_queue_bytes: 8192,
        max_watches_per_subscriber: 8,
        max_subscribers: 128,
        max_in_flight_ingest: 4,
        retry_after_ms: 10,
    };
    let node = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        backpressure,
        identity(),
    )
    .await;

    // Small receive windows so a non-reading subscriber stalls quickly.
    let client = quiccrunch_core::transport::adapter::TquicClient::with_options(
        quiccrunch_core::transport::adapter::RuntimeOptions {
            initial_max_data: 2048,
            initial_max_stream_data: 512,
            max_connection_window: 2048,
            max_stream_window: 512,
            ..tls.client_options()
        },
    )
    .await
    .expect("client");
    let connector =
        Arc::new(quiccrunch_core::messaging::transport::TquicConnector::new(client).await);
    let pool = Arc::new(MessagingPool::new(connector, PoolConfig::default()));

    // Open a watch and never read from it.
    let (stalled, _request_id) = pool
        .open_watch(
            &node.url(),
            None,
            &identity(),
            &HashMap::new(),
            RequestPayload::PolicyWatch(PolicyWatch {
                policy_id: "default".into(),
                since_version: 0,
            }),
        )
        .await
        .expect("open watch");
    tokio::time::sleep(Duration::from_millis(100)).await;
    for _ in 0..100 {
        if store.subscriber_count() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(store.subscriber_count() > 0, "watch was not registered");

    for version in 0..2000 {
        node.server
            .publish("default", vec![version as u8; 256], "sum");
    }
    for _ in 0..100 {
        if metrics.snapshot().watch_shed_total > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let snapshot = metrics.snapshot();
    assert!(
        snapshot.watch_shed_total >= 1,
        "expected a shed, got {snapshot:?}"
    );
    assert!(snapshot.watch_slow_consumer_total >= 1);
    assert!(
        snapshot.watch_queue_depth <= 4,
        "queue must stay bounded, depth {}",
        snapshot.watch_queue_depth
    );
    assert!(store.retained_entries("default") <= 4096);

    // The server remains responsive to unrelated RPCs.
    let ack = timeout(
        TEST_TIMEOUT,
        pool.call(
            &node.url(),
            None,
            &identity(),
            &HashMap::new(),
            RequestPayload::Heartbeat(Heartbeat {
                node_id: "probe".into(),
                epoch: 1,
            }),
        ),
    )
    .await
    .expect("heartbeat timed out")
    .expect("heartbeat failed");
    assert!(matches!(ack, ResponsePayload::Heartbeat(_)));

    drop(stalled);
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reconnect_storm_keeps_pool_bounded() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        RetentionConfig::default(),
        metrics.clone(),
    ));
    let node = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        BackpressureConfig::default(),
        identity(),
    )
    .await;

    let pool = pool(
        &tls,
        PoolConfig {
            max_connections: 2,
            max_retries: 5,
            max_connect_attempts: 8,
            rpc_timeout: Duration::from_secs(2),
            backoff_base: Duration::from_millis(10),
            backoff_max: Duration::from_millis(200),
            ..PoolConfig::default()
        },
    )
    .await;

    let url = node.url();
    let mut tasks = Vec::new();
    for i in 0..50u64 {
        let pool = pool.clone();
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            pool.call(
                &url,
                None,
                &identity(),
                &HashMap::new(),
                RequestPayload::Heartbeat(Heartbeat {
                    node_id: format!("node-{i}"),
                    epoch: i,
                }),
            )
            .await
        }));
    }
    for task in tasks {
        timeout(TEST_TIMEOUT, task)
            .await
            .expect("storm task timed out")
            .expect("storm task panicked")
            .expect("storm call failed");
    }
    assert_eq!(pool.pool_size().await, 1);
    assert_eq!(
        pool.metrics_snapshot().connections_opened,
        1,
        "50 concurrent RPCs must reuse one pooled connection"
    );

    // Restart the server on the same address; every client must reconnect.
    let node = node.restart().await;
    let mut tasks = Vec::new();
    for i in 0..50u64 {
        let pool = pool.clone();
        let url = url.clone();
        tasks.push(tokio::spawn(async move {
            pool.call(
                &url,
                None,
                &identity(),
                &HashMap::new(),
                RequestPayload::Heartbeat(Heartbeat {
                    node_id: format!("node-{i}"),
                    epoch: i,
                }),
            )
            .await
        }));
    }
    for task in tasks {
        timeout(TEST_TIMEOUT, task)
            .await
            .expect("reconnect task timed out")
            .expect("reconnect task panicked")
            .expect("reconnect call failed");
    }

    assert_eq!(pool.pool_size().await, 1, "pool must stay bounded");
    let stats = pool.metrics_snapshot();
    assert!(stats.reconnects_total >= 1);
    assert!(
        stats
            .connections_opened
            .saturating_sub(stats.connections_closed)
            <= 1,
        "leaked connections: {stats:?}"
    );

    node.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mutation_retry_is_applied_once_within_dedupe_window() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        RetentionConfig::default(),
        metrics.clone(),
    ));
    let node = spawn_server(
        &tls,
        store.clone(),
        metrics.clone(),
        BackpressureConfig::default(),
        identity(),
    )
    .await;
    let pool = pool(&tls, PoolConfig::default()).await;

    let ingest = RequestPayload::EventIngest(EventIngest {
        events: vec![Event {
            event_id: "evt-1".into(),
            kind: "waf.block".into(),
            occurred_at_unix_ms: 1,
            payload: b"{}".to_vec(),
        }],
    });
    let first = pool
        .call_with_request_id(
            &node.url(),
            None,
            &identity(),
            &HashMap::new(),
            ingest.clone(),
            42,
        )
        .await
        .expect("first ingest");
    let second = pool
        .call_with_request_id(&node.url(), None, &identity(), &HashMap::new(), ingest, 42)
        .await
        .expect("retried ingest");
    assert_eq!(first, second, "dedupe must replay the original response");

    let lease = RequestPayload::LeaseAcquire(LeaseAcquire {
        lease_id: "lease-1".into(),
        resource: "policy/default".into(),
        ttl_ms: 5_000,
    });
    let first = pool
        .call_with_request_id(
            &node.url(),
            None,
            &identity(),
            &HashMap::new(),
            lease.clone(),
            43,
        )
        .await
        .expect("first lease");
    let second = pool
        .call_with_request_id(&node.url(), None, &identity(), &HashMap::new(), lease, 43)
        .await
        .expect("retried lease");
    assert_eq!(first, second);

    // One event + one lease apply, not four.
    let service_applied = {
        // The server's service is private; observe through the response shape
        // and the dedupe counter instead.
        metrics.snapshot().dedupe_hits_total
    };
    assert!(
        service_applied >= 2,
        "both retries must be served from dedupe"
    );

    node.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn event_ingest_sheds_with_explicit_backpressure() {
    struct SlowIngest {
        release: Arc<tokio::sync::Notify>,
    }

    #[async_trait]
    impl MessagingService for SlowIngest {
        async fn lease_acquire(
            &self,
            _ctx: &RequestContext,
            _req: LeaseAcquire,
        ) -> quiccrunch_core::messaging::Result<LeaseAcquired> {
            Err(MessagingError::Service("unused".into()))
        }
        async fn lease_release(
            &self,
            _ctx: &RequestContext,
            _req: LeaseRelease,
        ) -> quiccrunch_core::messaging::Result<LeaseReleased> {
            Err(MessagingError::Service("unused".into()))
        }
        async fn policy_get(
            &self,
            _ctx: &RequestContext,
            _req: PolicyGet,
        ) -> quiccrunch_core::messaging::Result<PolicyBundle> {
            Err(MessagingError::Service("unused".into()))
        }
        async fn event_ingest(
            &self,
            _ctx: &RequestContext,
            req: EventIngest,
        ) -> quiccrunch_core::messaging::Result<EventIngested> {
            self.release.notified().await;
            Ok(EventIngested {
                accepted: req.events.len() as u32,
                duplicate: 0,
            })
        }
        async fn heartbeat(
            &self,
            _ctx: &RequestContext,
            req: Heartbeat,
        ) -> quiccrunch_core::messaging::Result<HeartbeatAck> {
            Ok(HeartbeatAck {
                server_time_unix_ms: 0,
                epoch: req.epoch,
            })
        }
    }

    let release = Arc::new(tokio::sync::Notify::new());
    let server = Arc::new(
        MessagingServer::new(Arc::new(SlowIngest {
            release: release.clone(),
        }))
        .with_backpressure(BackpressureConfig {
            max_in_flight_ingest: 1,
            retry_after_ms: 7,
            ..Default::default()
        })
        .with_authorization(quiccrunch_core::messaging::server::AuthorizationPolicy {
            require_identity: true,
            ..Default::default()
        }),
    );

    let make_frame = |id: &str| {
        encode(&Message::request(
            1,
            1,
            Body::Request {
                identity: identity(),
                metadata: HashMap::new(),
                payload: RequestPayload::EventIngest(EventIngest {
                    events: vec![Event {
                        event_id: id.into(),
                        kind: "waf.block".into(),
                        occurred_at_unix_ms: 1,
                        payload: Vec::new(),
                    }],
                }),
            },
        ))
        .unwrap()
    };
    let first = make_frame("a");
    let second = make_frame("b");

    let peer = identity();
    let first_server = server.clone();
    let first_peer = peer.clone();
    let first_task =
        tokio::spawn(async move { first_server.dispatch(&first, Some(&first_peer)).await });
    // Give the first ingest time to enter the service and block.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let second_result = server.dispatch(&second, Some(&peer)).await;
    release.notify_one();
    let first_result = first_task.await.expect("first dispatch task");

    let frames = |dispatch: quiccrunch_core::messaging::server::Dispatch| match dispatch {
        quiccrunch_core::messaging::server::Dispatch::Frames(frames) => frames,
        _ => panic!("expected frames"),
    };
    let first_frames = frames(first_result);
    let second_frames = frames(second_result);
    let mut bodies = Vec::new();
    for frame in first_frames.into_iter().chain(second_frames) {
        bodies.push(decode(&frame).unwrap().body);
    }
    let shed = bodies.iter().any(|body| {
        matches!(
            body,
            Body::Error {
                code: 14,
                retry_after_ms: 7,
                ..
            }
        )
    });
    assert!(shed, "expected an explicit backpressure error: {bodies:?}");
    assert_eq!(server.metrics().snapshot().ingest_shed_total, 1);
}

struct HangingIo;

#[async_trait]
impl FrameIo for HangingIo {
    async fn send_frame(&mut self, _frame: &[u8]) -> quiccrunch_core::messaging::Result<()> {
        Ok(())
    }

    async fn recv_frame(&mut self) -> quiccrunch_core::messaging::Result<Vec<u8>> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn in_flight_request_fails_retryable_instead_of_hanging() {
    let mut client = MessagingClient::new(Box::new(HangingIo), identity())
        .with_timeout(Duration::from_millis(100));
    let err = client
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "node".into(),
            epoch: 1,
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(err, MessagingError::Retryable { .. }),
        "expected retryable timeout, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pool_evicts_lru_endpoint_when_bounded() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        RetentionConfig::default(),
        metrics.clone(),
    ));
    let mut nodes = Vec::new();
    for _ in 0..3 {
        nodes.push(
            spawn_server(
                &tls,
                store.clone(),
                metrics.clone(),
                BackpressureConfig::default(),
                identity(),
            )
            .await,
        );
    }
    let pool = pool(
        &tls,
        PoolConfig {
            max_connections: 2,
            ..PoolConfig::default()
        },
    )
    .await;
    for node in &nodes {
        pool.call(
            &node.url(),
            None,
            &identity(),
            &HashMap::new(),
            RequestPayload::Heartbeat(Heartbeat {
                node_id: "probe".into(),
                epoch: 1,
            }),
        )
        .await
        .expect("call");
    }
    assert_eq!(pool.pool_size().await, 2);
    let stats: ClientMetricsSnapshot = pool.metrics_snapshot();
    assert_eq!(stats.pool_evictions_total, 1);

    for node in nodes {
        node.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn event_ingest_duplicate_event_ids_are_reported() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        RetentionConfig::default(),
        metrics.clone(),
    ));
    let node = spawn_server(
        &tls,
        store,
        metrics,
        BackpressureConfig::default(),
        identity(),
    )
    .await;
    let pool = pool(&tls, PoolConfig::default()).await;
    let payload = RequestPayload::EventIngest(EventIngest {
        events: vec![
            Event {
                event_id: "same".into(),
                kind: "waf.block".into(),
                occurred_at_unix_ms: 1,
                payload: Vec::new(),
            },
            Event {
                event_id: "same".into(),
                kind: "waf.block".into(),
                occurred_at_unix_ms: 2,
                payload: Vec::new(),
            },
        ],
    });
    let response = pool
        .call(&node.url(), None, &identity(), &HashMap::new(), payload)
        .await
        .expect("ingest");
    match response {
        ResponsePayload::EventIngest(result) => {
            assert_eq!(result.accepted, 1);
            assert_eq!(result.duplicate, 1);
        }
        other => panic!("unexpected response: {other:?}"),
    }
    node.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn idle_connections_are_reaped() {
    let tls = TestTls::new();
    let metrics = Arc::new(MessagingMetrics::default());
    let store = Arc::new(PolicyStore::new(
        RetentionConfig::default(),
        metrics.clone(),
    ));
    let node = spawn_server(
        &tls,
        store,
        metrics,
        BackpressureConfig::default(),
        identity(),
    )
    .await;
    let pool = pool(
        &tls,
        PoolConfig {
            idle_timeout: Duration::from_millis(50),
            ..PoolConfig::default()
        },
    )
    .await;
    pool.call(
        &node.url(),
        None,
        &identity(),
        &HashMap::new(),
        RequestPayload::Heartbeat(Heartbeat {
            node_id: "probe".into(),
            epoch: 1,
        }),
    )
    .await
    .expect("call");
    assert_eq!(pool.pool_size().await, 1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(pool.reap_idle().await, 1);
    assert_eq!(pool.pool_size().await, 0);
    assert_eq!(pool.metrics_snapshot().idle_evictions_total, 1);
    node.stop().await;
}
