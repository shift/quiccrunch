# QuicCrunch Load Testing Framework

This directory contains a comprehensive load testing framework for QuicCrunch, addressing the critical QA gaps identified in the SDLC gap analysis.

## Overview

The load testing framework provides:

- **Comprehensive Load Testing**: Modern load testing with configurable scenarios
- **End-to-End Testing**: Full RPC communication flow validation
- **Performance Regression Testing**: Automated baseline monitoring and alerting
- **Stress Testing**: Different workload patterns and edge cases
- **Integration Testing**: Multi-service RPC communication validation
- **Performance Benchmarking**: Automated performance monitoring with visualization

## Architecture

```
tests/load-testing/
├── src/
│   ├── bin/                    # Test executables
│   │   ├── load_test.rs       # Main load testing tool
│   │   ├── stress_test.rs     # Stress testing scenarios
│   │   ├── e2e_test.rs        # End-to-end testing
│   │   ├── regression_test.rs # Performance regression detection
│   │   └── dashboard.rs       # Performance monitoring dashboard
│   ├── framework/             # Core testing framework
│   ├── scenarios/             # Test scenarios and workloads
│   ├── metrics/               # Performance metrics and monitoring
│   ├── generators/            # Test data generation
│   └── reporting/             # Test result reporting and visualization
├── config/                    # Test configurations
├── scenarios/                 # Predefined test scenarios
├── baselines/                 # Performance baselines
└── reports/                   # Generated test reports
```

## Key Features

### 1. Load Testing Framework
- **Scalable Architecture**: Support for thousands of concurrent connections
- **Realistic Workloads**: RPC patterns based on real-world usage
- **Multiple Test Types**: Latency, throughput, streaming, and mixed workloads
- **Backend Agnostic**: Tests both Quinn and tQUIC backends

### 2. End-to-End Testing
- **Full RPC Lifecycle**: Connection establishment to graceful shutdown
- **Multi-Service Communication**: Tests complex RPC interaction patterns
- **Error Scenario Testing**: Network failures, timeouts, and edge cases
- **Streaming RPC Validation**: Bidirectional and unidirectional streams

### 3. Performance Regression Testing
- **Baseline Management**: Automated baseline capture and storage
- **Trend Analysis**: Statistical analysis of performance trends
- **Automated Alerting**: Configurable thresholds and notifications
- **Historical Tracking**: Long-term performance monitoring

### 4. Stress Testing
- **Resource Exhaustion**: Memory, CPU, and connection limits
- **Network Conditions**: Packet loss, latency, and bandwidth constraints
- **Burst Traffic**: Sudden load spikes and traffic patterns
- **Long-Running Tests**: Stability and memory leak detection

## Quick Start

### Prerequisites

Ensure you have the Rust toolchain and required dependencies:

```bash
# Install Rust and Cargo (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Enter the Nix development shell (for QuicCrunch dependencies)
nix develop
```

### Building the Framework

```bash
cd tests/load-testing
cargo build --release
```

### Running Load Tests

```bash
# Basic load test
cargo run --bin load-test -- --scenario simple --duration 60s

# Comprehensive test suite
cargo run --bin load-test -- --scenario all --backend quinn --duration 300s

# Stress test with high concurrency
cargo run --bin stress-test -- --scenario burst --connections 5000 --duration 120s

# End-to-end integration test
cargo run --bin e2e-test -- --scenario full-lifecycle --services 3

# Performance regression test
cargo run --bin regression-test -- --baseline baseline-v1.0.0 --threshold 5%
```

### Viewing Results

```bash
# Start the performance dashboard
cargo run --bin load-dashboard -- --port 8080

# Generate static reports
cargo run --bin load-test -- --scenario all --report-format html --output reports/
```

## Test Scenarios

### 1. Simple Load Test
- **Purpose**: Basic functionality validation under load
- **Pattern**: Steady request rate with small message sizes
- **Metrics**: Latency percentiles, throughput, error rate

### 2. Realistic Workload
- **Purpose**: Production-like traffic patterns
- **Pattern**: Variable request rates, mixed message sizes
- **Metrics**: End-to-end latency, resource utilization

### 3. Streaming RPC Test
- **Purpose**: Validate streaming RPC performance
- **Pattern**: Long-lived streams with continuous data flow
- **Metrics**: Stream throughput, connection lifecycle

### 4. Burst Traffic Test
- **Purpose**: Handle sudden traffic spikes
- **Pattern**: Periodic high-intensity bursts
- **Metrics**: Recovery time, resource spikes, error handling

### 5. Resource Exhaustion Test
- **Purpose**: System behavior under resource constraints
- **Pattern**: Gradually increasing load until limits
- **Metrics**: Breaking points, graceful degradation

## Configuration

Test scenarios are configured via TOML files in the `config/` directory:

```toml
[scenario.simple]
name = "Simple Load Test"
duration = "60s"
connections = 100
request_rate = 1000
message_size = 1024

[scenario.realistic]
name = "Realistic Workload"
duration = "300s"
connections = 500
request_rate_pattern = "variable"
message_size_distribution = "realistic"

[performance.thresholds]
max_latency_p99 = "100ms"
min_throughput = "10000"
max_error_rate = "0.1%"

[regression.baselines]
storage = "file"
path = "baselines/"
retention = "30d"
```

## Metrics and Monitoring

The framework collects comprehensive metrics:

### Performance Metrics
- **Latency**: Min, max, mean, percentiles (50th, 95th, 99th, 99.9th)
- **Throughput**: Requests/second, bytes/second
- **Error Rates**: Connection errors, timeout errors, protocol errors
- **Resource Usage**: CPU, memory, file descriptors, network sockets

### RPC-Specific Metrics
- **Connection Lifecycle**: Handshake time, connection duration
- **Stream Metrics**: Stream creation rate, data transfer rates
- **Protocol Metrics**: QUIC packet loss, retransmissions
- **Backend Comparison**: Quinn vs tQUIC performance

### System Metrics
- **Network**: Bandwidth utilization, packet loss, RTT
- **CPU**: Utilization per core, context switches
- **Memory**: Heap usage, allocations, garbage collection
- **I/O**: Disk and network I/O patterns

## Regression Testing

### Baseline Management
- **Automatic Capture**: Baselines captured after successful test runs
- **Version Tagging**: Baselines linked to git commits and versions
- **Retention Policy**: Configurable baseline retention and cleanup

### Trend Analysis
- **Statistical Tests**: Uses t-tests and Mann-Whitney U tests for significance
- **Outlier Detection**: Identifies and flags anomalous results
- **Confidence Intervals**: Statistical confidence in performance changes

### Alerting
- **Threshold-Based**: Configurable thresholds for performance degradation
- **Trend-Based**: Alerts on negative performance trends
- **Integration**: Supports webhook notifications and CI/CD integration

## Integration with CI/CD

The framework integrates with continuous integration:

```yaml
# Example GitHub Actions integration
name: Load Testing
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  load-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
      - name: Run Load Tests
        run: |
          cd tests/load-testing
          cargo run --bin load-test -- --scenario ci --duration 120s
      - name: Performance Regression Check
        run: |
          cargo run --bin regression-test -- --baseline main --threshold 10%
```

## Troubleshooting

### Common Issues

1. **High Memory Usage**: Reduce concurrent connections or enable connection pooling
2. **Connection Timeouts**: Check network configuration and server capacity
3. **Inconsistent Results**: Ensure stable test environment and sufficient warmup

### Debug Mode

Enable detailed logging for troubleshooting:

```bash
RUST_LOG=debug cargo run --bin load-test -- --scenario simple --verbose
```

### Performance Profiling

Generate performance profiles during testing:

```bash
cargo run --bin load-test -- --scenario simple --profile --profile-output profiles/
```

## Contributing

See the main QuicCrunch project for contribution guidelines. When adding new test scenarios:

1. Add scenario configuration to `config/scenarios/`
2. Implement scenario logic in `src/scenarios/`
3. Add documentation and examples
4. Include performance baselines

## License

Same as the main QuicCrunch project.