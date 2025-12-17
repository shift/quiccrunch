# gRPC Benchmark for QUICCRUNCH Comparison

This directory contains a complete gRPC baseline implementation for performance comparison with QUICCRUNCH. It implements all four gRPC request models with realistic benchmark workloads and comprehensive metrics collection.

## Features

### gRPC Services Implemented
- **Unary RPC**: Simple request-response pattern
- **Client Streaming**: Client sends stream, server responds once
- **Server Streaming**: Client sends one request, server responds with stream
- **Bidirectional Streaming**: Full duplex communication

### Benchmark Capabilities
- Configurable message sizes and counts
- Multiple payload patterns (random, sequential, zeroes, repeating)
- Concurrent connection testing
- Comprehensive metrics collection (latency, throughput, errors)
- Prometheus metrics integration
- JSON result export

## Quick Start

### 1. Build the Project

```bash
cd benchmark_comparison/grpc
cargo build --release
```

### 2. Start the Server

```bash
# Basic server
cargo run --bin grpc-server

# Custom address
cargo run --bin grpc-server -- --address 0.0.0.0:50051

# With custom metrics port
cargo run --bin grpc-server -- --address 0.0.0.0:50051 --metrics-port 9090
```

### 3. Run Client Tests

```bash
# Run all test types
cargo run --bin grpc-client -- --server http://127.0.0.1:50051 --test-type all

# Specific test type
cargo run --bin grpc-client -- --server http://127.0.0.1:50051 --test-type unary --messages 10000 --message-size 4096

# High concurrency test
cargo run --bin grpc-client -- --server http://127.0.0.1:50051 --test-type bidirectional --connections 10 --messages 50000
```

### 4. Run Comprehensive Benchmark Suite

```bash
# Basic benchmark suite
cargo run --bin benchmark-runner -- --suite basic --output basic_results.json

# Comprehensive suite (all configurations)
cargo run --bin benchmark-runner -- --suite comprehensive --output full_results.json

# Throughput-focused suite
cargo run --bin benchmark-runner -- --suite throughput --output throughput_results.json
```

## Configuration Options

### Client Options
- `--server`: Server address (default: `http://127.0.0.1:50051`)
- `--test-type`: Test type (`unary`, `client-streaming`, `server-streaming`, `bidirectional`, `latency`, `flood`, `all`)
- `--messages`: Number of messages (default: `1000`)
- `--message-size`: Message size in bytes (default: `1024`)
- `--connections`: Concurrent connections (default: `1`)
- `--duration`: Test duration in seconds (default: `30`)
- `--payload-pattern`: Payload pattern (`random`, `sequential`, `zeroes`, `repeating`)

### Server Options
- `--address`: Bind address (default: `127.0.0.1:50051`)
- `--metrics-port`: Prometheus metrics port (default: `9090`)

### Benchmark Runner Options
- `--suite`: Benchmark suite (`basic`, `throughput`, `latency`, `stress`, `comprehensive`)
- `--output`: Results output file (JSON format)
- `--warmup`: Warmup duration in seconds

## Benchmark Suites

### Basic Suite
- Single configuration with 1K messages, 1KB payload
- Good for quick validation

### Throughput Suite
- Multiple message sizes (1KB, 4KB, 16KB, 64KB)
- High message counts (10K messages)
- Optimized for maximum throughput measurement

### Latency Suite
- Small message sizes (64B, 256B, 1KB)
- Zero-filled payloads for consistent latency
- High message counts for statistical significance

### Stress Suite
- Multiple concurrent connections (1, 5, 10, 25)
- Medium message sizes (4KB)
- Longer duration for stability testing

### Comprehensive Suite
- All combinations of message sizes, counts, and connections
- Complete performance matrix
- Most thorough evaluation

## Metrics Collection

### Client-Side Metrics
- **Latency**: Min, Max, Mean, P50, P95, P99, P99.9
- **Throughput**: Messages/sec, Bytes/sec, Mbps
- **Errors**: Connection, Timeout, Protocol, Other
- **Data Transfer**: Total bytes sent/received

### Server-Side Metrics
- **Concurrent Connections**: Active connection count
- **Total Messages**: Cumulative message count
- **CPU Usage**: Estimated CPU utilization
- **Memory Usage**: Estimated memory consumption

### Prometheus Metrics
- `grpc_requests_total`: Total gRPC requests
- `grpc_request_duration_us`: Request latency histogram
- `grpc_client_streaming_total`: Client streaming calls
- `grpc_server_streaming_total`: Server streaming calls
- `grpc_bidirectional_streaming_total`: Bidirectional streaming calls

## Example Usage Scenarios

### 1. Basic Latency Test
```bash
# Start server
cargo run --bin grpc-server &

# Run latency test
cargo run --bin grpc-client -- \
  --test-type latency \
  --messages 10000 \
  --message-size 64 \
  --payload-pattern zeroes
```

### 2. High Throughput Test
```bash
# Start server
cargo run --bin grpc-server &

# Run throughput test
cargo run --bin grpc-client -- \
  --test-type bidirectional \
  --messages 100000 \
  --message-size 65536 \
  --connections 10
```

### 3. Production Simulation
```bash
# Start server
cargo run --bin grpc-server &

# Run comprehensive benchmark
cargo run --bin benchmark-runner -- \
  --suite comprehensive \
  --output production_benchmark.json \
  --warmup 30
```

## Performance Analysis

### Results Format
Results are exported in JSON format with the following structure:
```json
{
  "test_id": "uuid",
  "test_type": "unary",
  "start_time": 1234567890,
  "end_time": 1234567895,
  "total_messages": 1000,
  "successful_messages": 998,
  "failed_messages": 2,
  "bytes_sent": 1024000,
  "bytes_received": 1024000,
  "latency_stats": {
    "min_us": 100,
    "max_us": 5000,
    "mean_us": 500.0,
    "p50_us": 400,
    "p95_us": 1200,
    "p99_us": 2000,
    "p999_us": 3500
  },
  "throughput_stats": {
    "messages_per_second": 1000.0,
    "bytes_per_second": 1024000.0,
    "megabits_per_second": 8.192
  },
  "error_stats": {
    "connection_errors": 0,
    "timeout_errors": 1,
    "protocol_errors": 1,
    "other_errors": 0
  }
}
```

### Comparison with QUICCRUNCH
This gRPC implementation serves as a baseline for comparing with QUICCRUNCH performance. Key comparison metrics:
- **Latency**: Compare P95 and P99 latencies across different message sizes
- **Throughput**: Compare maximum sustainable throughput
- **Scalability**: Compare performance under high concurrency
- **Resource Usage**: Compare CPU and memory efficiency

## Architecture

### Server Implementation
- Uses `tonic` for gRPC framework
- Implements realistic workloads with configurable processing delays
- Includes CPU-bound work simulation
- Provides comprehensive server metrics

### Client Implementation
- Supports all four gRPC request patterns
- Includes detailed timing and throughput measurement
- Implements proper error handling and retry logic
- Supports concurrent connection testing

### Common Utilities
- Shared configuration and metrics collection
- Payload generation with multiple patterns
- Comprehensive error tracking
- Statistical analysis utilities

## Dependencies

- `tonic`: gRPC framework
- `prost`: Protocol buffer implementation
- `tokio`: Async runtime
- `tracing`: Structured logging
- `metrics`: Metrics collection
- `prometheus`: Metrics export
- `serde`: JSON serialization
- `clap`: Command line parsing
- `criterion`: Benchmarking framework

## Contributing

When adding new features or benchmarks:
1. Update the protocol definitions in `proto/benchmark.proto`
2. Regenerate the client/server code: `cargo build`
3. Add corresponding tests
4. Update documentation
5. Ensure all benchmarks pass

## License

Licensed under the Apache License 2.0 or MIT License. See LICENSE-APACHE and LICENSE-MIT for details.