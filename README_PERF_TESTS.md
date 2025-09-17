# QuicCrunch Network Performance Testing Suite

A comprehensive network performance testing system for QuicCrunch that tests all streaming variants, scalability, and provides detailed performance metrics.

## 🎯 Features

### Core Testing Capabilities
- **Unary RPC Testing**: Simple request/response patterns
- **Client Streaming**: Client sends stream of messages, server responds once
- **Server Streaming**: Client requests once, server streams responses
- **Bidirectional Streaming**: Both client and server stream messages
- **Echo Testing**: Basic latency and correctness testing
- **Throughput Testing**: Sustained load testing

### Performance Metrics
- **Latency**: Min, max, average, and percentile measurements
- **Throughput**: Requests per second and megabits per second
- **Scalability**: Connection and concurrency scaling analysis
- **Resource Usage**: Memory and CPU monitoring
- **Error Rates**: Failed requests and error analysis

### Test Categories
- **Streaming Variants**: Tests all RPC streaming patterns
- **Scalability Tests**: Tests with different connection/concurrency levels
- **Payload Size Tests**: Tests with various message sizes (64B to 256KB)
- **Endurance Tests**: Long-running sustained load tests

## 🏗️ Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│                 │    │                  │    │                 │
│ Performance     │◄──►│   QuicCrunch     │◄──►│ Performance     │
│ Test Client     │    │   QUIC Transport │    │ Test Server     │
│                 │    │   + RPC Layer    │    │                 │
│ - Test Runner   │    │                  │    │ - RPC Services  │
│ - Metrics       │    │ - Stream Mux     │    │ - Metrics       │
│ - Reporting     │    │ - Flow Control   │    │ - Handlers      │
│                 │    │ - Error Recovery │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

## 🚀 Quick Start

### Prerequisites

1. **Nix Environment** (recommended):
   ```bash
   nix develop
   ```

2. **Manual Setup**:
   - Rust 1.75+ with Cargo
   - Required system libraries for QUIC

### Build and Validate

1. **Build the test binaries**:
   ```bash
   ./validate_perf_tests.sh
   ```

2. **Run basic validation**:
   ```bash
   # This will build, start server, run basic tests, and validate setup
   ./validate_perf_tests.sh
   ```

### Run Performance Tests

1. **Streaming variants test** (default):
   ```bash
   ./run_performance_tests.sh
   ```

2. **All test categories**:
   ```bash
   ./run_performance_tests.sh all
   ```

3. **Specific test category**:
   ```bash
   ./run_performance_tests.sh scalability
   ./run_performance_tests.sh payload
   ```

4. **Custom test**:
   ```bash
   ./run_performance_tests.sh custom \
     --test-type bidirectional \
     --payload 8192 \
     --connections 10 \
     --concurrency 4
   ```

## 📊 Test Types

### 1. Unary Testing
Simple request/response pattern testing:
```bash
# Test unary RPC performance
./run_performance_tests.sh custom --test-type unary --messages 10000
```

### 2. Client Streaming Testing
Client sends multiple messages, server responds once:
```bash
# Test client streaming performance
./run_performance_tests.sh custom --test-type client-streaming --messages 5000
```

### 3. Server Streaming Testing
Server sends multiple responses to single client request:
```bash
# Test server streaming performance
./run_performance_tests.sh custom --test-type server-streaming --messages 5000
```

### 4. Bidirectional Streaming Testing
Both client and server stream messages:
```bash
# Test bidirectional streaming performance
./run_performance_tests.sh custom --test-type bidirectional --messages 3000
```

### 5. Echo Testing
Basic latency and correctness testing:
```bash
# Test basic echo performance
./run_performance_tests.sh custom --test-type echo --messages 20000 --payload 1024
```

## 🔧 Configuration

### Test Configuration
Edit `perf_test_config.yaml` to customize:
- Test parameters (message counts, payload sizes)
- Connection settings (timeouts, concurrency)
- Performance targets
- Reporting options

### Server Configuration
```bash
# Start server with custom settings
./target/debug/perf_test_server \
  --bind "0.0.0.0:50051" \
  --max-message-size 2097152 \
  --verbose
```

### Client Configuration
```bash
# Run client with custom settings
./target/debug/perf_test_client \
  --server "192.168.1.100:50051" \
  --test-type all \
  --messages 50000 \
  --payload-size 4096 \
  --connections 5 \
  --concurrency 8 \
  --duration 120 \
  --rate-limit 1000 \
  --output results.json
```

## 📈 Performance Metrics

### Latency Metrics
- **Average Latency**: Mean response time
- **Min/Max Latency**: Best and worst case response times  
- **Percentiles**: P50, P90, P95, P99 latency measurements

### Throughput Metrics
- **Requests/Second**: Request rate sustained
- **Megabits/Second**: Data transfer rate
- **Messages/Second**: Message processing rate

### Scalability Metrics
- **Connection Scaling**: Performance vs number of connections
- **Concurrency Scaling**: Performance vs concurrent operations
- **Linear Scaling Efficiency**: How well performance scales

### Resource Metrics
- **Memory Usage**: Peak and average memory consumption
- **CPU Usage**: Processing overhead
- **Network Utilization**: Bandwidth efficiency

## 📋 Example Results

```json
{
  "duration_seconds": 60.0,
  "requests_sent": 100000,
  "responses_received": 100000,
  "errors": 0,
  "requests_per_second": 1666.7,
  "bytes_sent": 409600000,
  "bytes_received": 409600000,
  "throughput_mbps_tx": 54.6,
  "throughput_mbps_rx": 54.6,
  "avg_latency_ms": 2.4,
  "min_latency_ms": 0.8,
  "max_latency_ms": 12.1,
  "connections_used": 5
}
```

## 🔍 Troubleshooting

### Common Issues

1. **Server fails to start**:
   ```bash
   # Check if port is in use
   netstat -ln | grep 50051
   
   # Use different port
   ./target/debug/perf_test_server --bind "127.0.0.1:50052"
   ```

2. **Client connection fails**:
   ```bash
   # Test basic connectivity
   nc -z 127.0.0.1 50051
   
   # Check server logs
   tail -f perf_test_results/server.log
   ```

3. **Performance lower than expected**:
   - Check system resources (CPU, memory)
   - Verify network configuration
   - Increase payload size for higher throughput
   - Adjust connection/concurrency parameters

4. **Build errors**:
   ```bash
   # Ensure in Nix shell
   nix develop
   
   # Clean build
   cargo clean && cargo build
   ```

### Debug Mode

Enable verbose logging:
```bash
./run_performance_tests.sh --verbose
```

Check detailed logs:
```bash
tail -f perf_test_results/server.log
tail -f perf_test_results/client.log
```

## 🎛️ Advanced Usage

### Custom Test Scenarios

1. **High Concurrency Test**:
   ```bash
   ./run_performance_tests.sh custom \
     --test-type echo \
     --connections 20 \
     --concurrency 16 \
     --messages 100000
   ```

2. **Large Message Test**:
   ```bash
   ./run_performance_tests.sh custom \
     --test-type unary \
     --payload 262144 \
     --messages 1000
   ```

3. **Endurance Test**:
   ```bash
   ./run_performance_tests.sh custom \
     --test-type throughput \
     --duration 1800 \
     --rate-limit 500
   ```

### Automated Testing

Run tests in CI/CD:
```bash
# Non-interactive mode
./run_performance_tests.sh all --no-server 2>&1 | tee test_results.log
```

### Performance Regression Testing

Compare results over time:
```bash
# Baseline test
./run_performance_tests.sh > baseline_results.json

# After changes
./run_performance_tests.sh > current_results.json

# Compare (requires custom analysis script)
diff baseline_results.json current_results.json
```

## 📁 File Structure

```
quiccrunch/
├── quiccrunch-core/src/bin/
│   ├── perf_test_server.rs      # Performance test server
│   └── perf_test_client.rs      # Performance test client
├── run_performance_tests.sh     # Main test runner script
├── validate_perf_tests.sh       # Validation and setup script
├── perf_test_config.yaml        # Test configuration
├── perf_test_results/           # Test results (created during runs)
│   ├── server.log
│   ├── client.log
│   ├── *.json                   # Individual test results
│   └── performance_report.md    # Generated report
└── README_PERF_TESTS.md         # This file
```

## 🤝 Contributing

To add new test types or metrics:

1. **Server**: Add handlers in `perf_test_server.rs`
2. **Client**: Add test methods in `perf_test_client.rs`
3. **Scripts**: Update `run_performance_tests.sh`
4. **Config**: Add parameters to `perf_test_config.yaml`

## 📄 License

Same as QuicCrunch project (Apache 2.0 OR MIT)