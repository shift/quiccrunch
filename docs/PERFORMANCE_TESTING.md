# QUIC + Cap'n Proto Performance Test Implementation

This document provides complete instructions for running the performance test specification for comparing Quinn and tQUIC backends with Cap'n Proto serialization.

## 📋 Test Specification Overview

This implementation provides a comprehensive test suite that evaluates:

1. **RPC Latency** - Round-trip time measurement (Test Case 1)
2. **Message Throughput** - Concurrent request handling capacity (Test Case 2)  
3. **Streaming Bandwidth** - Large data transfer performance (Test Case 3)
4. **Resource Utilization** - CPU and memory profiling analysis
5. **Backend Comparison** - Side-by-side Quinn vs tQUIC analysis

## 🚀 Quick Start

### Prerequisites

1. **NixOS Environment** (required)
   ```bash
   # Enter the development shell
   nix develop
   ```

2. **Build Both Backends**
   ```bash
   # Build Quinn backend
   cargo build --release --bin benchmark --features backend-quinn
   
   # Build tQUIC backend  
   cargo build --release --bin benchmark --features backend-tquic
   ```

### Run Complete Specification Tests

Execute all test cases for both backends with full reporting:

```bash
# Run complete specification test suite
nix develop -c ./scripts/run_specification_tests.sh
```

This will:
- Run all 3 test cases for both Quinn and tQUIC backends
- Generate comprehensive performance reports
- Create JSON results for detailed analysis
- Provide backend comparison recommendations

## 📊 Test Execution Options

### 1. Specification-Compliant Testing

Execute the complete test specification:

```bash
# All backends, all test cases
nix develop -c ./scripts/run_specification_tests.sh

# Single backend
BACKEND=quinn nix develop -c ./scripts/run_specification_tests.sh

# With profiling analysis
RUN_PROFILING=true nix develop -c ./scripts/run_specification_tests.sh
```

### 2. Backend Comparison Analysis

Run side-by-side comparison with detailed analysis:

```bash
# Standard comparison
nix develop -c ./scripts/compare_backends.sh

# Extended comparison with profiling
PROFILING=true TEST_DURATION=30 nix develop -c ./scripts/compare_backends.sh
```

### 3. Individual Test Cases

Run specific test cases manually:

```bash
# Start server (in one terminal)
nix develop -c ./target/release/benchmark server --verbose

# Run specific tests (in another terminal)
# Test Case 1: Latency
nix develop -c ./target/release/benchmark client latency --iterations 10000

# Test Case 2: Throughput  
nix develop -c ./target/release/benchmark client flood --duration 10 --concurrency 100

# Test Case 3: Bandwidth
nix develop -c ./target/release/benchmark client stream --size-gib 1.0
```

### 4. Profiling Analysis

Generate CPU and memory profiles as specified:

```bash
# CPU profiling with perf + flamegraph
PROFILING_TOOL=perf nix develop -c ./scripts/profile_benchmark.sh

# Memory profiling with heaptrack
PROFILING_TOOL=heaptrack nix develop -c ./scripts/profile_benchmark.sh

# Both profiling tools
PROFILING_TOOL=both nix develop -c ./scripts/profile_benchmark.sh
```

## 📈 Results and Reporting

### Output Directory Structure

```
test_results/run_YYYYMMDD_HHMMSS/
├── quinn/                          # Quinn backend results
│   ├── test_case_1_latency.json   # Latency test results
│   ├── test_case_2_flood.json     # Throughput test results
│   ├── test_case_3_stream.json    # Bandwidth test results
│   ├── server.log                 # Server logs
│   └── profiling/                 # Profiling data (if enabled)
├── tquic/                          # tQUIC backend results
│   ├── test_case_1_latency.json   # (same structure as Quinn)
│   └── ...
└── reports/
    ├── specification_report.md    # Main compliance report
    └── visualization_report.txt   # Performance analysis
```

### Key Reports

1. **Specification Report** (`reports/specification_report.md`)
   - Complete test specification compliance
   - Performance metrics for all test cases
   - Backend comparison analysis
   - Recommendations based on results

2. **Visualization Report** (generated on demand)
   ```bash
   python scripts/visualize_results.py test_results/run_YYYYMMDD_HHMMSS/
   ```

## 🔧 Configuration

### Environment Variables

All scripts support these environment variables:

```bash
# Backend selection
BACKEND=quinn          # quinn, tquic, or both (default: both)

# Test configuration  
ADDRESS=127.0.0.1:50055    # Server address (default)
TEST_DURATION=10           # Test duration in seconds (default: 10)
OUTPUT_DIR=./test_results  # Results directory (default)

# Profiling options
RUN_PROFILING=true         # Enable profiling (default: false)
PROFILING_TOOL=perf        # perf, heaptrack, or both (default: perf)
```

### Standardized Configuration

The implementation ensures fair comparison by using identical settings for both backends:

- **Timeout Settings**: 30s idle timeout, 5s keep-alive (per specification)
- **Flow Control**: Identical window sizes for each test case
- **Congestion Control**: NewReno (available in both backends)
- **Feature Flags**: Multipath and HyStart disabled for consistency

## 📋 Test Case Details

### Test Case 1: RPC Ping-Pong (Latency)
- **Objective**: Measure lowest possible round-trip time
- **Method**: Single connection, 10,000 iterations, 64-byte messages
- **Metrics**: Mean, median, P90, P95, P99, max latency in microseconds

### Test Case 2: RPC Flood (Message Throughput)  
- **Objective**: Measure server capacity for concurrent requests
- **Method**: 100 concurrent tasks, 10-second duration, maximum message rate
- **Metrics**: Total messages, messages/sec, throughput Mbps, latency distribution

### Test Case 3: Bulk Data Streaming (Bandwidth)
- **Objective**: Measure maximum sustainable data transfer rate
- **Method**: Single connection, 1 GiB transfer, unidirectional stream
- **Metrics**: Total time, throughput Gbps, chunk transfer rate

## 🔬 Profiling and Analysis

### CPU Profiling (perf + flamegraph)

1. **Record Performance Data**
   ```bash
   perf record --call-graph dwarf ./target/release/benchmark client flood
   ```

2. **Generate Reports**
   ```bash
   perf report --input perf.data
   flamegraph perf.data > flamegraph.svg
   ```

### Memory Profiling (heaptrack)

1. **Record Allocations**
   ```bash
   heaptrack ./target/release/benchmark client flood
   ```

2. **Analyze Results**
   ```bash
   heaptrack_print heaptrack.data.gz
   heaptrack_gui heaptrack.data.gz  # Interactive analysis
   ```

## 📊 Performance Analysis

### Success Criteria (per specification)

The tests focus on **characterization** rather than pass/fail:

1. **Performance Differences**: Statistical significance between backends
2. **Resource Usage**: CPU and memory consumption patterns  
3. **Use Case Suitability**: Optimal backend for specific workloads

### Interpretation Guidelines

- **Latency-Sensitive Apps**: Choose backend with lower P99 latency
- **High-Throughput Apps**: Choose backend with higher messages/sec
- **Bandwidth-Intensive Apps**: Choose backend with higher Gbps throughput
- **Resource-Constrained**: Choose backend with lower CPU/memory usage

## 🚨 Troubleshooting

### Common Issues

1. **Build Failures**
   ```bash
   # Ensure you're in nix develop
   nix develop
   
   # Clean and rebuild
   cargo clean
   cargo build --release --bin benchmark --features backend-quinn
   ```

2. **Server Won't Start**
   ```bash
   # Check port availability
   netstat -ln | grep 50055
   
   # Use different port
   ADDRESS=127.0.0.1:50056 nix develop -c ./scripts/run_specification_tests.sh
   ```

3. **Profiling Tools Missing**
   ```bash
   # Install perf
   sudo apt install linux-perf
   
   # Install flamegraph
   cargo install flamegraph
   
   # Install heaptrack  
   sudo apt install heaptrack
   ```

4. **Permission Issues**
   ```bash
   # Make scripts executable
   chmod +x scripts/*.sh scripts/*.py
   
   # Run perf with appropriate permissions
   sudo sysctl kernel.perf_event_paranoid=1
   ```

## 📚 Advanced Usage

### Custom Test Configurations

1. **Modify Test Parameters**
   ```bash
   # Longer latency test
   ./target/release/benchmark client latency --iterations 50000
   
   # Higher concurrency throughput test
   ./target/release/benchmark client flood --concurrency 500 --duration 30
   
   # Larger streaming test
   ./target/release/benchmark client stream --size-gib 5.0
   ```

2. **Backend-Specific Settings**
   Edit `src/config.rs` to modify `StandardizedBenchmarkConfig` for custom scenarios.

### Automated CI/CD Integration

```bash
# Headless testing for CI systems
BACKEND=both RUN_PROFILING=false nix develop -c ./scripts/run_specification_tests.sh

# Generate machine-readable results
python scripts/visualize_results.py results/ --output results.json
```

## 📈 Results Interpretation

### Sample Output Analysis

```json
{
  "test_case": "latency",
  "backend": "quinn", 
  "metrics": {
    "mean_us": 245.7,
    "p99_us": 450.2,
    "iterations": 10000
  }
}
```

**Analysis**: Quinn achieves ~246μs mean latency with P99 of 450μs for small messages on loopback.

### Performance Comparison

Use the visualization tools to compare:
```bash
python scripts/visualize_results.py test_results/run_YYYYMMDD_HHMMSS/ --print
```

This generates comparison tables, recommendations, and performance analysis.

## 🎯 Next Steps

1. **Production Testing**: Run on production-equivalent hardware
2. **Network Conditions**: Test with realistic latency and packet loss
3. **Load Patterns**: Use application-specific message patterns
4. **Stability Testing**: Extended duration tests for reliability assessment
5. **Feature Testing**: Compare backend-specific features (multipath, etc.)

---

**For questions or issues**: Please check the project documentation or create an issue in the repository.