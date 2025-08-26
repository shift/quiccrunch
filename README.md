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

QuicCrunch is designed for high performance:

- Zero-copy message handling where possible
- Stream multiplexing to avoid head-of-line blocking
- Efficient binary protocol
- Minimal allocations in hot paths

## Security

Security is enabled by default:

- TLS 1.3 encryption
- Certificate-based authentication
- Built-in certificate generation utilities

## Testing

Run the test suite:

```bash
cargo test
```

Run benchmarks:

```bash
cargo bench
```

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT license

at your option.

## Contributing

We welcome contributions! Please feel free to submit a Pull Request.