# QuicCrunch

A high-performance RPC framework built on QUIC transport with multi-worker architecture for extreme throughput.

## Features

- 🚀 **Multi-worker architecture** - True parallelism across CPU cores with SO_REUSEPORT
- 📦 **Zero-copy message handling** - bytes::Bytes throughout the write path
- 🔒 **Built-in TLS security** - Hardware-accelerated cryptography (AES-NI, AVX)
- ⚡ **High throughput** - 30,000+ msg/s with 4 CPU cores
- 🛡️ **Production ready** - Health checks, graceful shutdown, rate limiting
- 📊 **Observability** - OpenTelemetry/Prometheus metrics, structured logging
- 🔐 **Authentication** - JWT tokens, RBAC, permission-based access control

## Performance

| Configuration | Throughput | Latency |
|--------------|------------|---------|
| Single worker | 5,000 msg/s | 194 μs |
| **4 workers (SO_REUSEPORT)** | **31,629 msg/s** | **32 μs** |

### Benchmark Results (4 workers, 16 connections)

| Message Size | Throughput | Bandwidth |
|-------------|------------|-----------|
| 64B | 31,629 msg/s | 34 Mbps |
| 256B | 30,134 msg/s | 125 Mbps |
| 1024B | 24,805 msg/s | 408 Mbps |
| 4096B | 12,926 msg/s | 848 Mbps |

## Quick Start

### Multi-Worker Server (Recommended)

```bash
# Start server with 4 workers
cargo run --bin multi_worker_server -- 127.0.0.1:4433 \
    quiccrunch-core/tests/certs/cert.pem \
    quiccrunch-core/tests/certs/key.pem \
    4
```

### Run Benchmark

```bash
# 16 connections, pipeline depth 1800, 1024B messages
cargo run --bin pipelined_benchmark -- 127.0.0.1:4433 \
    1800 1800 16 1024 4
```

## Architecture

### Multi-Worker with SO_REUSEPORT

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

Each worker has its own `TquicRuntime` running on its own OS thread. The kernel distributes UDP packets across workers via SO_REUSEPORT.

### RPC Framework

```rust
use quiccrunch_core::rpc::service::{ServiceDefinition, MethodDefinition, StreamingType};

// Define a service
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
use quiccrunch_core::interceptor::auth::{AuthInterceptor, Claims};

// Create auth interceptor with JWT
let auth = AuthInterceptor::with_secret("secret-key".to_string());

// Generate token
let token = auth.generate_token(
    "user123",
    vec!["admin".to_string()],
    vec!["read".to_string(), "write".to_string()],
    HashMap::new(),
)?;

// Verify token
let claims = auth.verify_token(&token)?;
```

### Rate Limiting

```rust
use quiccrunch_core::rpc::rate_limit::{RateLimitInterceptor, DdosProtectionConfig};

// Create rate limiter
let rate_limiter = RateLimitInterceptor::new(
    1000,  // requests per second
    5000,  // burst capacity
    DdosProtectionConfig::default(),
);

// Check rate limit
rate_limiter.check_rate_limit("client1", "my_service", 1024).await?;
```

### Metrics

```rust
use quiccrunch_core::rpc::metrics::RpcMetrics;

let metrics = RpcMetrics::new();

// Record request
metrics.record_request("my_service", "my_method", duration).await;

// Get Prometheus format
let prometheus = metrics.to_prometheus().await;
```

## Binaries

| Binary | Description |
|--------|-------------|
| `multi_worker_server` | Multi-worker QUIC server (recommended) |
| `mt_server` | Single-runtime server (for comparison) |
| `pipelined_benchmark` | High-performance benchmark client |
| `quic_server` | Production server with logging |
| `quic_client` | Production client |

## Configuration

### Environment Variables

```bash
# Logging
RUST_LOG=info        # Set log level
RUST_LOG=debug       # Verbose output

# Server
RUST_LOG=warn ./target/debug/multi_worker_server 127.0.0.1:4433 cert.pem key.pem 4
```

### Command Line Arguments

```bash
# multi_worker_server: address cert_file key_file worker_count
./target/debug/multi_worker_server 0.0.0.0:8443 cert.pem key.pem 8

# pipelined_benchmark: address pipeline_depth messages_per_stream connections msg_size workers
./target/debug/pipelined_benchmark 127.0.0.1:4433 1800 1800 16 1024 4
```

## Features

### Production Infrastructure

- **Health Checks**: `/health/live`, `/health/ready` endpoints
- **Graceful Shutdown**: Signal handling (SIGTERM, SIGINT) with connection draining
- **Configuration**: JSON/TOML/YAML config files, environment variables
- **Structured Logging**: Correlation IDs, JSON-formatted logs

### RPC Framework

- **Service Reflection**: Introspect services and methods at runtime
- **Authentication**: JWT tokens with RS256/HS256 support
- **Authorization**: Role-based access control (RBAC)
- **Rate Limiting**: Token bucket algorithm with per-client limits
- **DDoS Protection**: Auto-blocking after violations
- **Metrics**: OpenTelemetry/Prometheus integration
- **SIMD Parsing**: AVX2-optimized message parsing

## Building

### Using Nix (Recommended)

```bash
nix develop -c bash -c "cargo build --release"
```

### Using Cargo

```bash
cargo build --release
```

## License

Apache-2.0 OR MIT
