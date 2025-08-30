# Context Files Reference

This document provides quick reference to all relevant files and their purposes for the gRPC vs QuicCrunch performance comparison project.

## Agent Communication Context Files

### Implementation Context (`ai/contexts/benchmark_implementation.md`)
- Task specifications for implementation agent
- Protocol definitions and requirements
- Expected API interfaces and signatures
- Implementation status and progress tracking

### Testing Context (`ai/contexts/benchmark_testing.md`)
- Test requirements and validation criteria
- Implementation feedback from testing agent
- Bug reports and failure analysis
- Test results and performance metrics

### Documentation Context (`ai/contexts/benchmark_documentation.md`)
- Documentation requirements and standards
- Benchmark results and analysis
- Report generation specifications
- Documentation update requirements

## Agent Workflow

```
Implementation Agent → Testing Agent → Documentation Agent
       ↑                    ↓
       └── Feedback Loop ───┘
```

## tQUIC Backend Files

### Core Implementation
- **`quiccrunch-core/src/stream.rs`**: Main stream implementation - currently has placeholder tQUIC integration
- **`quiccrunch-core/src/lib.rs`**: Core library exports and module definitions
- **`quiccrunch-core/src/backend/mod.rs`**: Backend abstraction layer
- **`quiccrunch-core/Cargo.toml`**: Features configuration including `backend-tquic`

### Integration Points
- **`quiccrunch-core/build.rs`**: Build script for protocol buffer generation
- **`quiccrunch-core/src/error.rs`**: Error handling for stream operations
- **`quiccrunch-core/src/config.rs`**: Configuration structures including TlsConfig

## Benchmark Framework Files

### Main Implementation
- **`quiccrunch-core/examples/grpc_comparison.rs`**: Primary benchmark implementation
- **`quiccrunch-core/proto/benchmark.proto`**: gRPC service definitions with streaming support
- **`scripts/comprehensive_comparison.sh`**: Shell script for running benchmarks

### Configuration
- **`quiccrunch-core/benches/`**: Directory containing benchmark configurations
- **`Cargo.toml`**: Root workspace configuration
- **`flake.nix`**: Nix development environment setup

## Benchmark Config Files

### Current Implementation
```rust
// In grpc_comparison.rs
const MESSAGE_SIZES: &[usize] = &[64, 1024, 4096, 16384, 65536, 1048576, 8388608];
```

### Target Implementation (needs expansion)
```rust
const MESSAGE_SIZES: &[usize] = &[
    16, 32, 64, 128, 256, 512, 768,           // Small (B)
    1024, 2048, 4096, 8192,                  // Small (KB)  
    16384, 32768, 65536, 131072,             // Medium (KB)
    1048576, 2097152                         // Large (MB)
];
```

## Streaming Implementation Files

### Protocol Definitions
- **`quiccrunch-core/proto/benchmark.proto`**: Contains all streaming service definitions:
  ```protobuf
  // Unary RPC
  rpc UnaryCall(SimpleRequest) returns (SimpleResponse);
  
  // Client streaming RPC  
  rpc FloodTest(stream SimpleRequest) returns (SimpleResponse);
  
  // Server streaming RPC
  rpc StreamTest(SimpleRequest) returns (stream SimpleResponse);
  
  // Bidirectional streaming RPC
  rpc FullDuplexCall(stream SimpleRequest) returns (stream SimpleResponse);
  ```

### Implementation Areas (need work)
- **`grpc_comparison.rs:run_unary_benchmark()`**: ✅ Implemented
- **`grpc_comparison.rs:run_client_streaming_benchmark()`**: ❌ Missing
- **`grpc_comparison.rs:run_server_streaming_benchmark()`**: ❌ Missing  
- **`grpc_comparison.rs:run_bidirectional_benchmark()`**: ❌ Missing

## Report Files

### Current Output
- **`benchmark_results/grpc_comparison_results_20250829_133535.json`**: Detailed JSON metrics
- **`benchmark_results/grpc_comparison_summary_20250829_133535.md`**: Comprehensive analysis
- **`benchmark_results/release_comparison_dev_20250829_133535.md`**: Release format summary

### Performance Baseline Data
- **`performance/2025-08-29-backend-tquic.json`**: Historical tQUIC performance data
- **`performance/2025-08-29-backend-tquic-report.md`**: Previous performance analysis
- **`baseline_performance_results.txt`**: System baseline measurements

### Template Structure
```
benchmark_results/
├── grpc_comparison_results_{timestamp}.json     # Raw metrics
├── grpc_comparison_summary_{timestamp}.md       # Technical analysis  
├── release_comparison_dev_{timestamp}.md        # Executive summary
└── test_matrix_{timestamp}.csv                  # Planned: Full test data
```

## Testing Files

### Integration Tests
- **`tests/integration/`**: Integration test directory
- **`tests/load-testing/`**: Load testing configurations and scripts
- **`test_results/`**: Historical test results and analysis

### Validation Scripts
- **`scripts/validate_fixes.sh`**: Validation script for fixes
- **`scripts/run_benchmark.sh`**: Benchmark execution script
- **`scripts/check.sh`**: General validation and checking

### Performance Analysis
- **`scripts/analyze_results.py`**: Python script for result analysis
- **`performance_investigation/`**: Performance debugging artifacts

## Development Environment

### Nix Configuration
- **`flake.nix`**: Complete development environment with all dependencies
- **`flake.lock`**: Locked dependency versions

### Build Configuration  
- **`.cargo/config.toml`**: Cargo build configuration
- **`Makefile`**: Build automation and common tasks

### Docker Setup
- **`docker/docker-compose.dev.yml`**: Development environment
- **`docker/Dockerfile.server`**: Server container configuration

## Key Data Structures

### BenchmarkConfig (in grpc_comparison.rs)
```rust
struct BenchmarkConfig {
    message_size: usize,
    // Need to add:
    streaming_pattern: StreamingPattern,
    concurrent_connections: usize,
    duration_seconds: u64,
}
```

### Performance Metrics (output format)
```json
{
  "timestamp": "ISO-8601",
  "test_config": {
    "message_size": 1024,
    "streaming_pattern": "unary", 
    "connections": 1
  },
  "quiccrunch_metrics": {
    "latency_p50": 1.23,
    "latency_p95": 2.45,
    "throughput_msg_per_sec": 10000,
    "memory_usage_mb": 45.2
  },
  "grpc_metrics": { /* same structure */ }
}
```

## Dependencies and Features

### Cargo Features
```toml
# In quiccrunch-core/Cargo.toml
[features]
default = []
backend-tquic = ["tquic"]
grpc-comparison = ["tonic", "prost"]
```

### Key Dependencies
- **tonic**: gRPC framework for Rust
- **prost**: Protocol Buffer implementation
- **tquic**: QUIC implementation (currently problematic)
- **tokio**: Async runtime
- **serde**: Serialization for results

## Critical Issues

### tQUIC Stream Creation Panic
**Location**: `quiccrunch-core/src/stream.rs`
**Issue**: Stream creation panics with "Simple tquic streams require proper integration - not yet implemented"
**Impact**: Blocks all actual benchmark execution

### Missing Streaming Implementations
**Location**: `grpc_comparison.rs`
**Issue**: Only unary pattern implemented, missing 3 streaming patterns
**Impact**: Incomplete benchmark coverage

### Limited Message Size Coverage
**Location**: `grpc_comparison.rs:MESSAGE_SIZES`
**Issue**: Missing granular sizes especially under 1KB
**Impact**: Gaps in performance characterization

## Quick Reference Commands

### Build and Test
```bash
# Enter development environment
nix develop

# Build with features
nix develop -c -- cargo build --features backend-tquic,grpc-comparison

# Run benchmarks
nix develop -c -- cargo run --example grpc_comparison --features backend-tquic,grpc-comparison
```

### Debugging
```bash
# Check tQUIC integration
nix develop -c -- cargo test --features backend-tquic

# Validate compilation
nix develop -c -- cargo check --features grpc-comparison
```