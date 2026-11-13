# QuicCrunch

Experimental QUIC RPC transport and benchmark harness, built to evaluate whether
QUIC is a better backend messaging layer than tonic/gRPC for the distri-kv
control plane.

[![CI](https://github.com/shift/quiccrunch/actions/workflows/ci.yml/badge.svg)](https://github.com/shift/quiccrunch/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
![status: experimental](https://img.shields.io/badge/status-experimental-orange.svg)

## Project status

**This is a prototype.** It is not production software, it has no API or wire
stability promise, and it should not be deployed. The transport, the instrumented
benchmark and the tonic comparison harness are real and reproducible; most of the
surrounding framework is not finished.

QuicCrunch exists to answer one question with measurements: does QUIC give a
control-plane RPC anything that tonic/gRPC does not? The answer so far, on this
hardware and for this RPC shape, is no.

The first tagged release is **`v0.1.0-alpha.1`** (pre-release; not published to
crates.io). Read [`RELEASE_NOTES.md`](RELEASE_NOTES.md) before pinning it and
use the exact `rev` in [`docs/VERSIONING.md`](docs/VERSIONING.md).

### What is implemented and working

- Multi-worker QUIC server: one `TquicRuntime` event loop per worker on a
  dedicated OS thread, workers sharing a UDP socket via `SO_REUSEPORT`.
- Async client/server adapter over tQUIC 1.6
  (`quiccrunch-core::transport::adapter`), with push-based stream reads and a
  batched, wait-free send path.
- A versioned Raft RPC contract over Cap'n Proto
  (`quiccrunch-core/schema/raft.capnp`, see [`docs/RAFT_RPC.md`](docs/RAFT_RPC.md)),
  exercised by QUIC end-to-end tests.
- An instrumented benchmark that timestamps every request at enqueue, write
  hand-off and response, and reports completed-response throughput and true RTT
  percentiles (`quiccrunch-core/src/bin/pipelined_benchmark.rs`).

### What the measurements show

- **~51k msg/s** (median of three runs) at 4 workers / 16 connections / pipeline
  depth 1, with zero errors and zero watchdog timeouts; RTT p50 **~1.1 ms** and
  p99 **~3.4 ms** in that cell.
- Pipeline depth does not raise throughput below ~16 connections; it mostly
  raises queueing. The depth-64 and depth-1800 cells are queueing-dominated and
  must not be read as transport latency.
- Against tonic, with the same 1 KiB echo RPC and the same client
  instrumentation, **tonic wins bulk throughput: 1.5×–2.9× at pipeline depth
  1800**, and at depth 64 from 16 connections up. The only clear QuicCrunch win
  is 1 connection at depth 1 (+17% throughput, p99 184 µs vs 340 µs).

Sources: [`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md) for
methodology, hardware and the full 36-cell matrix;
[`docs/GRPC_VS_QUICCRUNCH.md`](docs/GRPC_VS_QUICCRUNCH.md) for the head-to-head.

### What is not done

- **Peer identity on the worker-pool path.** mTLS, SPIFFE SAN binding, RBAC,
  CRL checking and hot rotation are implemented and tested on the adapter path
  (`TquicClient`/`TquicServer`). `AsyncTquicClient` installs TLS but does not yet
  expose the peer identity, so identity-based authorization must use the
  adapter. See [`docs/SECURITY.md`](docs/SECURITY.md).
- **Raft RPC performance.** The Raft RPC surface exists and is benchmarked
  against tonic. Tonic wins every cell, including bulk throughput; see
  [`docs/RAFT_TRANSPORT_PERFORMANCE.md`](docs/RAFT_TRANSPORT_PERFORMANCE.md).
- **Backend messaging contract.** The versioned messaging prototype is
  implemented, but QuicCrunch is still not the default WAF↔distri-kv transport.
  The remaining gaps are in [`docs/BACKEND_MESSAGING.md`](docs/BACKEND_MESSAGING.md).
- **Network realism.** No packet loss, WAN delay, connection migration or
  CPU-cost-per-byte measurement. The comparison matrix is loopback (plus an
  injected fixed delay) and the path is lossless, so QUIC loss recovery never
  runs.
- **RPC serialization.** The generic `rpc` request/response envelope is a
  hand-written length-prefixed binary format; Cap'n Proto is used by the Raft RPC
  schema. The benchmark echo path is raw bytes, so serialization is not on the
  measured path.
- **Authentication.** The JWT/RBAC interceptor exists behind the non-default
  `auth` feature, but that feature does not compile on `main`. It is excluded
  from the default build and from CI. See
  [`docs/known-issues/auth-feature-does-not-build.md`](docs/known-issues/auth-feature-does-not-build.md).
- **Operational hardening.** CRL revocation, CRL/CA/certificate hot rotation
  and their tests are in place; OCSP is not implemented, established
  connections are not re-checked after revocation, and the worker-pool path
  does not hot-rotate. See [`SECURITY.md`](SECURITY.md).

### Deliberately out of scope

- Production deployment, API stability and a release cadence.
- Replacing tonic in distri-kv's control plane — the measurements argue against
  it.
- Kernel bypass (DPDK/AF_XDP) and eBPF reuseport steering.
- Sharding each worker into multiple event loops.
- A full RPC framework: codegen, discovery and the high-level client/server API
  are partly commented out in `quiccrunch-core` (see
  [`docs/known-issues/doctests-high-level-api.md`](docs/known-issues/doctests-high-level-api.md)).

## Performance

Every number below was produced by `scripts/bench.sh` on the hardware listed in
[`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md). Nothing here is
derived from send counts or from `1 / throughput`.

| Canonical cell (4 workers, 16 conns, 4 streams/conn, 1 KiB) | Throughput | RTT p50 | RTT p99 | Errors |
| --- | --- | --- | --- | --- |
| Pipeline depth 1 | 51,350 msg/s | 1,088 µs | 3,396 µs | 0 |

The matrix is one run per cell on a shared host, so treat ±20% as run-to-run
noise. Depth-64 and depth-1800 numbers, the worker-scaling summary, the error
analysis and the reproduction commands are all in
[`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md).

**Withdrawn claims.** An earlier README advertised 5,000 msg/s / 194 µs and
31,629 msg/s / 32 µs. Those were arithmetic, not measurement: `p99` was
`1 / throughput`, individual requests were never timed, and throughput counted
sends regardless of completions. They have been withdrawn; the only remaining
mention is the correction note in
[`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md).

## Getting started

### Prerequisites

- Linux (`SO_REUSEPORT` is required for the multi-worker server).
- Nix with flakes enabled, **or** stable Rust plus `cmake`, `clang`,
  `pkg-config` and OpenSSL headers.
- The flake and CI track the latest stable Rust. No MSRV is declared; the
  published measurements used rustc 1.89.0.

### Build

```bash
# With Nix (recommended: provides the toolchain and native dependencies)
nix develop -c cargo build --release

# With Cargo, if the native dependencies above are already installed
cargo build --release
```

### Run the server

```bash
# Development-only loopback certificate
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem \
  -days 365 -nodes -subj '/CN=localhost'

# Usage: multi_worker_server [addr] [cert] [key] [workers] [--bench]
cargo run --release --bin multi_worker_server -- \
  127.0.0.1:4433 cert.pem key.pem 4 --bench
```

`--bench` selects pacing-free, deep-window transport settings for same-host
measurement. Without it, the production defaults apply. The binary also accepts
`--spread` (worker `w` listens on `port + w`) and defaults to
`quiccrunch-core/tests/certs/{cert,key}.pem` when no paths are given.

### Run a benchmark

```bash
# Full matrix, including server startup and certificate generation
nix develop -c bash scripts/bench.sh

# One cell: address, seconds, connections, streams/conn, depth, payload bytes
cargo run --release --bin pipelined_benchmark -- 127.0.0.1:4433 10 16 4 1 1024
```

`scripts/bench.sh` documents the environment variables (`WORKERS`, `CONNS`,
`DEPTHS`, `MODE`, `DURATION`, `RUNS`, ...) in its header.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    UDP Socket (SO_REUSEPORT)            │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │  Worker 0   │ │  Worker 1   │ │  Worker 2   │ ...   │
│  │  (CPU 0)    │ │  (CPU 1)    │ │  (CPU 2)    │       │
│  │ TquicRuntime│ │ TquicRuntime│ │ TquicRuntime│       │
│  └─────────────┘ └─────────────┘ └─────────────┘       │
└─────────────────────────────────────────────────────────┘
```

Each worker runs its own `TquicRuntime` on a dedicated OS thread. The kernel
distributes incoming UDP packets across workers via `SO_REUSEPORT`; there is no
load balancer. The kernel's 4-tuple hash is uneven for small connection counts
(10 / 4 / 2 / 0 connections across 4 workers was observed), which is why the
benchmark has a `--spread` mode with exact distribution. See
[`docs/PERFORMANCE_RELEASE.md` §3](docs/PERFORMANCE_RELEASE.md).

## Library API

These snippets are copied from
[`quiccrunch-core/examples/rpc_api.rs`](quiccrunch-core/examples/rpc_api.rs),
which CI builds, so they track the code. They exercise the `rpc` module; the
measured path is the transport adapter, not this layer.

### Define an RPC service

```rust
use quiccrunch_core::rpc::service::{MethodDefinition, ServiceDefinition, StreamingType};

fn define_service() -> ServiceDefinition {
    let mut service = ServiceDefinition::new(1, "Calculator");
    service.add_method(MethodDefinition {
        id: 1,
        name: "Add".to_string(),
        streaming_type: StreamingType::Unary,
        request_schema: None,
        response_schema: None,
    });
    service
}
```

### Rate limiting

```rust
use quiccrunch_core::rpc::rate_limit::{DdosProtectionConfig, RateLimitInterceptor};

async fn check_rate_limit() -> Result<(), Box<dyn std::error::Error>> {
    let rate_limiter = RateLimitInterceptor::new(
        1000, // requests per second
        5000, // burst capacity
        DdosProtectionConfig::default(),
    );
    rate_limiter
        .check_rate_limit("client1", "my_service", 1024)
        .await?;
    Ok(())
}
```

### Prometheus metrics

```rust
use std::time::Duration;
use quiccrunch_core::rpc::metrics::RpcMetrics;

async fn collect_metrics() -> String {
    let metrics = RpcMetrics::new();
    metrics
        .record_request("my_service", "my_method", Duration::from_millis(2))
        .await;
    metrics.to_prometheus().await
}
```

## Workspace layout

| Crate / directory | State | Description |
| --- | --- | --- |
| `quiccrunch-core` | workspace member | Transport adapter, RPC types, Raft RPC, benchmark binaries |
| `quiccrunch-macros` | workspace member | Procedural macros for RPC codegen |
| `quiccrunch-cli` | workspace member | `quiccrunch` CLI for schema-driven codegen and ad-hoc RPC calls |
| `comparison` | workspace member | tonic vs QuicCrunch benchmark harness |
| `quiccrunch-ffi` | excluded from the workspace | Legacy C FFI; targets the disabled high-level API |
| `quiccrunch-security` | not a workspace member | Separate security crate; not built or tested by CI |

## Feature status

| Area | State |
| --- | --- |
| Transport | tQUIC 1.6 over QUIC v1, one-way TLS (server authentication only) |
| Serialization | Cap'n Proto for the Raft RPC schema; hand-written framing for the generic `rpc` envelope |
| Auth | JWT/RBAC code exists behind the non-default `auth` feature, which does not currently compile (see known issues) |
| Rate limiting | Token bucket plus DDoS auto-blocking in `rpc::rate_limit` |
| Observability | Prometheus text exposition in `rpc::metrics`; `tracing` logging; structured logger with correlation IDs in `production` |
| Health and shutdown | Helpers in the `production` module; not wired into the benchmark binaries |
| TLS acceleration | BoringSSL (via tQUIC) selects AES-NI/AVX at runtime; the in-tree `crypto_accel` module is not exported |

## Documentation

- [`docs/README.md`](docs/README.md) — index of every document, with how current each one is.
- [`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md) — measured performance and methodology.
- [`docs/GRPC_VS_QUICCRUNCH.md`](docs/GRPC_VS_QUICCRUNCH.md) — tonic head-to-head and recommendation.
- [`docs/RAFT_RPC.md`](docs/RAFT_RPC.md) — Raft RPC contract.
- [`comparison/README.md`](comparison/README.md) — comparison harness and reproduction.
- [`docs/known-issues/`](docs/known-issues/) — known gaps and their status.

## Security

The TLS defaults, the explicit opt-outs and the remaining gaps (no client
certificates, no CRL/OCSP, no CA rotation without restart) are documented in
[`SECURITY.md`](SECURITY.md) and [`docs/SECURITY.md`](docs/SECURITY.md).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for the build, the exact local gate
(format, clippy with `-D warnings`, tests, `cargo audit`), commit style and PR
expectations. This project follows the
[Contributor Covenant 2.1](CODE_OF_CONDUCT.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT License](LICENSE-MIT) at your option.
