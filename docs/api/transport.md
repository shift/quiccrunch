# Transport API Reference

The transport module provides the core functionality for QUIC-based communication in QuicCrunch.

## QuicTransport

The main transport implementation that manages QUIC connections.

### Creation

```rust
use quiccrunch::{QuicTransport, TlsConfig};

// Create with default configuration
let transport = QuicTransport::new(tls_config)?;

// Create with custom configuration
let transport = QuicTransport::with_config(tls_config, transport_config)?;
```

### Methods

#### `new(tls_config: TlsConfig) -> Result<Self>`

Creates a new QuicTransport instance with default configuration.

Parameters:
- `tls_config`: TLS configuration containing certificates and keys

Returns:
- `Result<QuicTransport>`: New transport instance or error

#### `with_config(tls_config: TlsConfig, transport_config: TransportConfig) -> Result<Self>`

Creates a new QuicTransport instance with custom configuration.

Parameters:
- `tls_config`: TLS configuration containing certificates and keys
- `transport_config`: Custom transport configuration

Returns:
- `Result<QuicTransport>`: New transport instance or error

### Transport Trait Implementation

#### `async fn connect(&self, addr: &str) -> Result<Connection>`

Connects to a remote endpoint.

Parameters:
- `addr`: Address to connect to (e.g., "127.0.0.1:50051")

Returns:
- `Result<Connection>`: Active connection or error

Example:
```rust
let conn = transport.connect("127.0.0.1:50051").await?;
```

#### `async fn listen(&self, addr: &str) -> Result<Incoming>`

Starts listening for incoming connections.

Parameters:
- `addr`: Address to listen on (e.g., "127.0.0.1:50051")

Returns:
- `Result<Incoming>`: Listener or error

Example:
```rust
let incoming = transport.listen("127.0.0.1:50051").await?;
```

## Connection

Represents an active QUIC connection.

### Methods

#### `async fn open_bi(&self) -> Result<QuicStream>`

Opens a new bidirectional stream.

Returns:
- `Result<QuicStream>`: New stream or error

#### `fn flow_control_status(&self) -> Result<FlowControlInfo>`

Gets current connection flow control information.

Returns:
- `Result<FlowControlInfo>`: Flow control metrics

Example:
```rust
let info = conn.flow_control_status()?;
println!("RTT: {:?}", info.rtt);
```

## Incoming

Handles incoming connections.

### Methods

#### `async fn accept(&mut self) -> Result<Connection>`

Accepts an incoming connection.

Returns:
- `Result<Connection>`: New connection or error

#### `fn local_addr(&self) -> Result<String>`

Gets the local address this listener is bound to.

Returns:
- `Result<String>`: Local address or error

## Configuration

### TlsConfig

Configuration for TLS security:

```rust
pub struct TlsConfig {
    pub cert: Certificate,      // Server/client certificate
    pub key: PrivateKey,       // Private key
    pub ca_cert: Certificate,  // CA certificate
}
```

### Flow Control Information

Metrics available through `flow_control_status()`:

```rust
pub struct FlowControlInfo {
    pub bytes_sent: u64,         // Bytes in flight
    pub window_size: u64,        // Current window size
    pub rtt: Duration,           // Round-trip time
    pub congestion_events: u64,  // Number of congestion events
}
```

## Best Practices

1. **Connection Management**
   - Cache and reuse connections when possible
   - Implement proper error handling and retry logic
   - Set appropriate timeouts for your use case
   - Clean up resources by closing unused connections

2. **Configuration**
   - Tune transport configuration based on workload:
     ```rust
     let mut config = TransportConfig::default();
     config
         .max_concurrent_uni_streams(1000_u32.into())
         .keep_alive_interval(Some(Duration::from_secs(5)))
         .initial_rtt(Duration::from_millis(100))
         .initial_window_size(65535);
     ```
   - Adjust flow control parameters based on message sizes
   - Configure appropriate idle timeouts

3. **TLS Security**
   - Always validate certificates
   - Use secure certificate generation and storage
   - Implement certificate rotation
   - Keep TLS libraries updated

4. **Error Handling**
   - Handle connection failures gracefully
   - Implement reconnection logic
   - Log and monitor connection errors
   - Handle stream errors independently

## Common Patterns

### Server Implementation
```rust
let mut incoming = transport.listen("127.0.0.1:50051").await?;

while let Ok(conn) = incoming.accept().await {
    tokio::spawn(async move {
        while let Ok(stream) = conn.open_bi().await {
            // Handle stream
        }
    });
}
```

### Client Implementation
```rust
let conn = transport.connect("127.0.0.1:50051").await?;
let stream = conn.open_bi().await?;

// Send/receive messages
stream.send_message(request).await?;
let response = stream.receive_message().await?;
```

### Flow Control Monitoring
```rust
let conn = transport.connect("127.0.0.1:50051").await?;

// Periodically monitor flow control
loop {
    let info = conn.flow_control_status()?;
    if info.congestion_events > threshold {
        // Handle congestion
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```