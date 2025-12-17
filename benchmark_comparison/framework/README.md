# Benchmark Framework

A comprehensive benchmarking framework for comparing gRPC and QUICCRUNCH implementations with detailed metrics collection, analysis, and reporting capabilities.

## Features

### Test Orchestration
- Automatic server/client lifecycle management
- Multiple test scenarios with configurable parameters
- Parallel test execution with resource limits
- Warmup periods and cleanup between tests
- Health checks and graceful shutdown

### Metrics Collection
- **Latency Metrics**: Min, Max, Mean, P50, P95, P99, P99.9 percentiles
- **Throughput Metrics**: Messages/sec, Bytes/sec, Mbps
- **Resource Usage**: CPU, Memory, Network, Disk utilization
- **Error Analysis**: Connection errors, timeouts, protocol errors
- **Connection Metrics**: Establishment time, success rates

### Data Storage
- Structured JSON/CSV/YAML output formats
- Metadata preservation for reproducible tests
- Historical data management
- Compression support for large datasets

### Analysis Tools
- Statistical significance testing
- Performance trend analysis
- Implementation comparisons
- Scenario-specific recommendations
- Regression detection

### Reporting
- Interactive HTML reports with charts
- Performance comparison visualizations
- Export to multiple formats
- Executive summaries and detailed analysis

## Quick Start

### Installation

```bash
cd benchmark_comparison/framework
cargo build --release
```

### Basic Usage

1. **Initialize Configuration**
```bash
./target/release/benchmark_runner init --config benchmark_config.yaml
```

2. **Validate Environment**
```bash
./target/release/benchmark_runner validate --system --dependencies
```

3. **Run Benchmarks**
```bash
./target/release/benchmark_runner run --config benchmark_config.yaml
```

4. **Generate Reports**
```bash
./target/release/benchmark_runner report --input ./results --output ./reports
```

## Configuration

The framework uses YAML configuration files to define test parameters:

```yaml
test_name: "performance_comparison"
description: "Comprehensive performance comparison"

implementations:
  - name: "gRPC"
    implementation_type: "GRPC"
    server_command: "cargo run --bin server"
    client_command: "cargo run --bin client"
    working_directory: "../grpc"
    startup_timeout_secs: 30
    shutdown_timeout_secs: 10
    health_check_url: "http://localhost:50051"

  - name: "QUICCRUNCH"
    implementation_type: "QUICCRUNCH"
    server_command: "cargo run --bin server"
    client_command: "cargo run --bin client"
    working_directory: "../quiccrunch"
    startup_timeout_secs: 30
    shutdown_timeout_secs: 10
    health_check_url: "http://localhost:8080"

test_scenarios:
  - name: "light_load"
    message_sizes: [64, 256, 1024]
    concurrency_levels: [1, 5, 10]
    duration_secs: 30
    warmup_duration_secs: 5
    connections_per_client: 1
    messages_per_connection: 1000
    payload_pattern: "Random"

  - name: "medium_load"
    message_sizes: [1024, 4096, 16384]
    concurrency_levels: [10, 50, 100]
    duration_secs: 60
    warmup_duration_secs: 10
    connections_per_client: 5
    rate_limit: 10000
    payload_pattern: "Random"

  - name: "heavy_load"
    message_sizes: [4096, 65536, 1048576]
    concurrency_levels: [100, 500, 1000]
    duration_secs: 120
    warmup_duration_secs: 20
    connections_per_client: 10
    rate_limit: 50000
    payload_pattern: "Random"

global_settings:
  parallel_tests: 2
  max_retries: 3
  cleanup_between_tests: true
  system_monitoring_interval_ms: 1000
  resource_limits:
    max_cpu_percent: 90.0
    max_memory_mb: 8192
    max_disk_usage_percent: 80.0

output_settings:
  results_directory: "./results"
  formats: ["Json", "Csv", "Yaml"]
  include_raw_data: true
  generate_charts: true
  chart_types: ["LatencyComparison", "ThroughputComparison", "ResourceUsage"]
  compression: false
```

## Command Line Interface

### Commands

- `run` - Execute benchmark suite
- `report` - Generate reports from existing results
- `validate` - Check system requirements and dependencies
- `cleanup` - Remove test artifacts and processes
- `init` - Initialize configuration file

### Options

- `--config <path>` - Configuration file path
- `--output-dir <path>` - Output directory for results
- `--log-level <level>` - Logging level (trace, debug, info, warn, error)
- `--implementations <list>` - Override implementations (comma-separated)
- `--scenarios <list>` - Override scenarios (comma-separated)
- `--parallel <num>` - Number of parallel tests

## Test Scenarios

### Predefined Scenarios

1. **Light Load**: Basic functionality testing
   - Message sizes: 64B - 1KB
   - Concurrency: 1-10 connections
   - Duration: 30 seconds

2. **Medium Load**: Typical usage patterns
   - Message sizes: 1KB - 16KB
   - Concurrency: 10-100 connections
   - Duration: 60 seconds

3. **Heavy Load**: Performance limits
   - Message sizes: 4KB - 1MB
   - Concurrency: 100-1000 connections
   - Duration: 120 seconds

### Custom Scenarios

Create custom scenarios by defining:

- Message size ranges
- Concurrency levels
- Test duration and warmup periods
- Rate limiting
- Payload patterns (Random, Sequential, Zeroes, Repeating)
- Network conditions (bandwidth, latency, packet loss)

## Metrics and Analysis

### Collected Metrics

#### Performance Metrics
- Latency distribution (min, max, mean, percentiles)
- Throughput (messages/sec, bytes/sec, Mbps)
- Connection establishment time
- Success and error rates

#### System Metrics
- CPU usage percentage
- Memory consumption
- Network I/O
- Disk usage
- Process count and load average

#### Error Analysis
- Connection failures
- Timeout errors
- Protocol errors
- Network errors
- Error rate trends

### Statistical Analysis

- **Significance Testing**: T-tests and Mann-Whitney U tests
- **Trend Analysis**: Linear regression and time series analysis
- **Percentile Analysis**: P50, P95, P99, P99.9 latency percentiles
- **Regression Detection**: Automated performance regression identification

### Performance Comparisons

The framework provides detailed comparisons between implementations:

- Latency improvements (percentage)
- Throughput gains
- Resource efficiency
- Reliability metrics
- Overall performance scores

## Reports

### HTML Reports

Interactive HTML reports include:

- Executive summary
- Performance charts and graphs
- Detailed comparison tables
- Recommendations
- Historical trends

### Chart Types

- **Latency Comparison**: Bar charts comparing mean latency
- **Throughput Comparison**: Throughput by implementation
- **Resource Usage**: CPU and memory consumption
- **Error Rates**: Error rate comparisons
- **Percentile Distribution**: Latency percentile curves

### Export Formats

- **JSON**: Structured data for programmatic access
- **CSV**: Tabular data for spreadsheet analysis
- **YAML**: Human-readable configuration format
- **HTML**: Interactive reports with visualizations

## Integration

### gRPC Integration

The framework automatically starts gRPC servers and runs benchmark clients:

```bash
# gRPC server should implement health check at /health
# gRPC client should accept environment variables:
# BENCHMARK_MESSAGE_SIZE, BENCHMARK_CONCURRENCY, BENCHMARK_DURATION
# BENCHMARK_WARMUP, BENCHMARK_CONNECTIONS, BENCHMARK_RATE_LIMIT
```

### QUICCRUNCH Integration

Similar integration pattern for QUICCRUNCH implementations:

```bash
# QUICCRUNCH server health check
# QUICCRUNCH client environment variables
```

### Custom Implementations

Add new implementations by:

1. Defining implementation configuration
2. Implementing health check endpoint
3. Supporting standard environment variables
4. Following output format conventions

## Best Practices

### Test Environment

- Use dedicated hardware for consistent results
- Disable unnecessary services and background processes
- Ensure network isolation and stable conditions
- Monitor system resources during tests

### Configuration

- Start with default configurations and customize gradually
- Use appropriate warmup periods for stable measurements
- Configure realistic test scenarios based on expected usage
- Set reasonable resource limits to prevent system overload

### Analysis

- Run multiple test iterations for statistical significance
- Compare results across different scenarios
- Monitor trends over time for regression detection
- Consider both average and percentile performance

### Troubleshooting

Common issues and solutions:

1. **Port conflicts**: Use automatic port allocation
2. **Resource exhaustion**: Adjust parallel test limits
3. **Timeout errors**: Increase timeout values
4. **Inconsistent results**: Ensure stable test environment

## Examples

### Quick Performance Test

```bash
# Run quick comparison with default settings
./target/release/benchmark_runner run --parallel 1

# Generate HTML report
./target/release/benchmark_runner report --format html
```

### Custom Scenario

```bash
# Run specific scenario with custom parameters
./target/release/benchmark_runner run \
  --implementations "gRPC,QUICCRUNCH" \
  --scenarios "heavy_load" \
  --parallel 4
```

### Environment Validation

```bash
# Check system requirements
./target/release/benchmark_runner validate --system --dependencies

# Clean up before running tests
./target/release/benchmark_runner cleanup --processes --temp
```

## Architecture

The framework consists of several key components:

- **Orchestrator**: Test execution and lifecycle management
- **Storage**: Result persistence and retrieval
- **Analysis**: Statistical analysis and comparison
- **Reporting**: Chart generation and report creation
- **Utils**: System utilities and helper functions

Each component is designed to be modular and extensible, allowing for customization and enhancement based on specific requirements.

## Contributing

To extend the framework:

1. Add new metrics collectors in the `metrics` module
2. Implement custom storage backends
3. Create new chart types and visualizations
4. Add support for additional protocols
5. Enhance statistical analysis capabilities

## License

This benchmark framework is part of the QUICCRUNCH project and follows the same licensing terms.