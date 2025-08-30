# Benchmark Implementation Context

## Task: Comprehensive QuicCrunch vs gRPC Benchmark Suite

### Objective
Implement a complete benchmark suite that compares QuicCrunch and gRPC performance across all streaming patterns with accurate performance measurement.

### Requirements

#### 1. Streaming Patterns to Implement
- **Request/Response (Unary)**: ✅ Already implemented
- **Client Streaming**: Client sends stream of messages, server responds once
- **Server Streaming**: Client sends one message, server responds with stream
- **Bidirectional Streaming**: Full duplex communication

#### 2. Protocol Definitions Required
```protobuf
service BenchmarkService {
  // Unary RPC - already implemented
  rpc UnaryCall(SimpleRequest) returns (SimpleResponse);
  
  // Client streaming RPC - TO IMPLEMENT
  rpc ClientStreaming(stream SimpleRequest) returns (SimpleResponse);
  
  // Server streaming RPC - TO IMPLEMENT
  rpc ServerStreaming(SimpleRequest) returns (stream SimpleResponse);
  
  // Bidirectional streaming RPC - TO IMPLEMENT
  rpc BidirectionalStreaming(stream SimpleRequest) returns (stream SimpleResponse);
}
```

#### 3. Message Size Test Matrix
```rust
const MESSAGE_SIZES: &[usize] = &[
    16, 32, 64, 128, 256, 512, 768,           // Small (B)
    1024, 2048, 4096, 8192,                  // Small (KB)  
    16384, 32768, 65536, 131072,             // Medium (KB)
    1048576, 2097152, 4194304                // Large (MB)
];
```

#### 4. Performance Metrics to Collect
- **Latency**: p50, p95, p99 percentiles
- **Throughput**: Messages per second
- **Resource Usage**: CPU, Memory, Network
- **Error Rates**: Connection failures, message losses
- **Concurrent Connection Performance**: 1, 10, 100, 1000 connections

#### 5. Implementation Files to Create/Modify

##### Core Implementation Files
- `quiccrunch-core/examples/comprehensive_benchmark.rs` - New complete benchmark suite
- `quiccrunch-core/src/benchmark/mod.rs` - Benchmark utilities module
- `quiccrunch-core/src/benchmark/metrics.rs` - Performance measurement utilities
- `quiccrunch-core/src/benchmark/grpc_client.rs` - gRPC client implementation
- `quiccrunch-core/src/benchmark/grpc_server.rs` - gRPC server implementation
- `quiccrunch-core/src/benchmark/quiccrunch_client.rs` - QuicCrunch client implementation
- `quiccrunch-core/src/benchmark/quiccrunch_server.rs` - QuicCrunch server implementation

##### Protocol Definition Updates
- `quiccrunch-core/proto/benchmark.proto` - Add missing streaming methods
- `quiccrunch-core/build.rs` - Update proto build configuration

##### Configuration Updates
- `quiccrunch-core/Cargo.toml` - Add benchmark feature flags and dependencies
- Add `benchmark-suite` feature flag

#### 6. Implementation Architecture

##### Benchmark Configuration Structure
```rust
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub message_size: usize,
    pub streaming_pattern: StreamingPattern,
    pub concurrent_connections: usize,
    pub duration_seconds: u64,
    pub warmup_seconds: u64,
    pub target_qps: Option<u64>, // For rate limiting
}

#[derive(Debug, Clone)]
pub enum StreamingPattern {
    Unary,
    ClientStreaming { message_count: usize },
    ServerStreaming { message_count: usize },
    Bidirectional { message_count: usize },
}
```

##### Performance Metrics Structure
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub latency_percentiles: LatencyMetrics,
    pub throughput: ThroughputMetrics,
    pub resource_usage: ResourceMetrics,
    pub error_metrics: ErrorMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub mean_ms: f64,
    pub std_dev_ms: f64,
}
```

#### 7. Client/Server Implementation Requirements

##### gRPC Implementation
- Use `tonic` framework for gRPC client/server
- Implement all 4 streaming patterns exactly as per protocol
- Use connection pooling for concurrent connections
- Implement proper error handling and retries

##### QuicCrunch Implementation  
- Use existing QuicCrunch stream API
- Implement equivalent functionality to gRPC patterns
- Handle tQUIC backend integration properly (with fallback if needed)
- Ensure thread safety and concurrent connection handling

#### 8. Performance Measurement Requirements

##### Timing Accuracy
- Use high-resolution timers (`std::time::Instant`)
- Measure end-to-end latency including serialization/deserialization
- Record per-message timestamps for streaming patterns

##### Resource Monitoring
- Monitor CPU usage during benchmark runs
- Track memory allocation and peak usage
- Monitor network I/O statistics
- Use `psutil` or similar for system metrics

##### Statistical Analysis
- Calculate proper percentiles using reservoir sampling or histogram
- Implement statistical significance testing
- Generate confidence intervals for measurements

#### 9. Test Data Generation

##### Message Content
```rust
// Generate realistic test data with varying complexity
pub fn generate_test_message(size: usize) -> Vec<u8> {
    // Mix of random data and patterns to simulate real traffic
    let mut data = Vec::with_capacity(size);
    // Implementation details...
}
```

#### 10. Expected Deliverables

##### Implementation Files
1. Complete benchmark suite with all streaming patterns
2. Accurate performance measurement infrastructure
3. Proper client/server implementations for both protocols
4. Configuration management and test matrix execution

##### Test Output Format
```json
{
  "benchmark_run": {
    "timestamp": "2025-08-30T12:00:00Z",
    "config": {
      "message_size": 1024,
      "streaming_pattern": "bidirectional",
      "concurrent_connections": 10,
      "duration_seconds": 30
    },
    "quiccrunch_results": { /* PerformanceMetrics */ },
    "grpc_results": { /* PerformanceMetrics */ },
    "comparison": {
      "throughput_ratio": 1.25,
      "latency_ratio": 0.80,
      "winner": "quiccrunch"
    }
  }
}
```

### Critical Implementation Notes

1. **tQUIC Integration**: Use the existing mock implementation initially, ensure it's production-ready
2. **Error Handling**: Implement robust error handling and recovery
3. **Thread Safety**: Ensure all concurrent operations are thread-safe
4. **Resource Cleanup**: Proper cleanup of connections and resources
5. **Deterministic Testing**: Ensure repeatable results with proper seeding

### Success Criteria
- All 4 streaming patterns implemented for both protocols
- Accurate performance measurements with statistical validity
- Comprehensive test matrix covering all message sizes and connection counts
- Clean, maintainable code with proper documentation
- Zero data races or memory leaks under concurrent load

## Status: READY FOR IMPLEMENTATION

Pass this context to the Implementation Agent to begin development.