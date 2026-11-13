# QuicCrunch v0.1.0-alpha.1

**Status: alpha pre-release. Experimental, not production software.**

This is the first tagged QuicCrunch release. It exists so downstream projects
can pin a concrete tag and commit instead of tracking a moving branch. It is
not a statement of API stability, wire stability or production readiness, and
no support window is offered.

The workspace version is `0.1.0-alpha.1`. **The crates are not published to
crates.io** (`quiccrunch-core`, `quiccrunch-macros`, `quiccrunch-cli` and
`quiccrunch-security` all return 404 from the registry). This release is a git
tag plus GitHub release artefacts only. See [`docs/VERSIONING.md`](docs/VERSIONING.md)
for the pin.

## What this is

QuicCrunch is a QUIC RPC transport and benchmark harness built to answer one
question: does QUIC give a control-plane RPC anything that tonic/gRPC does not?
It includes a Raft RPC surface, a versioned backend-messaging contract, and the
instrumentation used for the published comparisons.

## Transport

- Push-based stream reads: a readable stream is drained in the transport thread
  and handed directly to the owning task's channel, without a per-chunk command
  round trip.
- Bounded wait-free sends: `send_batch`/`send_bytes` enqueue into a bounded
  queue and return as soon as capacity exists; they do not wait for the runtime
  thread to perform the write. The queue is the backpressure mechanism.
- Measured **51,350 msg/s** (51.3k) at 4 workers / 16 connections / pipeline
  depth 1, 1 KiB echo RPC, median of three runs, with **zero errors and zero
  watchdog timeouts**. RTT p50 is **~1.09 ms** and p99 **~3.4 ms**; the
  realistic floor is about **1 ms p50** on this host.
- **tonic wins bulk throughput.** In the Raft `AppendEntries` comparison,
  QuicCrunch is **0.28–0.91× tonic** at pipeline depths 3 and above (512 B and
  4 KiB payloads); tonic wins all six cells. The gap is smallest at depth 1
  (0.91× at 512 B, 0.62× at 4 KiB). These numbers are provisional: the shared
  host load ranged from 4.77 to 8.71. Do not read this release as a throughput
  win over tonic.

Hardware, method and the full 36-cell matrix are in
[`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md); the Raft
comparison is in
[`docs/RAFT_TRANSPORT_PERFORMANCE.md`](docs/RAFT_TRANSPORT_PERFORMANCE.md).

## Identity and TLS

- mTLS on the adapter path: when a server is configured with a client CA it
  requests a client certificate and rejects a handshake without one.
- The verified peer chain must contain exactly one SPIFFE URI SAN; the trust
  domain is checked. Identity is taken from the chain BoringSSL verified, never
  from envelope or peer-supplied metadata.
- RBAC (`SpiffePolicy`) maps `Service.Method` to an explicit SPIFFE allowlist
  and fails closed for protected methods with no identity.
- CRL checking is enforced at handshake time. `CrlPolicy::Required` is the
  default and is fail-closed for a missing, expired, malformed or badly signed
  CRL.
- Certificates/keys, trust anchors and CRLs can be hot-rotated; a reload swaps
  material atomically for new handshakes without dropping established
  connections.

Details and the remaining gaps are in [`docs/SECURITY.md`](docs/SECURITY.md).

## Raft RPC and messaging contract

- Versioned Raft RPC surface over Cap'n Proto (`AppendEntries`, `RequestVote`,
  `InstallSnapshot`) in `quiccrunch-core/src/raft`.
- Messaging contract v1: `LeaseAcquire`/`LeaseRelease`, `PolicyGet`,
  `PolicyWatch`, `EventIngest` and `Heartbeat`.
- Watch resume is bounded and exact: retained replay strictly after
  `sinceVersion`, an explicit `resume-too-old` error with `earliest_available`
  and a snapshot fallback; no silent skipped versions.
- Reconnect uses bounded exponential backoff with jitter and an endpoint-keyed
  pooled client. Per-watch and per-subscriber queues are bounded; a slow
  consumer is closed with a specific error instead of dropping updates.
- Mutation retries reuse a request id and are deduplicated within a bounded
  window; outside that window the contract is at-least-once. See
  [`docs/BACKEND_MESSAGING.md`](docs/BACKEND_MESSAGING.md).

## Assurance

- Nightly `cargo-fuzz` targets, 60 seconds each in a bounded run on
  2026-11-13 (Linux x86_64):
  - `frame_codec`: **3,500,886 executions**
  - `messaging_decode`: **306,385 executions**
  - `raft_rpc_decode`: **505,107 executions**

  These are smoke runs, not proof that the decoders are free of bugs.
- Stable `proptest` coverage of the same wire decoders runs on every pull
  request (`quiccrunch-core/tests/prop_wire.rs`).
- Schema-evolution tests (`quiccrunch-core/tests/schema_evolution.rs`) prove
  forward compatibility for an added optional field, the
  ignore-unknown-fields policy, and rejection of a v2 message carrying a
  required field by a v1 decoder.
- `cargo audit`: **0 vulnerabilities** (informational/unmaintained advisories
  are printed but do not fail the gate).

## Known limits

- **Vendored tQUIC patch.** QuicCrunch patches tQUIC 1.6.0 for verified
  peer-chain access and mandatory client certificates
  (`patches/tquic-1.6.0-peer-identity.patch`). This is maintained locally until
  [Tencent/tquic#538](https://github.com/Tencent/tquic/issues/538) (or an
  equivalent upstream change) merges. Upgrading tQUIC means re-applying and
  re-testing the patch.
- **OCSP is not supported**, including stapled OCSP. Use CRLs and short-lived
  certificates.
- **No CRL coverage for the server's own certificate chain** beyond the peer
  CA anchors and CRLs that are explicitly configured; the server checks the
  peer chain, not itself. Established connections are not re-checked when a
  certificate is later revoked.
- **Only the wire decoders are fuzzed.** The JWT/RBAC authentication path, TLS
  handshake/certificate parsing, and watch/stream state machines are not
  fuzzed. The `auth` feature is not part of the default build and does not
  currently compile.
- **Windows and macOS builds are unverified and are not shipped.** The release
  workflow builds Linux x86_64 only. The multi-worker server relies on Linux
  `SO_REUSEPORT`, and the vendored tQUIC/BoringSSL build has not been verified
  on the other platforms.
- **No production-hardening claims.** Benchmarks are loopback; there is no
  packet-loss, WAN, connection-migration or cost-per-byte measurement. The
  high-level client/server API is partly disabled, and there is no API or wire
  stability promise beyond the `contractVersion` gate.

## Assets

- `quiccrunch-cli-x86_64-unknown-linux-gnu.tar.gz` — release build of the
  `quiccrunch` CLI.
