use std::sync::Arc;

use quiccrunch_core::interceptor::MetricsInterceptor;
use quiccrunch_core::messaging::client::{spawn_loopback_server, LoopbackIo, MessagingClient};
use quiccrunch_core::messaging::server::{AuthorizationPolicy, InMemoryService, MessagingServer};
use quiccrunch_core::messaging::{
    Event, EventIngest, Heartbeat, LeaseAcquire, LeaseRelease, MessagingError, MessagingMetrics,
    PeerIdentity, PolicyGet, PolicyWatch, RequestPayload, ResponsePayload,
};
use quiccrunch_core::rpc::rate_limit::{DdosProtectionConfig, RateLimitInterceptor};

fn waf_identity() -> PeerIdentity {
    PeerIdentity {
        spiffe_id: "spiffe://distriwaf/node-a".into(),
        node_id: "node-a".into(),
        roles: vec!["waf".into()],
    }
}

fn server(
    service: Arc<InMemoryService>,
    peer: PeerIdentity,
) -> (MessagingClient, tokio::task::JoinHandle<()>) {
    let (client_io, server_io) = LoopbackIo::pair();
    let store = service.store().clone();
    let server = Arc::new(
        MessagingServer::with_policy_store(service, store, Arc::new(MessagingMetrics::default()))
            .with_interceptors({
                let mut chain = quiccrunch_core::interceptor::InterceptorChain::new();
                chain.add(MetricsInterceptor::new());
                chain
            })
            .with_authorization(
                AuthorizationPolicy {
                    require_identity: true,
                    ..Default::default()
                }
                .allow_role("EventIngest", "waf"),
            ),
    );
    let handle = spawn_loopback_server(server_io, server, Some(peer));
    (
        MessagingClient::new(Box::new(client_io), waf_identity()),
        handle,
    )
}

#[tokio::test]
async fn loopback_lease_policy_event_and_heartbeat() {
    let service = Arc::new(InMemoryService::default());
    for i in 0..3 {
        service.publish("default", format!("bundle-{i}").into_bytes(), "sum");
    }
    let (mut client, _handle) = server(service, waf_identity());

    let acquired = client
        .call(RequestPayload::LeaseAcquire(LeaseAcquire {
            lease_id: "lease-1".into(),
            resource: "policy/default".into(),
            ttl_ms: 10_000,
        }))
        .await
        .unwrap();
    let token = match acquired {
        ResponsePayload::LeaseAcquire(a) => {
            assert!(a.granted);
            a.fencing_token
        }
        other => panic!("unexpected: {other:?}"),
    };

    let released = client
        .call(RequestPayload::LeaseRelease(LeaseRelease {
            lease_id: "lease-1".into(),
            fencing_token: token,
        }))
        .await
        .unwrap();
    assert_eq!(
        released,
        ResponsePayload::LeaseRelease(quiccrunch_core::messaging::LeaseReleased { released: true }),
        "release response"
    );

    let policy = client
        .call(RequestPayload::PolicyGet(PolicyGet {
            policy_id: "default".into(),
            known_version: 0,
        }))
        .await
        .unwrap();
    match policy {
        ResponsePayload::PolicyGet(bundle) => {
            assert_eq!(bundle.version, 3);
            assert!(!bundle.bundle.is_empty());
        }
        other => panic!("unexpected: {other:?}"),
    }

    let ingested = client
        .call(RequestPayload::EventIngest(EventIngest {
            events: vec![Event {
                event_id: "e1".into(),
                kind: "waf.block".into(),
                occurred_at_unix_ms: 1,
                payload: b"{}".to_vec(),
            }],
        }))
        .await
        .unwrap();
    assert_eq!(
        ingested,
        ResponsePayload::EventIngest(quiccrunch_core::messaging::EventIngested {
            accepted: 1,
            duplicate: 0
        })
    );

    let ack = client
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "node-a".into(),
            epoch: 4,
        }))
        .await
        .unwrap();
    match ack {
        ResponsePayload::Heartbeat(ack) => assert_eq!(ack.epoch, 4),
        other => panic!("unexpected: {other:?}"),
    }

    // Server streaming last: the loopback server blocks on a live watch, so
    // this client must not need further unary calls afterwards. Replay from
    // version 2 yields exactly version 3.
    let mut seen = Vec::new();
    client
        .watch_until(
            RequestPayload::PolicyWatch(PolicyWatch {
                policy_id: "default".into(),
                since_version: 2,
            }),
            |event| {
                seen.push(event.version);
                false
            },
        )
        .await
        .unwrap();
    assert_eq!(seen, vec![3]);
}

#[tokio::test]
async fn loopback_rejects_unauthenticated_and_unauthorized() {
    let service = Arc::new(InMemoryService::default());
    let (mut client, _handle) = server(service, waf_identity());

    // Identity present but lacking the required role for EventIngest.
    let viewer = PeerIdentity {
        spiffe_id: "spiffe://distriwaf/viewer".into(),
        node_id: "viewer".into(),
        roles: vec!["viewer".into()],
    };
    let (mut viewer_client, _handle2) = server(Arc::new(InMemoryService::default()), viewer);
    let err = viewer_client
        .call(RequestPayload::EventIngest(EventIngest { events: vec![] }))
        .await
        .unwrap_err();
    assert!(
        matches!(err, MessagingError::PermissionDenied(_)),
        "got {err:?}"
    );

    // No transport identity and untrusted envelope identity is refused.
    let (client_io, server_io) = LoopbackIo::pair();
    let server = Arc::new(
        MessagingServer::new(Arc::new(InMemoryService::default())).with_authorization(
            AuthorizationPolicy {
                require_identity: true,
                allow_untrusted_identity: false,
                ..Default::default()
            },
        ),
    );
    let _handle3 = spawn_loopback_server(server_io, server, None);
    let mut anonymous = MessagingClient::new(Box::new(client_io), PeerIdentity::default());
    let err = anonymous
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "x".into(),
            epoch: 0,
        }))
        .await
        .unwrap_err();
    assert!(
        matches!(err, MessagingError::Unauthorized(_)),
        "got {err:?}"
    );

    // A healthy call still works through the same client helper.
    let ack = client
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "node-a".into(),
            epoch: 1,
        }))
        .await
        .unwrap();
    assert!(matches!(ack, ResponsePayload::Heartbeat(_)), "ack: {ack:?}");
}

#[tokio::test]
async fn loopback_rate_limit_is_enforced() {
    let (client_io, server_io) = LoopbackIo::pair();
    let mut chain = quiccrunch_core::interceptor::InterceptorChain::new();
    chain.add(RateLimitInterceptor::new(
        1,
        1,
        DdosProtectionConfig {
            max_requests_per_second: 100,
            max_burst: 100,
            max_concurrent_connections: 10,
            max_request_size: 4096,
            block_duration: std::time::Duration::from_secs(1),
            max_blocked_clients: 10,
        },
    ));
    let server = Arc::new(
        MessagingServer::new(Arc::new(InMemoryService::default()))
            .with_interceptors(chain)
            .with_authorization(AuthorizationPolicy {
                require_identity: true,
                ..Default::default()
            }),
    );
    let _handle4 = spawn_loopback_server(server_io, server, Some(waf_identity()));

    let mut client = MessagingClient::new(Box::new(client_io), waf_identity())
        .with_metadata("x-client-id", "client-1");

    let first = client
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "node-a".into(),
            epoch: 1,
        }))
        .await;
    assert!(first.is_ok(), "first call should pass: {first:?}");

    let second = client
        .call(RequestPayload::Heartbeat(Heartbeat {
            node_id: "node-a".into(),
            epoch: 2,
        }))
        .await;
    assert!(
        second.is_err() && format!("{second:?}").to_lowercase().contains("rate limit"),
        "second call should be rate limited: {second:?}"
    );
}
