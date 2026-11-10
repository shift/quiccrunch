# QuicCrunch

A high-performance RPC framework built on QUIC transport with multi-worker architecture for extreme throughput.

[![CI](https://github.com/shift/quiccrunch/actions/workflows/ci.yml/badge.svg)](https://github.com/shift/quiccrunch/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)

## Highlights

- **Multi-worker QUIC server** — true parallelism across CPU cores via SO_REUSEPORT
- **Hardware-accelerated TLS** — AES-NI, AVX, SHA-NI with automatic feature detection
- **Cap'n Proto serialization** — zero-copy message encoding/decoding
- **Built-in RPC framework** — service reflection, auth, rate limiting, metrics
- Measured throughput and true percentiles (see [`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md)); tail latency is dominated by pipeline queueing, not transport RTT

## Performance

QuicCrunch is measured with an instrumented benchmark that timestamps every
request and reports true RTT, end-to-end latency and completed-response
throughput. Reproduce with `scripts/bench.sh`; the numbers, methodology,
hardware and known limits are in [`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md).

The previous table (5,000 msg/s / 194 µs and 31,629 msg/s / 32 µs) was
arithmetic, not measurement: `p99` was `1 / throughput`, individual requests
were never timed, and throughput counted sends regardless of completions. Those
numbers have been withdrawn.

## Getting Started

### Prerequisites

- Rust 1.75+ (stable recommended)
- Linux (SO_REUSEPORT support required for multi-worker)
- cmake, clang, pkg-config, libssl-dev

### Build

```bash
# With Nix (recommended)
nix develop --command bash -c "cargo build --release"

# With Cargo
cargo build --release
```

### Run the Server

```bash
# Generate test certs (for development only)
openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem \
  -days 365 -nodes -subj '/CN=localhost'

# Start multi-worker server
cargo run --bin multi_worker_server -- \
  127.0.0.1:4433 cert.pem key.pem 4
```

### Run a Benchmark

```bash
cargo run --bin pipelined_benchmark -- \
  127.0.0.1:4433 1800 1800 16 1024 4
```

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

Each worker runs its own `TquicRuntime` on a dedicated OS thread. The kernel distributes incoming UDP packets across workers via SO_REUSEPORT — no load balancer required.

## Usage

### Define an RPC Service

```rust
use quiccrunch_core::rpc::service::{ServiceDefinition, MethodDefinition, StreamingType};

let mut service = ServiceDefinition::new(1, "Calculator");
service.add_method(MethodDefinition {
    id: 1,
    name: "Add".to_string(),
    streaming_type: StreamingType::Unary,
    request_schema: None,
    response_schema: None,
});
```

### Authentication

```rust
use quiccrunch_core::interceptor::auth::AuthInterceptor;

let auth = AuthInterceptor::with_secret("secret-key".to_string());
let token = auth.generate_token(
    "user123",
    vec!["admin".to_string()],
    vec!["read".to_string(), "write".to_string()],
    HashMap::new(),
)?;
let claims = auth.verify_token(&token)?;
```

### Rate Limiting

```rust
use quiccrunch_core::rpc::rate_limit::{RateLimitInterceptor, DdosProtectionConfig};

let rate_limiter = RateLimitInterceptor::new(
    1000,  // requests per second
    5000,  // burst capacity
    DdosProtectionConfig::default(),
);
rate_limiter.check_rate_limit("client1", "my_service", 1024).await?;
```

### Prometheus Metrics

```rust
use quiccrunch_core::rpc::metrics::RpcMetrics;

let metrics = RpcMetrics::new();
metrics.record_request("my_service", "my_method", duration).await;
let prometheus = metrics.to_prometheus().await;
```

## Workspace Crates

| Crate | Description |
|-------|-------------|
| `quiccrunch-core` | Core library — transport, RPC, crypto, metrics |
| `quiccrunch-cli` | Command-line client |
| `quiccrunch-macros` | Procedural macros for RPC codegen |
| `quiccrunch-ffi` | C FFI bindings |

## Features

- **Transport**: QUIC via tQUIC backend, multi-stream multiplexing
- **Serialization**: Cap'n Proto with SIMD-accelerated parsing (AVX2)
- **Security**: JWT auth (HS256/RS256), RBAC, TLS 1.3 with hardware acceleration
- **Observability**: OpenTelemetry/Prometheus metrics, structured logging with correlation IDs
- **Resilience**: Token bucket rate limiting, DDoS protection with auto-blocking
- **Production**: Health checks (`/health/live`, `/health/ready`), graceful shutdown

## Contributing

Contributions are welcome! Please open an issue first to discuss what you'd like to change.

```bash
# Run the full test suite
cargo test --all-features

# Check formatting and lints
cargo fmt --all -- --check
cargo clippy --all-targets --all-features
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.
