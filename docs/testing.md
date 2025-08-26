# Testing Procedures

This document outlines the testing procedures for QuicCrunch, covering unit tests, integration tests, and benchmarks.

## Running Tests

### Unit Tests

Run the basic test suite (inside devShell):
```bash
nix develop -c cargo test
```

Run tests with logging enabled:
```bash
RUST_LOG=debug nix develop -c cargo test -- --nocapture
```

### Integration Tests

Run integration tests specifically:
```bash
nix develop -c cargo test --test integration_transport
```

### Property-Based Tests

QuicCrunch uses proptest for property-based testing:
```bash
nix develop -c cargo test --test prop_frame
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

## Linting and formatting

Run both before submitting changes:

```bash
nix develop -c cargo fmt --all
nix develop -c cargo clippy --all-targets --all-features -- -D warnings
```

Policies:
- No `unwrap`, `expect`, or `panic!` in non-test code.
- Treat all clippy warnings as errors in CI.
- Use `thiserror` for public error types.

## Headless plotting

Plotting in tests runs headless to be CI-safe. Ensure:
```bash
HEADLESS_PLOT=1 nix develop -c cargo test
```
This disables font rendering and writes PNG buffers directly.

## Privileged scaling tests (opt-in)

The scaling tests require root privileges and specific hardware. They are excluded from the default suite. To run:
```bash
sudo -E nix develop -c cargo test --features scaling-tests --test scaling_benchmark -- --test-threads=1 --nocapture
```

## Continuous Integration

Recommended CI steps:
1. cargo fmt --all -- --check
2. cargo clippy --all-targets --all-features -- -D warnings
3. cargo test (without scaling-tests)

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