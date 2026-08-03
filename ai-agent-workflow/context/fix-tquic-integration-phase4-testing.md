# Phase 4: Testing & Validation - Context

## Objective
Validate all tQUIC integration fixes work end-to-end with real QUIC data transmission.

## Test Implementation Plan

### 1. Unit Tests (test_worker_pool.rs)
Add to existing test file:

```rust
#[cfg(test)]
mod tquic_integration_tests {
    use super::*;
    use tokio::sync::oneshot;
    
    #[tokio::test]
    async fn test_connection_id_translation_bidirectional() {
        let mut worker = TquicWorker::new(WorkerConfig::default_client()).unwrap();
        
        // Simulate connection establishment
        let remote_addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();
        let our_conn_id = worker.handle_connect(remote_addr).unwrap();
        
        // Verify forward mapping
        let tquic_id = worker.connections.get(&our_conn_id).copied();
        assert!(tquic_id.is_some());
        
        // Verify reverse mapping
        let reverse_id = worker.reverse_connections.get(&tquic_id.unwrap()).copied();
        assert_eq!(reverse_id, Some(our_conn_id));
    }
    
    #[tokio::test]
    async fn test_stream_send_receive_unit() {
        // This tests the worker methods directly with mocked endpoint
        // Requires mocking tQUIC endpoint - may need mockall or manual mock
    }
}
```

### 2. Integration Test (NEW FILE: tests/tquic_integration.rs)
```rust
use quiccrunch_core::transport::tquic::{TquicClient, TquicServer, TquicConnection, TquicStream};
use std::net::SocketAddr;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_simple_connect_send_receive() {
    // Start server
    let server = TquicServer::new(Some(2)).await.unwrap();
    let mut listener = server.listen("127.0.0.1:4433").await.unwrap();
    
    // Start client
    let client = TquicClient::new(Some(2)).await.unwrap();
    let conn = timeout(Duration::from_secs(5), client.connect("127.0.0.1:4433"))
        .await.unwrap().unwrap();
    
    // Open stream
    let mut stream = timeout(Duration::from_secs(5), conn.open_stream())
        .await.unwrap().unwrap();
    
    // Send data
    let test_data = b"Hello, tQUIC!";
    timeout(Duration::from_secs(5), stream.send(test_data, false))
        .await.unwrap().unwrap();
    
    // Server accepts connection
    let server_conn = timeout(Duration::from_secs(5), listener.accept())
        .await.unwrap().unwrap();
    
    // Server accepts stream
    let mut server_stream = timeout(Duration::from_secs(5), server_conn.accept_stream())
        .await.unwrap().unwrap();
    
    // Server receives data
    let received = timeout(Duration::from_secs(5), server_stream.receive(1024))
        .await.unwrap().unwrap();
    
    assert_eq!(received, test_data);
}

#[tokio::test]
async fn test_bidirectional_streaming() {
    let server = TquicServer::new(Some(2)).await.unwrap();
    let mut listener = server.listen("127.0.0.1:4434").await.unwrap();
    
    let client = TquicClient::new(Some(2)).await.unwrap();
    let conn = client.connect("127.0.0.1:4434").await.unwrap();
    
    let mut client_stream = conn.open_stream().await.unwrap();
    
    let server_conn = listener.accept().await.unwrap();
    let mut server_stream = server_conn.accept_stream().await.unwrap();
    
    // Client -> Server
    client_stream.send(b"Client to Server", false).await.unwrap();
    let received = server_stream.receive(1024).await.unwrap();
    assert_eq!(received, b"Client to Server");
    
    // Server -> Client
    server_stream.send(b"Server to Client", false).await.unwrap();
    let received = client_stream.receive(1024).await.unwrap();
    assert_eq!(received, b"Server to Client");
    
    // Close with FIN
    client_stream.send(b"", true).await.unwrap();
    let received = server_stream.receive(1024).await.unwrap();
    assert_eq!(received, b"");  // FIN received
}

#[tokio::test]
async fn test_concurrent_streams() {
    let server = TquicServer::new(Some(2)).await.unwrap();
    let mut listener = server.listen("127.0.0.1:4435").await.unwrap();
    
    let client = TquicClient::new(Some(2)).await.unwrap();
    let conn = client.connect("127.0.0.1:4435").await.unwrap();
    
    // Open multiple concurrent streams
    let mut handles = vec![];
    for i in 0..10 {
        let mut stream = conn.open_stream().await.unwrap();
        let data = format!("Message {}", i).into_bytes();
        
        handles.push(tokio::spawn(async move {
            stream.send(&data, false).await.unwrap();
            stream.receive(1024).await.unwrap()
        }));
    }
    
    let server_conn = listener.accept().await.unwrap();
    
    // Accept and handle all streams
    for _ in 0..10 {
        let mut s = server_conn.accept_stream().await.unwrap();
        let received = s.receive(1024).await.unwrap();
        s.send(&received, false).await.unwrap();  // Echo back
    }
    
    // Wait for all client operations
    for handle in handles {
        let response = handle.await.unwrap();
        assert!(!response.is_empty());
    }
}
```

### 3. Example Validation
Run the bidirectional streaming example:

```bash
# Terminal 1 - Server
cargo run --example bidirectional_streaming --features backend-tquic server

# Terminal 2 - Client  
cargo run --example bidirectional_streaming --features backend-tquic client

# Expected: Real data transmitted, not empty stubs
# Server should log received messages
# Client should log echoed responses
```

### 4. Benchmark Validation
```bash
# Run benchmark suite
cargo run --release --example benchmark_suite --features backend-tquic

# Or run simple benchmark
cargo run --release --bin quiccrunch-benchmark --features backend-tquic

# Should see:
# - Actual throughput numbers (not errors)
# - Latency measurements
# - Connection scaling metrics
```

### 5. Existing Test Suite
```bash
# Run all tQUIC related tests
cargo test --features backend-tquic tquic

# Run transport tests
cargo test --features backend-tquic transport

# Run worker pool tests
cargo test --features backend-tquic test_worker_pool

# Run all tests
cargo test --features backend-tquic
```

## Validation Checklist

### Functional Tests
- [ ] `test_simple_connect_send_receive` passes
- [ ] `test_bidirectional_streaming` passes  
- [ ] `test_concurrent_streams` passes
- [ ] All existing unit tests still pass
- [ ] No race conditions in connection mapping

### Example Tests
- [ ] `bidirectional_streaming` server starts without error
- [ ] `bidirectional_streaming` client connects successfully
- [ ] Real data logged on both sides (not empty)
- [ ] Multiple messages work correctly
- [ ] FIN handling works (stream closes cleanly)

### Benchmark Tests
- [ ] Benchmark runs without panics
- [ ] Throughput > 0 (actual data transfer)
- [ ] Latency measurements reasonable (< 100ms local)
- [ ] Multiple connections scale correctly

### Regression Tests
- [ ] All previously passing tests still pass
- [ ] No memory leaks in long-running tests
- [ ] Connection close cleans up properly
- [ ] Server can handle multiple sequential clients

## Performance Baselines (Post-Fix Targets)

| Metric | Target | Notes |
|--------|--------|-------|
| Simple send/receive | < 10ms | Local loopback |
| 10 concurrent streams | < 50ms total | All streams |
| 1000 messages | > 10k msg/sec | Baseline |
| Connection setup | < 100ms | Including TLS |

## Debugging Tips

### Enable tQUIC Debug Logging
```rust
// In test setup
std::env::set_var("RUST_LOG", "tquic=debug,quiccrunch=debug");
tracing_subscriber::fmt().init();
```

### Common Issues
1. **Connection timeout** - Check server cert paths, port binding
2. **Stream receive returns empty** - May be `Error::Done`, not actual data
3. **Connection not found** - ID translation not working
4. **Test hangs** - Missing timeout on async operations

### Packet Capture
```bash
# Capture QUIC packets for debugging
sudo tcpdump -i lo -w quic.pcap udp port 4433
# Analyze with Wireshark (QUIC dissector)
```

## Dependencies
- Phases 1-3 complete
- tQUIC v1.6.0 with proper TLS certs

## Estimated Effort
2-3 hours focused development + test execution time

---
*Context for Phase 4 of: Fix tQUIC Integration Gaps*
*Rust Implementation Context*
*Last updated: 2025-01-16*