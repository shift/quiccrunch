# QuicCrunch Examples

## Multi-Worker Server Example

The `multi_worker_server.rs` example demonstrates the high-performance multi-worker architecture with SO_REUSEPORT.

### Features Demonstrated

- ✅ Multi-worker architecture with separate TquicRuntime per worker
- ✅ SO_REUSEPORT for kernel-level packet distribution
- ✅ True parallelism across CPU cores
- ✅ Length-prefixed message framing
- ✅ Echo server pattern

### Running the Example

Start the server:
```bash
cargo run --bin multi_worker_server -- 127.0.0.1:4433 \
    quiccrunch-core/tests/certs/cert.pem \
    quiccrunch-core/tests/certs/key.pem \
    4
```

Run the benchmark:
```bash
cargo run --bin pipelined_benchmark -- 127.0.0.1:4433 \
    1800 1800 16 1024 4
```

### Architecture

Each worker:
1. Creates its own `TquicRuntime` on a dedicated OS thread
2. Binds to the same UDP port via SO_REUSEPORT
3. Accepts connections independently
4. Processes streams without contention

### Performance

| Workers | Throughput | Latency |
|---------|------------|---------|
| 1 | 5,000 msg/s | 194 μs |
| 4 | **31,629 msg/s** | **32 μs** |

## Benchmark Example

The `pipelined_benchmark.rs` example demonstrates high-throughput client with pipelined I/O.

### Features

- ✅ Multiple concurrent connections
- ✅ Pipelined message processing
- ✅ Length-prefixed framing
- ✅ Throughput and latency measurement

### Usage

```bash
# Basic usage
cargo run --bin pipelined_benchmark -- 127.0.0.1:4433

# Custom parameters
cargo run --bin pipelined_benchmark -- \
    127.0.0.1:4433 \    # server address
    1800 \               # pipeline depth
    1800 \               # messages per stream
    16 \                 # connections
    1024 \               # message size (bytes)
    4                    # workers
```

### Output

```
=== HIGH-PERFORMANCE BENCHMARK RESULTS ===
Duration: 0.974s
Total messages: 28800
Throughput: 29561 msg/s
Throughput: 486.22 Mbps
Avg latency: 33.83 μs
Bytes sent: 30146560 (30.15 MB)
Bytes received: 30146560 (30.15 MB)
Errors: 0
Pipeline depth: 1800
Concurrency: 16
Streams per connection: 4
```

## RPC Examples

### Service Definition

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
use quiccrunch_core::interceptor::auth::{AuthInterceptor, Role, Permission};
use std::collections::HashMap;

// Create auth with secret
let auth = AuthInterceptor::with_secret("my-secret".to_string());

// Generate token
let token = auth.generate_token(
    "user123",
    vec!["admin".to_string()],
    vec!["read".to_string(), "write".to_string()],
    HashMap::new(),
)?;

// Verify token
let claims = auth.verify_token(&token)?;
assert_eq!(claims.sub, "user123");
```

### Rate Limiting

```rust
use quiccrunch_core::rpc::rate_limit::{RateLimitInterceptor, DdosProtectionConfig};

let limiter = RateLimitInterceptor::new(
    1000,   // requests per second
    5000,   // burst capacity
    DdosProtectionConfig::default(),
);

// Check before processing
match limiter.check_rate_limit("client1", "api", 1024).await {
    Ok(()) => { /* proceed */ }
    Err(e) => { /* rate limited */ }
}
```

### Metrics Collection

```rust
use quiccrunch_core::rpc::metrics::RpcMetrics;
use std::time::Duration;

let metrics = RpcMetrics::new();

// Record request
metrics.record_request("my_service", "my_method", Duration::from_millis(50)).await;

// Record response
metrics.record_response("my_service", "my_method", "success", Duration::from_millis(50)).await;

// Get Prometheus format
let prometheus = metrics.to_prometheus().await;
println!("{}", prometheus);
```

### Configuration Management

```rust
use quiccrunch_core::production::ConfigManager;

let config = ConfigManager::new();

// Load from file (JSON, TOML, or YAML)
config.load("config.toml").await?;

// Load from environment
config.load_from_env("MY_APP").await;

// Get value
let host = config.get("server.host").await;
```

### Health Checks

```rust
use quiccrunch_core::production::{ServerHealthCheck, HealthCheck};

let health = ServerHealthCheck::new(1000);

// Liveness probe
let liveness = health.check_liveness().await;
assert_eq!(liveness.status, HealthStatus::Healthy);

// Readiness probe
let readiness = health.check_readiness().await;
```

## Running Examples

### Prerequisites

1. Generate certificates (or use test certs):
```bash
cd quiccrunch-core
openssl req -x509 -newkey rsa:2048 -keyout tests/certs/key.pem \
    -out tests/certs/cert.pem -days 365 -nodes \
    -subj "/CN=localhost"
```

2. Build examples:
```bash
cargo build --examples
```

### Example Commands

```bash
# Multi-worker server
cargo run --bin multi_worker_server -- 127.0.0.1:4433 \
    tests/certs/cert.pem tests/certs/key.pem 4

# Benchmark
cargo run --bin pipelined_benchmark -- 127.0.0.1:4433 1800 1800 16 1024 4

# Production server
cargo run --bin quic_server -- 127.0.0.1:4433 4

# Production client
cargo run --bin quic_client -- 127.0.0.1:4433 100
```
