# Backend Messaging (WAF ↔ distri-kv)

Status: **prototype / conditional candidacy** — QuicCrunch is not yet approved as
the WAF↔distri-kv transport, but the blockers identified in
`quiccrunch-evaluation.md` now have concrete fixes and a working prototype.

## 1. What was built

| Area | Result |
| --- | --- |
| Workspace build | `cargo check --workspace --all-targets` passes |
| Workspace tests | `cargo test --workspace` passes (doctest examples in `quiccrunch-core` are disabled; they describe an unimplemented high-level API) |
| CI lint gate | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| TLS defaults | Peer verification is **on by default**; `verify_cert = false` is rejected unless `allow_insecure_tls = true` |
| mTLS | Client certificate/key and CA bundle are applied to BoringSSL; servers request/verify client certs when a CA is configured |
| Metadata | Request metadata is serialized/deserialized in Cap'n Proto; bearer tokens now cross the wire |
| Auth / RBAC / rate limit / metrics | `RpcServer` has a mandatory interceptor chain; `AuthInterceptor`, `RateLimitInterceptor` and `MetricsInterceptor` are invoked before/after dispatch |
| Dependency audit | `cargo audit`: **0 vulnerabilities** (13 informational unmaintained warnings) |
| Messaging prototype | Versioned Cap'n Proto schema, server dispatch, Rust client, in-memory loopback tests |
| Watch resume | Bounded per-policy retention, exact `sinceVersion` replay, explicit `resume-too-old` |
| Reconnect / pooling | Endpoint-keyed bounded pool, backoff+jitter, idle eviction, self-healing watcher |
| Backpressure | Bounded per-watch/subscriber queues, slow-consumer close, `EventIngest` shedding |
| Idempotency | Request-id dedupe for mutations; at-least-once documented outside the window |

## 2. Versioned messaging contract

Schema: `quiccrunch-core/schema/messaging.capnp`.
Wire framing: `u32` big-endian length + Cap'n Proto envelope, 4 MiB maximum.

Services:

- `LeaseAcquire` / `LeaseRelease` — unary request/response with fencing tokens.
- `PolicyGet` — unary policy bundle fetch.
- `PolicyWatch` — server streaming: retained replay after `sinceVersion`,
  then live updates; `StreamEnd` only on normal shutdown.
- `EventIngest` — bounded batch ingest (max 1024 events), admission-controlled.
- `Heartbeat` — liveness/epoch exchange.

Versioning rules:

- `Envelope.contractVersion` is mandatory and currently `1`.
- Servers reject unknown versions before touching payloads.
- New fields use new ordinals; existing field numbers/types are never reused.
  `Error.earliestAvailable`, `Error.serverLatest` and `Error.retryable` were
  added this way; v1 decoders that predate them read the Cap'n Proto defaults.
- `MAX_ENVELOPE_BYTES`, `MAX_BUNDLE_BYTES`, `MAX_EVENTS_PER_INGEST` and
  `MAX_METADATA_ENTRIES` are enforced on encode and decode.
- Decode uses a 4 MiB traversal limit and nesting limit 32.

## 3. Watch resume semantics

`PolicyWatch.sinceVersion` means "I have applied every version up to and
including this one". The server replays retained versions strictly greater than
`sinceVersion`, in increasing order, then keeps the stream open for live
updates. Versions are contiguous per policy and never reused.

The server registers the subscriber and computes the replay set under the same
lock, so a concurrent publish is either in the replay set or in the live queue —
never lost and never duplicated. Clients additionally drop any event whose
version is less than or equal to their last applied version, and treat a gap as
a re-sync trigger.

If the first required version has been evicted, the server fails the watch with
`resume-too-old { earliest_available }` (wire code 12). It never silently skips
a version. The client responds by fetching a full `PolicyGet` snapshot, setting
its last applied version to the snapshot version, and resuming the watch. The
same fallback handles `resume-ahead` (code 13), which means the client is ahead
of the server — for example after server state loss; the snapshot is
authoritative and the client follows it.

## 4. Retention bounds

Each policy keeps a ring buffer configured by `RetentionConfig`:

| Bound | Default | Behaviour |
| --- | --- | --- |
| `max_entries` | 1024 versions | Oldest entries are evicted first |
| `max_bytes` | 4 MiB of bundle payload | Oldest entries are evicted first |

An entry larger than `max_bytes` is delivered to live subscribers but not
retained, because retaining it would break the memory bound; a later resume
across that version therefore takes the snapshot fallback. Evictions increment
`watch_evictions_total`. Retention is per policy and independent of subscriber
count, so a dead subscriber cannot grow server memory.

`PolicyStore::earliest_available(policy)` exposes the oldest replayable version
for operators and tests.

## 5. Backpressure policy

Publishing never blocks on a subscriber. Limits are in `BackpressureConfig`:

| Limit | Default | On overflow |
| --- | --- | --- |
| Per-watch queued events | 1024 | Close watch with `slow-consumer` (code 15) |
| Per-watch queued bytes | 4 MiB | Close watch with `slow-consumer` |
| Per-subscriber queued bytes (all watches) | 8 MiB | Close the offending watch |
| Watches per subscriber | 8 | Reject with `watch-limit-exceeded` (code 16) |
| Concurrent watch subscriptions | 1024 | Reject with `watch-limit-exceeded` |
| Concurrent `EventIngest` calls | 64 | Reject with `backpressure` (code 14) |

The chosen watch policy is **close with a specific error**, not drop-oldest:
dropping an update would create a permanent gap in a security control plane.
The client reconnects from its last applied version and resumes from retention
(or takes a snapshot if that version was evicted). Shed and eviction counts are
exported as metrics. Transport sends also have a deadline; a socket that cannot
accept a frame within 5 s is treated as a slow consumer.

`EventIngest` is admission-controlled before the service is called. When the
in-flight cap is reached the request is rejected with `backpressure` and
`retry_after_ms`; nothing is buffered and nothing is applied.

## 6. Retry and idempotency contract

Every request carries a client-generated `request_id`; retries reuse it. The
server keeps a bounded dedupe cache (`DedupeConfig`, default 4096 entries) keyed
by `(peer node/spiffe id, request_id)` for mutations: `LeaseAcquire`,
`LeaseRelease` and `EventIngest`. A retry inside the window replays the original
response instead of applying the mutation again, so retries are
**at-most-once within the window**.

Outside the window the contract is **at-least-once**. Mutations carry stable
keys (`lease_id`, `event_id`) so the backend can deduplicate; the reference
`InMemoryService` does so for event ids. Read-only calls (`PolicyGet`,
`Heartbeat`) are always safe to retry.

The client pool retries retryable failures (`transport`, `retryable`,
`backpressure`) with bounded exponential backoff and full jitter, reusing the
request id. A request that sent but received no response within `rpc_timeout`
returns a retryable error instead of hanging; the caller may retry safely within
the dedupe window.

## 7. Reconnect and pooling

`MessagingPool` is keyed by `(address, server_name)` and bounded by
`max_connections`. It reuses one QUIC connection per endpoint across RPCs and
opens one bidirectional stream per call. Connections idle for `idle_timeout` are
closed by `reap_idle()`; when the pool is full, the least-recently-used idle
endpoint is evicted. If every slot is active, new endpoints are shed with a
`backpressure` error rather than closing a live connection.

Reconnect uses bounded exponential backoff with jitter (`backoff_base`,
`backoff_max`, `max_connect_attempts`). In-flight calls are limited per
connection (`max_in_flight_per_connection`); waiting longer than `queue_wait`
sheds with `backpressure`. A failed stream invalidates the pooled connection
and the next call redials.

`PolicyWatcher` re-establishes watches automatically from its last applied
version, honours `resume-too-old` with a snapshot fallback, skips duplicate
versions, and re-syncs on gaps. Metrics: `reconnects_total`,
`watch_resumes_total`, `snapshot_fallbacks_total`, `pool_evictions_total`,
`idle_evictions_total`, `pool_size`, `retries_total`.

## 8. Metrics

`MessagingMetrics::snapshot()` exposes:

| Metric | Meaning |
| --- | --- |
| `watch_evictions_total` | Retained versions evicted by the ring buffer |
| `watch_retained_entries` / `watch_retained_bytes` | Current retention gauges |
| `watch_shed_total` | Watches closed because a queue was full |
| `watch_slow_consumer_total` | Subscriptions closed as slow consumers |
| `watch_queue_depth` / `watch_queue_max_depth` | Queued events (gauge / high-water) |
| `resume_too_old_total` / `resume_ahead_total` | Resume fallbacks |
| `subscribers` | Live watch subscriptions |
| `ingest_shed_total` / `ingest_in_flight` | `EventIngest` admission control |
| `ingest_accepted_total` | Events accepted |
| `dedupe_hits_total` | Mutation retries served from the dedupe cache |

Client-side counters live in `ClientMetrics::snapshot()` (see section 7). Both
are plain atomics so a Prometheus/OTel exporter can scrape them without a lock.

## 9. Security model

1. **Transport**: TLS 1.3 via BoringSSL. Client verification defaults on.
   Configure `ca_cert_file` (or rely on an explicitly opted-in insecure mode for
   tests). `server_name` is configurable so DNS/SPIFFE SANs can be verified.
2. **mTLS**: `client_cert_file` + `client_key_file` are presented by clients;
   servers request and verify client certificates when `ca_cert_file` is set.
3. **Identity**: the transport layer is expected to supply `PeerIdentity`.
   Envelope-asserted identity is only honoured with
   `allow_untrusted_identity = true` (development).
4. **Authn**: bearer tokens in `authorization` metadata are verified by
   `AuthInterceptor` (HS256/RS256, expiry, issuer, audience).
5. **Authz**: per-method roles in `AuthorizationPolicy`; the prototype requires
   role `waf` for `EventIngest`.
6. **Rate limiting**: `RateLimitInterceptor` is part of the server chain and
   keys on `x-client-id` / `x-spiffe-id`. The sub-token refill bug was fixed.
7. **Metrics**: `MetricsInterceptor` records totals, failures and latency.

Known limitation: tQUIC 1.6 does not expose the peer certificate to the
application. The server can request/verify a client certificate, but cannot yet
read its SPIFFE ID. Until that is available, identity must be bound at the
application layer (e.g. a signed identity assertion) or tQUIC must gain a peer
certificate accessor. This is the single blocking item for a production mTLS
identity model.

## 10. Dependency remediation

Updated: `bytes` 1.12.1, `capnp`/`capnpc` 0.27.x, `tonic` 0.12, `prost` 0.13,
`crossbeam-epoch`, plus a general lockfile refresh. Removed the stale `rustls
0.21` direct dependency and the unmaintained `opentelemetry-jaeger` exporter
(which pulled vulnerable `thrift`). `cargo audit` is clean of vulnerabilities.

## 11. Assessment: can QuicCrunch become a viable backend messaging layer?

**Conditionally, yes for the application layer; not yet for production
identity.** The unary and streaming contract, watch resume, bounded retention,
reconnect/pooling and backpressure are implemented and tested, and the security
scaffolding is wired rather than dead code. Remaining gaps are concrete
engineering items rather than missing architecture.

| Gap | Impact | Status / estimate |
| --- | --- | --- |
| Peer certificate / SPIFFE identity from tQUIC | Blocks production mTLS authorization | Open — 2–3 weeks (patch tQUIC or add identity assertion) |
| Reconnect, backoff, health-aware pooling | Control plane resilience | **Closed** — `MessagingPool` + `PolicyWatcher` |
| Flow control / bounded watch chunking | Slow consumers can stall streams | **Closed** — bounded queues + slow-consumer close |
| Watch resume correctness (server-side retention) | Missed events after disconnect | **Closed** — `PolicyStore` ring buffer + `resume-too-old` |
| Mutation retry idempotency | Duplicate lease/event application | **Closed within window** — request-id dedupe; at-least-once outside |
| Schema evolution policy + compatibility tests | Safe v1→v2 rollout | Open — 1 week (new-ordinal rule documented; no compat suite) |
| Prometheus/OTel exporter + `/metrics`, health endpoints | Operability | Partially closed — atomic snapshots exist; no exporter/endpoint |
| Per-worker metrics/health, graceful drain | Multi-worker operations | Open — 1 week |
| Fuzzing (decode/auth/streaming) + adversarial TLS tests | Assurance | Open — 1–2 weeks |
| Revocation of policy versions / compromised node keys | Cannot withdraw a bad bundle | Open — requires contract extension |
| Latency work (2 ms p50 / 35 ms p99 measured) | Lease/heartbeat tail latency | Open — 2–3 weeks |

**Total to production parity: roughly 6–10 engineer-weeks**, down from 10–16
now that resume, pooling and backpressure are closed. The remaining blocking
item for a production identity model is still the peer certificate accessor.

Recommendation: keep QuicCrunch as a **prototype backend** behind a
`PolicySource`/`EventSink`-style trait boundary. Do not make it the default
control-plane transport until peer identity is exposed by the QUIC stack and the
observability/schema-evolution work lands.

## 12. Running the prototype

```bash
nix develop -c cargo test -p quiccrunch-core --test messaging_loopback
nix develop -c cargo test -p quiccrunch-core --test messaging_production
nix develop -c cargo test --workspace
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo audit
```

The loopback test covers unary calls, server streaming, identity rejection,
per-method RBAC, rate limiting and metrics interception without opening a
socket. `messaging_production` uses real QUIC sockets with TLS verification on:

- publish on node A, subscribe on B, disconnect, publish 100 more, resume with
  `sinceVersion` — exact, gap-free, duplicate-free replay;
- retention eviction returns `resume-too-old` with the correct
  `earliest_available`, and the snapshot fallback converges;
- a non-reading subscriber is bounded and shed, counters increment, and the
  server keeps answering unary RPCs;
- 50 concurrent clients reconnect after a server restart; the pool stays
  bounded and closes superseded connections;
- mutation retries reuse a request id and are applied once within the dedupe
  window;
- `EventIngest` sheds with an explicit `backpressure` error;
- a sent request that receives no response fails retryable instead of hanging.

`messaging::transport` contains the tQUIC adapters (`TquicFrameIo`,
`TquicConnector`, `serve_listener`, `serve_connection`, `connect`). The
node-A/node-B test shares one `PolicyStore` between two servers to model a
replicated policy backend; replication itself is the backend's responsibility.

## Peer identity (mTLS / SPIFFE)


This document describes how a backend connection's authenticated identity is
carried into QuicCrunch's RPC authorization layer. It complements
[`SECURITY.md`](SECURITY.md), which covers the TLS/mTLS configuration itself.

### Connection identity (adapter path)

`TquicConnection::peer_identity() -> Option<PeerIdentity>` returns the identity
extracted from the **verified** peer certificate chain (SPIFFE X.509-SVID).
On the server this is the client workload; on the client it is the server
workload. `TquicListener::accept_with_identity()` returns the same value next to
the accepted connection.

The identity is `None` when:

* the peer presented no certificate (normal for a one-way TLS client), or
* the certificate has no `spiffe://` URI SAN, or
* mTLS is off on the server (a client certificate was never requested), or
* the local TLS stack did not verify the peer chain (client with
  `verify_server_cert` off). An unverified certificate is never exposed as an
  identity.

On a server configured with `RuntimeOptions::client_ca_file`, `None` cannot
reach the accept path for a live connection: the handshake fails, or the
connection is closed in `on_conn_established`, and
`RuntimeMetrics::mtls_rejections` is incremented.

### From connection to `RpcContext`

The server is responsible for copying the transport identity into the request
context. Use the provided helper so the metadata key cannot drift:

```rust
let mut metadata = HashMap::new();
if let Some(identity) = connection.peer_identity() {
    identity.insert_metadata(&mut metadata); // x-quiccrunch-peer-spiffe-id
}
```

`PeerIdentity::insert_metadata` writes
`x-quiccrunch-peer-spiffe-id = spiffe://<trust-domain>/ns/<ns>/sa/<sa>`.

**Trust boundary:** never copy this key from peer-supplied metadata. It must
only be populated from `TquicConnection::peer_identity()`. The interceptor
cannot tell the difference, so the server-side copy is the enforcement point.

### Method policy

`quiccrunch_core::identity::SpiffePolicy` maps a `Service.Method` key to the
exact SPIFFE IDs allowed to call it:

```rust
let mut policy = SpiffePolicy::new();
policy.allow(
    "Raft.AppendEntries",
    vec![SpiffeId::parse("spiffe://cluster.local/ns/kv/sa/raft")?],
);
policy.deny_all("Admin.Shutdown");
```

* a method with no rule is not restricted by this policy;
* an empty allowlist denies every identity (fail the method closed);
* a missing identity is denied for any protected method.

The `auth`-feature `SpiffeIdentityInterceptor` applies this policy in the
existing interceptor chain. Its `with_require_identity(true)` mode additionally
requires a transport identity for methods without a rule.

This is orthogonal to JWT authentication: SPIFFE proves *which workload
process* opened the connection, while a JWT (or another token) can carry
end-user or tenant claims. A typical server enables both.

### Worker-pool path

`AsyncTquicClient`/`transport/worker.rs` now installs a `TlsConfig`, honours
`WorkerConfig::verify_cert` and `WorkerConfig::ca_cert_file`, presents
`client_cert_file`/`client_key_file`, and can require client certificates via
`client_ca_file` plus `trust_domain`. It does **not** yet expose the peer
identity to the async API, so identity-based authorization should use the
adapter (`TquicClient`/`TquicServer`) until that is added.

### Reproduce

```bash
cd /tmp/opencode/qc-id
nix develop -c env CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-id \
  cargo test --offline -p quiccrunch-core --test mtls_identity -- --test-threads=1
```

The tests generate a CA, server certificate, client certificates (including a
foreign-CA and an expired one) and a rogue-trust-domain SAN, then run real mTLS
handshakes over loopback UDP.
