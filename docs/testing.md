# Testing Procedures

This document outlines the testing procedures for QuicCrunch, covering unit tests, integration tests, and benchmarks.

## Running Tests

### Unit Tests

Run the basic test suite:
```bash
cargo test
```

Run tests with logging enabled:
```bash
RUST_LOG=debug cargo test -- --nocapture
```

### Integration Tests

Run integration tests specifically:
```bash
cargo test --test integration_transport
```

### Property-Based Tests

QuicCrunch uses proptest for property-based testing:
```bash
cargo test --test prop_frame
```

## Test Categories

### Transport Layer Tests
- Connection establishment/teardown
- Stream multiplexing
- Flow control
- Error handling

### TLS Security Tests
- Certificate validation
- Secure handshake
- Error cases
- SNI support

### Performance Tests
- Latency measurements
- Bandwidth tests
- Resource usage
- Connection limits

## Writing Tests

### Unit Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Setup
        let transport = QuicTransport::new(test_config())?;

        // Exercise
        let result = transport.some_operation();

        // Verify
        assert!(result.is_ok());
    }
}
```

### Integration Test Structure

```rust
#[tokio::test]
async fn test_full_flow() {
    // Setup server
    let server = TestServer::new().await?;

    // Setup client
    let client = TestClient::connect(server.addr()).await?;

    // Perform operations
    let response = client.send_message("test").await?;

    // Verify
    assert_eq!(response, expected_response);
}
```

### Property Test Structure

```rust
proptest! {
    #[test]
    fn test_frame_serialization(
        frame in frame_strategy()
    ) {
        let encoded = frame.encode()?;
        let decoded = Frame::decode(&encoded)?;
        prop_assert_eq!(frame, decoded);
    }
}
```

## Test Utilities

### Test Certificates

Use the provided test certificate utilities:
```rust
use quiccrunch::test_utils::certs::{
    generate_test_ca_cert,
    generate_test_node_cert
};
```

### Network Conditions

Simulate various network conditions:
```rust
use quiccrunch::test_utils::network::{
    NetworkConfig,
    apply_network_conditions
};
```

### Test Servers

Use the test server implementation:
```rust
use quiccrunch::test_utils::TestServer;
```

## Continuous Integration

### GitHub Actions Workflow

Tests are run on every pull request:
1. Unit tests
2. Integration tests
3. Property tests
4. Linting checks
5. Documentation tests

### Local CI Checks

Run the same checks locally:
```bash
./scripts/check.sh
```

## Test Coverage

Generate and view test coverage:
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

Coverage targets:
- Line coverage: >80%
- Branch coverage: >70%
- Function coverage: >90%

## Debugging Tests

### Logging

Enable debug logging:
```bash
RUST_LOG=debug cargo test
```

### Tracing

Enable OpenTelemetry tracing:
```bash
RUST_LOG=trace cargo test --features telemetry
```

### Memory Profiling

Run tests with memory profiling:
```rust
#[global_allocator]
static ALLOC: Stats = Stats::new();

#[test]
fn test_with_allocation_tracking() {
    let before = ALLOC.bytes_allocated();
    // Test code
    let after = ALLOC.bytes_allocated();
    println!("Allocated: {} bytes", after - before);
}
```