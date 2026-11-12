# Backend Messaging (WAF ↔ distri-kv)

Status: **prototype, contract v1** — the messaging contract is implemented on
top of the merged mTLS/SPIFFE transport identity and the bounded
watch-retention/reconnect/backpressure machinery. QuicCrunch is still not the
default WAF↔distri-kv transport; the remaining gaps are listed in §11.

## 1. What was built

| Area | Result |
| --- | --- |
| Workspace build | `cargo check --workspace --all-targets` passes |
| Workspace tests | `cargo test --workspace` passes (doctest examples in `quiccrunch-core` are disabled; they describe an unimplemented high-level API) |
| CI lint gate | `cargo clippy --workspace --all-targets -- -D warnings` passes |
| TLS defaults | Server certificates are verified by default; the messaging path has no insecure mode. Tests use a generated CA. |
| mTLS | Clients present `client_cert_file`/`client_key_file`; a server with `client_ca_file` requires a client certificate and rejects the handshake otherwise |
| Peer identity | `TquicConnection::peer_identity()` returns the SPIFFE ID from the verified chain; `SpiffePolicy` authorizes methods. Envelope identity is never trusted. |
| Metadata | Request metadata is serialized/deserialized in Cap'n Proto; bearer tokens cross the wire |
| Auth / RBAC / rate limit / metrics | `RpcServer` has a mandatory interceptor chain; `AuthInterceptor`, `RateLimitInterceptor` and `MetricsInterceptor` are invoked before/after dispatch |
| Dependency audit | `cargo audit`: **0 vulnerabilities** (13 informational unmaintained warnings) |
| Messaging contract | Versioned Cap'n Proto schema, server dispatch, pooled Rust client, loopback and real-socket tests |
| Feature gate | None needed: messaging is in the default build and adds no optional dependency; the socket adapter uses the default `backend-tquic` feature |
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

- `Envelope.contractVersion` is mandatory and currently `1`. It is the
  **major** version. Additive minor changes do not change it; a breaking
  change increments it.
- Servers reject unknown versions before touching payloads. The error names
  both the received and the supported version, e.g.
  `unsupported contract version: 2 (supported: 1)`.
- New fields use new ordinals; existing field numbers/types are never reused.
  `Error.earliestAvailable`, `Error.serverLatest`, `Error.retryable` and the
  optional `Envelope.traceId` were added this way.
- **Unknown fields are safely ignored.** Cap'n Proto readers only follow the
  ordinals they know, so a v1 decoder ignores a field added in a later minor
  revision and reads the documented default for it. The schema does not use a
  reject-unknown-fields policy.
- A new **required** field is a breaking change: it must bump
  `contractVersion`. A v1 decoder rejects the new version before reading the
  payload, so it cannot silently substitute a default for the required field.
- `MAX_ENVELOPE_BYTES`, `MAX_BUNDLE_BYTES`, `MAX_EVENTS_PER_INGEST` and
  `MAX_METADATA_ENTRIES` are enforced on encode and decode.
- Decode uses a 4 MiB traversal limit and nesting limit 32.

### Evolution rules and compatibility matrix

| Change | Version bump | v1 decoder behaviour |
| --- | --- | --- |
| Add an optional field with a new ordinal | Minor (no `contractVersion` change) | Safely ignores it; known fields decode normally |
| Add a new enum value | Minor | Unknown value is a decode error for that field; callers must handle it |
| Add a new message type / union arm | Minor | Unknown union arm is a decode error; old peers cannot use the new call |
| Change a field type or reuse an ordinal | **Major** | Rejected by the version gate |
| Add a required field | **Major** | Rejected by the version gate before payload decode |
| Remove or rename a field | **Major** | Rejected by the version gate |
| Change the meaning of an existing field | **Major** | Rejected by the version gate |

The compatibility suite is
`quiccrunch-core/tests/schema_evolution.rs`. It proves forward compatibility
for an added optional field, asserts the ignore-unknown-fields policy, and
proves that a v2 message with a required field is rejected by the v1 decoder
with an error naming both versions. The Raft contract follows the same rules;
its version gate is in `quiccrunch-core/src/raft/codec.rs`.

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

The messaging layer does not implement its own TLS or identity model. It uses
the transport identity merged in #6 and the RPC interceptor chain.

1. **Transport**: TLS 1.3 via the vendored tQUIC/BoringSSL stack. The client
   verifies the server chain and name (`verify_server_cert = true`); the
   messaging path has no insecure switch. Loopback tests generate a CA and pass
   `ca_cert_file`.
2. **mTLS**: clients present `client_cert_file` + `client_key_file`. A server
   configured with `client_ca_file` requests a client certificate, fails the
   handshake when none is presented, and rejects a chain outside the configured
   trust domain. `RuntimeMetrics::mtls_rejections` counts rejections.
3. **Revocation**: with `crl_files` configured, the adapter checks the verified
   peer chain against issuer CRLs before reporting the connection as
   established. `CrlPolicy::Required` (default) is fail closed for a missing,
   expired, malformed or badly signed CRL; a listed serial is rejected.
   `revocation_rejections*` counters break rejections down by reason.
   Established connections are not re-checked; revocation applies to the next
   handshake. OCSP is not implemented (see [`SECURITY.md`](SECURITY.md)).
4. **Hot rotation**: server certificate/key, client certificate/key, trust
   anchors and CRLs are reloaded on `reload_interval` or on
   `reload_tls_material()`. A reload swaps material atomically for new
   handshakes and never drops established connections; see
   [`operations/certificate-rotation.md`](operations/certificate-rotation.md).
5. **Peer identity**: `TquicConnection::peer_identity()` returns the SPIFFE
   X.509-SVID from the chain BoringSSL verified. The server copies it into
   `RpcContext` metadata under `x-quiccrunch-peer-spiffe-id`; that key is never
   read from peer-supplied metadata. An unverified certificate never yields an
   identity.
6. **Authz**: `SpiffePolicy` maps `Service.Method` to allowed SPIFFE IDs and
   fails closed for protected methods with no identity. `AuthorizationPolicy`
   additionally requires the `waf` role for `EventIngest`.
7. **Authn**: bearer tokens in `authorization` metadata are verified by
   `AuthInterceptor` (HS256/RS256, expiry, issuer, audience).
8. **Rate limiting / metrics**: `RateLimitInterceptor` keys on
   `x-client-id`/`x-spiffe-id`; `MetricsInterceptor` records totals, failures
   and latency.
9. **Envelope identity**: `Envelope.Request.identity` is advisory only and is
   ignored unless `allow_untrusted_identity` is explicitly enabled for
   development. Production authorization uses the transport identity.

The worker-pool path (`AsyncTquicClient`) installs TLS and can require client
certificates, but does not yet expose the peer identity to the async API.
Identity-based authorization therefore uses the adapter path
(`TquicClient`/`TquicServer`) until that is added.

## 10. Dependency remediation

Updated: `bytes` 1.12.1, `capnp`/`capnpc` 0.27.x, `tonic` 0.12, `prost` 0.13,
`crossbeam-epoch`, plus a general lockfile refresh. Removed the stale `rustls
0.21` direct dependency and the unmaintained `opentelemetry-jaeger` exporter
(which pulled vulnerable `thrift`). `cargo audit` is clean of vulnerabilities.

## 11. Assessment: can QuicCrunch become a viable backend messaging layer?

**Conditionally, yes for the application layer; identity is no longer the
blocker.** The unary and streaming contract, watch resume, bounded retention,
reconnect/pooling, backpressure and mTLS/SPIFFE identity are implemented and
tested. Remaining gaps are concrete engineering items rather than missing
architecture.

| Gap | Impact | Status / estimate |
| --- | --- | --- |
| Peer certificate / SPIFFE identity from tQUIC | Blocks production mTLS authorization | **Closed** — vendored tQUIC patch + `SpiffePolicy` |
| Reconnect, backoff, health-aware pooling | Control plane resilience | **Closed** — `MessagingPool` + `PolicyWatcher` |
| Flow control / bounded watch chunking | Slow consumers can stall streams | **Closed** — bounded queues + slow-consumer close |
| Watch resume correctness (server-side retention) | Missed events after disconnect | **Closed** — `PolicyStore` ring buffer + `resume-too-old` |
| Mutation retry idempotency | Duplicate lease/event application | **Closed within window** — request-id dedupe; at-least-once outside |
| Schema evolution policy + compatibility tests | Safe v1→v2 rollout | **Closed** — rules documented; `schema_evolution` suite covers optional fields, unknown-field policy and major-version rejection |
| Prometheus/OTel exporter + `/metrics`, health endpoints | Operability | Partially closed — atomic snapshots exist; no exporter/endpoint |
| Per-worker metrics/health, graceful drain | Multi-worker operations | Open — 1 week |
| Fuzzing (decode/auth/streaming) + adversarial TLS tests | Assurance | **Wire decoders closed** — `fuzz/` targets + stable `prop_wire` proptests; auth/streaming/TLS remain in `docs/known-issues/fuzzing-coverage.md` |
| Revocation of policy versions / compromised node keys | Cannot withdraw a bad bundle | Open — requires contract extension |
| Latency work (2 ms p50 / 35 ms p99 measured) | Lease/heartbeat tail latency | Open — 2–3 weeks |

**Total to production parity: roughly 4–8 engineer-weeks**, down from 10–16
now that identity, resume, pooling and backpressure are closed.

Recommendation: keep QuicCrunch as a **prototype backend** behind a
`PolicySource`/`EventSink`-style trait boundary. Do not make it the default
control-plane transport until the observability, schema-evolution and
operational-hardening work lands.

## 12. Running the prototype

```bash
nix develop -c cargo test -p quiccrunch-core --test messaging_loopback
nix develop -c cargo test -p quiccrunch-core --test messaging_production
nix develop -c cargo test --workspace
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo audit
```

### Fuzzing and property tests

The stable test gate includes `quiccrunch-core/tests/prop_wire.rs`, which
exercises the same decoders as the nightly fuzz targets with structured and
adversarial inputs. The nightly targets live in `fuzz/` and are run manually or
from the `Fuzz (nightly)` `workflow_dispatch` CI job:

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
rustup run nightly scripts/fuzz.sh   # 60 s per target
FUZZ_SECONDS=120 rustup run nightly scripts/fuzz.sh
```

Targets: `frame_codec`, `messaging_decode`, `raft_rpc_decode`. See
`fuzz/README.md` and `docs/known-issues/fuzzing-coverage.md` for what is still
unfuzzed.

### Feature gate

Messaging is part of the default build and adds no optional dependency beyond
crates already in the default tree (`capnp`, `bytes`, `async-trait`, `tokio`).
The socket adapter uses the default `backend-tquic` feature, so there is no
separate `backend-messaging` feature to enable. The contract types, server
dispatch and in-memory loopback tests are compiled unconditionally.

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

The transport identity is the only identity the messaging layer trusts. It is
extracted from the certificate chain BoringSSL verified, never from the
envelope or from peer-supplied metadata.

```rust
let mut metadata = HashMap::new();
if let Some(identity) = connection.peer_identity() {
    identity.insert_metadata(&mut metadata); // x-quiccrunch-peer-spiffe-id
}
```

`PeerIdentity::insert_metadata` writes
`x-quiccrunch-peer-spiffe-id = spiffe://<trust-domain>/ns/<ns>/sa/<sa>`.
`quiccrunch_core::identity::SpiffePolicy` maps a `Service.Method` key to the
exact SPIFFE IDs allowed to call it; a protected method with no identity is
denied, and an empty allowlist denies every identity. The `auth`-feature
`SpiffeIdentityInterceptor` applies the policy in the existing interceptor
chain. This is orthogonal to JWT authentication: SPIFFE proves which workload
opened the connection, while a JWT can carry end-user or tenant claims.

The worker-pool path (`AsyncTquicClient`) installs TLS and can require client
certificates, but does not yet expose the peer identity to the async API.
Identity-based authorization therefore uses the adapter path
(`TquicClient`/`TquicServer`) until that is added.

### Identity tests

```bash
nix develop -c cargo test -p quiccrunch-core --test mtls_identity -- --test-threads=1
nix develop -c cargo test -p quiccrunch-core --test revocation_rotation -- --test-threads=1
```

The mTLS test generates a CA, server certificate and client certificates
(including foreign-CA, expired and rogue-trust-domain cases), then runs real
mTLS handshakes over loopback UDP. The revocation/rotation test generates CRLs
in-test and covers revoked, expired, missing and malformed CRLs plus server
certificate and trust-anchor rotation.
