# QUICCRUNCH Benchmark Implementation

This directory contains a complete QUICCRUNCH implementation that mirrors the gRPC baseline for fair performance comparison.

## Features

### Complete Protocol Compatibility
- **Unary RPC**: Simple request-response pattern
- **Client Streaming**: Client sends stream of messages, server responds once
- **Server Streaming**: Client sends one message, server responds with stream
- **Bidirectional Streaming**: Full duplex communication
- **Legacy Methods**: Latency test, flood test, stream test for backward compatibility

### QUICCRUNCH Integration
- Uses existing QUICCRUNCH RPC framework from `quiccrunch-core`
- Leverages tQUIC backend for optimal performance
- Compatible with existing Cap'n Proto serialization
- Follows established service registration patterns

### Performance Features
- Same workloads as gRPC baseline for fair comparison
- Configurable payload sizes and patterns
- Comprehensive metrics collection
- Detailed latency and throughput statistics
- Error tracking and reporting

## Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Client        │    │                  │    │   Server       │
│                 │    │   QUICCRUNCH    │    │                 │
│ ┌─────────────┐ │    │   RPC Framework │    │ ┌─────────────┐ │
│ │ Benchmark   │─┼────┤                  │────┼─│ Benchmark   │ │
│ │ Client      │ │    │                  │    │ │ Server      │ │
│ └─────────────┘ │    │                  │    │ └─────────────┘ │
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                       │                       │
         │                       │                       │
    ┌────────┐            ┌────────┐            ┌────────┐
    │ JSON   │            │ tQUIC  │            │ JSON   │
    │ Proto  │            │ Transport│           │ Proto  │
    └────────┘            └────────┘            └────────┘
```

## Usage

### Server

Start the QUICCRUNCH benchmark server:

```bash
# Basic usage
cargo run --bin quiccrunch-server

# Custom configuration
cargo run --bin quiccrunch-server -- \
  --address "https://0.0.0.0:8443" \
  --payload-size 2048 \
  --processing-delay 50 \
  --cpu-work 2000
```

#### Server Options
- `--address`: Server address to bind to (default: `https://127.0.0.1:8443`)
- `--payload-size`: Response payload size in bytes (default: 1024)
- `--processing-delay`: Processing delay in microseconds (default: 100)
- `--cpu-work`: CPU work intensity iterations (default: 1000)

### Client

Run individual benchmark tests:

```bash
# Unary test
cargo run --bin quiccrunch-client -- --test-type unary --num-messages 1000

# Client streaming test
cargo run --bin quiccrunch-client -- --test-type client-streaming --num-messages 500

# Server streaming test
cargo run --bin quiccrunch-client -- --test-type server-streaming

# Bidirectional streaming test
cargo run --bin quiccrunch-client -- --test-type bidirectional --num-messages 200

# Latency test
cargo run --bin quiccrunch-client -- --test-type latency --num-messages 10000

# Flood test
cargo run --bin quiccrunch-client -- --test-type flood --num-messages 5000
```

#### Client Options
- `--address`: Server address to connect to (default: `https://127.0.0.1:8443`)
- `--test-type`: Type of test (`unary`, `client-streaming`, `server-streaming`, `bidirectional`, `latency`, `flood`)
- `--message-size`: Message size in bytes (default: 1024)
- `--num-messages`: Number of messages to send (default: 1000)
- `--payload-pattern`: Payload pattern (`random`, `sequential`, `zeroes`, `repeating`)
- `--pattern-data`: Data for repeating pattern (default: "QUICCRUNCH")

### Benchmark Runner

Run comprehensive benchmark suites:

```bash
# Run all tests
cargo run --bin quiccrunch-benchmark-runner

# Custom test selection
cargo run --bin quiccrunch-benchmark-runner -- \
  --test-types "unary,bidirectional,latency" \
  --message-size 2048 \
  --num-messages 2000
```

#### Runner Options
- `--address`: Server address to connect to
- `--message-size`: Message size in bytes (default: 1024)
- `--num-messages`: Number of messages per test (default: 1000)
- `--test-types`: Comma-separated list of test types
- `--delay-between-tests`: Delay between tests in seconds (default: 2)

### Demo

Run demonstration scenarios:

```bash
# Quick demo of all streaming types
cargo run --bin quiccrunch-demo -- --demo-type quick

# Comprehensive demo with different sizes and patterns
cargo run --bin quiccrunch-demo -- --demo-type comprehensive

# Stress test with high loads
cargo run --bin quiccrunch-demo -- --demo-type stress
```

## Performance Comparison

This implementation is designed for fair performance comparison with gRPC:

### Identical Workloads
- Same message sizes and payload patterns
- Same processing delays and CPU work
- Same metrics collection methodology
- Same error handling patterns

### QUICCRUNCH Advantages
- **Lower Latency**: QUIC protocol overhead vs HTTP/2
- **Better Multiplexing**: Native stream multiplexing
- **Reduced RTTs**: 0-RTT and 1-RTT connection establishment
- **Improved Congestion Control**: BBR and modern algorithms

### Metrics Collected
- **Latency**: Min, Max, Mean, P50, P95, P99, P99.9
- **Throughput**: Messages/sec, MB/sec, Mbps
- **Errors**: Connection, timeout, protocol, other errors
- **Resource Usage**: CPU and memory consumption

## Building

```bash
# Build all components
cargo build --release

# Build specific binary
cargo build --release --bin quiccrunch-server
cargo build --release --bin quiccrunch-client
cargo build --release --bin quiccrunch-benchmark-runner
cargo build --release --bin quiccrunch-demo
```

## Testing

```bash
# Run unit tests
cargo test

# Run integration tests
cargo test --test integration

# Run benchmarks
cargo bench
```

## Protocol Compatibility

The implementation uses JSON serialization for compatibility with the existing protobuf definitions:

```json
// SimpleRequest
{
  "id": "test-123",
  "timestamp": 1640995200000000,
  "payload": "base64-encoded-data",
  "test_type": "unary",
  "sequence_number": 0
}

// SimpleResponse
{
  "id": "test-123",
  "timestamp": 1640995200000000,
  "payload": "base64-encoded-data",
  "status": "success",
  "metrics": {
    "concurrent_connections": 5,
    "total_messages": 1000,
    "cpu_usage": 15.5,
    "memory_usage": 52428800
  }
}
```

## Error Handling

The implementation provides comprehensive error handling:

- **Connection Errors**: Network connectivity issues
- **Timeout Errors**: Request timeouts
- **Protocol Errors**: Serialization/deserialization failures
- **Other Errors**: Uncategorized failures

All errors are tracked and reported in the final metrics.

## Configuration

### Server Configuration
```rust
ServerConfig {
    payload_size: 1024,              // Response payload size
    processing_delay: Duration::from_micros(100),  // Simulated processing time
    cpu_work_intensity: 1000,         // CPU work iterations
}
```

### Client Configuration
```rust
BenchmarkConfig {
    server_address: "https://127.0.0.1:8443".to_string(),
    message_size: 1024,               // Request payload size
    num_messages: 1000,               // Number of messages
    concurrent_connections: 1,         // Connection pool size
    test_duration_secs: 30,           // Test timeout
    warmup_duration_secs: 5,          // Warmup period
    payload_pattern: PayloadPattern::Random,  // Data generation
}
```

## Integration with Existing Codebase

This implementation leverages existing QUICCRUNCH components:

- **RPC Framework**: Uses `quiccrunch-core::rpc` module
- **Transport Layer**: Uses `quiccrunch-core::transport::tquic`
- **TLS Configuration**: Uses `quiccrunch-core::TlsConfig`
- **Error Types**: Uses `quiccrunch_core::rpc::RpcError`
- **Streaming**: Uses `RpcStreamSink` and `RpcStreamReceiver`

## Future Enhancements

- **Cap'n Proto Integration**: Replace JSON with Cap'n Proto for better performance
- **Connection Pooling**: Multi-connection client support
- **Advanced Metrics**: Integration with Prometheus/Grafana
- **Load Balancing**: Multi-server support
- **Security**: Mutual TLS authentication

## License

Licensed under the same terms as the QUICCRUNCH project (Apache-2.0 OR MIT).