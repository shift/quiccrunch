# QuicCrunch

A high-performance RPC library combining QUIC transport with Cap'n Proto serialization.

## Features

- 🚀 QUIC-based transport with stream multiplexing
- 📦 Zero-copy message handling
- 🔒 Built-in TLS security
- ⚡ Low latency and high throughput
- 🛠 Highly configurable

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
quiccrunch = "0.1"
```

## Quick Start

```rust
use quiccrunch::{QuicTransport, Transport, TlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generate or load certificates
    let (ca_cert, ca_key) = generate_ca_cert()?;
    let (cert, key) = generate_node_cert(&ca_cert, "server1")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Server
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            let mut stream = conn.open_bi().await?;
            let request = stream.receive_message().await?;
            stream.send_message(response).await?;
        }
    });
    
    // Client
    let conn = transport.connect("127.0.0.1:50051").await?;
    let mut stream = conn.open_bi().await?;
    
    stream.send_message(request).await?;
    let response = stream.receive_message().await?;

    Ok(())
}
```

## API Reference

### Core Types

#### QuicTransport

The main transport implementation that handles QUIC connections.

```rust
// Create with default configuration
let transport = QuicTransport::new(tls_config)?;

// Create with custom configuration
let transport = QuicTransport::with_config(tls_config, transport_config)?;
```

#### Transport Trait

The core trait defining transport behavior:

```rust
#[async_trait]
pub trait Transport {
    // Connect to a remote endpoint
    async fn connect(&self, addr: &str) -> Result<Connection>;
    
    // Start listening for incoming connections
    async fn listen(&self, addr: &str) -> Result<Incoming>;
}
```

#### Connection

Represents an active QUIC connection:

```rust
// Open a new bidirectional stream
let stream = connection.open_bi().await?;

// Get flow control information
let flow_info = connection.flow_control_status()?;
println!("RTT: {:?}, Window Size: {}", flow_info.rtt, flow_info.window_size);
```

#### Incoming

Handles incoming connections:

```rust
let mut incoming = transport.listen("127.0.0.1:50051").await?;

// Accept new connections
while let Ok(conn) = incoming.accept().await {
    // Handle connection
}

// Get bound address
let addr = incoming.local_addr()?;
```

#### QuicStream

Represents a bidirectional QUIC stream:

```rust
// Send a message
stream.send_message(bytes::Bytes::from("Hello")).await?;

// Receive a message
let msg = stream.receive_message().await?;
```

### Configuration

#### TlsConfig

Configure TLS security:

```rust
let tls_config = TlsConfig {
    cert: Certificate,      // Server/client certificate
    key: PrivateKey,       // Private key
    ca_cert: Certificate,  // CA certificate for verification
};
```

#### Transport Configuration

Fine-tune transport behavior:

```rust
use quinn::TransportConfig;

let mut config = TransportConfig::default();
config
    .max_concurrent_uni_streams(1000_u32.into())
    .keep_alive_interval(Some(Duration::from_secs(5)))
    .initial_rtt(Duration::from_millis(100))
    .initial_window_size(65535)
    .max_window_size(16_777_215)
    .receive_window(1_048_576)
    .send_window(1_048_576);

let transport = QuicTransport::with_config(tls_config, config)?;
```

### Flow Control

Monitor and manage flow control:

```rust
let flow_info = connection.flow_control_status()?;
println!("Connection stats:
    Bytes in flight: {}
    Window size: {}
    RTT: {:?}
    Congestion events: {}",
    flow_info.bytes_sent,
    flow_info.window_size,
    flow_info.rtt,
    flow_info.congestion_events
);
```

### Best Practices

1. **Connection Management**
   - Reuse connections when possible
   - Handle connection errors gracefully
   - Set appropriate timeouts for your use case

2. **Stream Usage**
   - Use multiple streams for concurrent operations
   - Close streams when done to free resources
   - Handle stream errors independently

3. **Flow Control**
   - Monitor flow control metrics
   - Adjust window sizes based on workload
   - Be mindful of memory usage with large windows

4. **Security**
   - Use proper certificate validation
   - Rotate certificates regularly
   - Keep TLS configuration up to date

5. **Performance**
   - Batch messages when possible
   - Use appropriate buffer sizes
   - Monitor and tune based on metrics

## Performance

QuicCrunch includes several performance optimizations that can be enabled based on your workload:

### Performance Features

- **Memory Pool Optimizations**: Thread-local buffer pools that provide significant benefits for medium to large messages (8KB+)
- **Adaptive Strategy Selection**: Automatically chooses the best optimization approach based on message size
- **Lock-Free Channels**: High-concurrency channel implementation for multi-stream workloads
- **Zero-copy message handling**: Where possible to minimize memory allocations
- **Stream multiplexing**: Avoids head-of-line blocking

### Build Configurations

Enable performance optimizations based on your workload:

```toml
# Small messages (<8KB) - use defaults
quiccrunch = { version = "0.1", features = ["backend-tquic"] }

# Medium messages (8KB-64KB) - enable memory pools
quiccrunch = { version = "0.1", features = ["backend-tquic", "memory-pools"] }

# Large messages (64KB+) - full optimizations
quiccrunch = { version = "0.1", features = ["backend-tquic", "adaptive-optimizations"] }

# High concurrency workloads - add lock-free channels
quiccrunch = { version = "0.1", features = ["backend-tquic", "adaptive-optimizations", "lock-free-channels"] }
```

### Performance Characteristics

Based on actual testing:
- **Memory pools**: 9-10% improvement for medium to large messages
- **Adaptive strategies**: Automatic optimization selection prevents performance regressions
- **tQUIC backend**: ~3x better performance than Quinn backend in most scenarios

**Note**: SIMD optimizations are currently experimental and may not provide benefits in all scenarios.

### Power Monitoring

QuicCrunch includes built-in power monitoring capabilities:

```rust
use quiccrunch::power::{PowerMonitor, PowerMetrics};

// Initialize power monitoring
let power_monitor = PowerMonitor::new()?;
let mut metrics = PowerMetrics::new();

// Monitor during operations
let power_data = power_monitor.sample()?;
metrics.record_sample(&power_data);

// Analyze efficiency
println!("Power efficiency: {:.3} W/MHz", power_data.power_efficiency());
```

See `docs/POWER_MONITORING_REPORT.md` for detailed analysis and results.

## Security

Security is enabled by default:

- TLS 1.3 encryption
- Certificate-based authentication
- Built-in certificate generation utilities

## Examples and Containerization

QuicCrunch includes comprehensive examples showing how to build and containerize applications:

- **Calculator Service**: Mathematical RPC service with JSON protocol
- **Chat Service**: Multi-user chat with rooms and message history
- **Echo Service**: Simple request/response demonstration
- **Development Environment**: Complete Docker-based development setup
- **Integration Tests**: Multi-service testing framework
- **Performance Benchmarks**: Latency, throughput, and load testing

### Quick Start with Examples

```bash
# Navigate to examples directory
cd examples

# Start all example services
make up

# Test calculator service
docker-compose exec calculator-server calculator-client --operation add --a 5 --b 3

# Test chat service interactively
docker-compose run --rm chat-server chat-client --user alice --interactive

# Run integration tests
make test-integration

# Run performance benchmarks
make test-performance
```

See [examples/README.md](examples/README.md) for complete documentation.

## Testing

Run the test suite:

```bash
nix develop -c cargo test
```

Scaling tests (require sudo and specific hardware) are excluded by default. Opt in with:

```bash
sudo -E nix develop -c cargo test --features scaling-tests --test scaling_benchmark -- --test-threads=1 --nocapture
```

Run benchmarks:

```bash
nix develop -c cargo bench
```

### Container-based Testing

For comprehensive testing including multi-service integration:

```bash
cd examples
make test           # Run all tests
make dev-env        # Start development environment
make validate       # Validate setup
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license

at your option.

## Contributing

We welcome contributions! Please feel free to submit a Pull Request.

## Coding standards

This project enforces a consistent Rust style and safety baseline:

- Formatting: rustfmt (edition 2021, grouped imports)
- Linting: clippy in CI with -D warnings
- Safety: forbid(unsafe_code) at crate level
- Error handling: thiserror for public error types
- Logging/telemetry: tracing; optional OpenTelemetry via `telemetry` feature
- Testing: unit, property-based (proptest), and integration tests; headless plotting via `HEADLESS_PLOT=1`

Run locally:
```bash
# Format
nix develop -c cargo fmt --all

# Lint (treat warnings as errors)
nix develop -c cargo clippy --all-targets --all-features -- -D warnings

# Tests (safe default)
nix develop -c cargo test

# Scaling benchmark (requires sudo; opt-in)
sudo -E nix develop -c cargo test --features scaling-tests --test scaling_benchmark -- --test-threads=1 --nocapture
```

Guidelines (short version):
- No unwrap/expect/panic in library code; bubble errors with Result and thiserror.
- Avoid blocking in async; prefer Tokio APIs.
- Keep messages under `TransportSettings.stream_receive_window`; enforced in `QuicStream`.
- Plotters in CI must be headless: set `HEADLESS_PLOT=1`.
- Feature flags:
  - `telemetry` to enable OpenTelemetry tracing
  - `scaling-tests` to run privileged scaling tests
