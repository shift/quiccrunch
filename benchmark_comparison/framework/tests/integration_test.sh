#!/bin/bash

# Integration test script for benchmark framework
# Tests the framework with both gRPC and QUICCRUNCH implementations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRAMEWORK_DIR="$(dirname "$SCRIPT_DIR")"

echo "Running benchmark framework integration tests..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test functions
test_result() {
    if [ $1 -eq 0 ]; then
        echo -e "${GREEN}✓${NC} $2"
        return 0
    else
        echo -e "${RED}✗${NC} $2"
        return 1
    fi
}

warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

info() {
    echo "ℹ $1"
}

# Change to framework directory
cd "$FRAMEWORK_DIR"

# Test 1: Build framework
echo "Test 1: Building framework..."
if cargo build --release; then
    test_result 0 "Framework build successful"
else
    test_result 1 "Framework build failed"
    exit 1
fi

# Test 2: Initialize configuration
echo "Test 2: Configuration initialization..."
if ./target/release/benchmark_runner init --config test_config.yaml --default; then
    test_result 0 "Configuration initialization successful"
else
    test_result 1 "Configuration initialization failed"
    exit 1
fi

# Test 3: Validate environment
echo "Test 3: Environment validation..."
if ./target/release/benchmark_runner validate --system --dependencies; then
    test_result 0 "Environment validation passed"
else
    test_result 1 "Environment validation failed"
    warning "Some dependencies may be missing, but continuing with tests"
fi

# Test 4: Configuration validation
echo "Test 4: Configuration validation..."
if ./target/release/benchmark_runner run --config test_config.yaml --implementations "nonexistent" --scenarios "nonexistent" --dry-run 2>/dev/null; then
    test_result 1 "Configuration validation should have failed"
else
    test_result 0 "Configuration validation correctly rejected invalid config"
fi

# Test 5: Storage functionality
echo "Test 5: Storage functionality..."
mkdir -p test_storage
cat > test_storage/test_result.json << 'EOF'
{
  "test_id": "test_123",
  "implementation_name": "test_impl",
  "scenario_name": "test_scenario",
  "start_time": "2023-01-01T00:00:00Z",
  "end_time": "2023-01-01T00:01:00Z",
  "success": true,
  "metrics": {
    "total_messages": 1000,
    "total_bytes": 1024000,
    "duration": {
      "secs": 60,
      "nanos": 0
    },
    "latency_min": {
      "secs": 0,
      "nanos": 100000
    },
    "latency_max": {
      "secs": 0,
      "nanos": 10000000
    },
    "latency_mean": {
      "secs": 0,
      "nanos": 500000
    },
    "latency_p50": {
      "secs": 0,
      "nanos": 400000
    },
    "latency_p95": {
      "secs": 0,
      "nanos": 2000000
    },
    "latency_p99": {
      "secs": 0,
      "nanos": 5000000
    },
    "latency_p99_9": {
      "secs": 0,
      "nanos": 8000000
    },
    "throughput_mps": 16.67,
    "throughput_bps": 17066.67,
    "connection_time": {
      "secs": 0,
      "nanos": 50000000
    },
    "system_metrics": {
      "cpu_usage_percent": 25.5,
      "memory_used_mb": 512,
      "memory_total_mb": 8192,
      "network_rx_bytes": 1024000,
      "network_tx_bytes": 1024000,
      "disk_usage_percent": 45.2,
      "load_average": 1.25,
      "process_count": 150
    },
    "errors": {
      "total_errors": 0,
      "connection_errors": 0,
      "timeout_errors": 0,
      "protocol_errors": 0,
      "network_errors": 0,
      "other_errors": 0,
      "error_rate": 0.0
    }
  }
}
EOF

if [ -f "test_storage/test_result.json" ]; then
    test_result 0 "Test data creation successful"
else
    test_result 1 "Test data creation failed"
fi

# Test 6: Report generation
echo "Test 6: Report generation..."
if ./target/release/benchmark_runner report --input test_storage --output test_reports --format json 2>/dev/null; then
    test_result 0 "Report generation successful"
else
    test_result 1 "Report generation failed"
    warning "Report generation may require actual test results"
fi

# Test 7: Cleanup functionality
echo "Test 7: Cleanup functionality..."
if ./target/release/benchmark_runner cleanup --temp; then
    test_result 0 "Cleanup functionality successful"
else
    test_result 1 "Cleanup functionality failed"
fi

# Test 8: Help and documentation
echo "Test 8: Help and documentation..."
if ./target/release/benchmark_runner --help > /dev/null 2>&1; then
    test_result 0 "Help command works"
else
    test_result 1 "Help command failed"
fi

# Test 9: Configuration examples
echo "Test 9: Configuration examples..."
if [ -f "examples/grpc_config.yaml" ] && [ -f "examples/quiccrunch_config.yaml" ]; then
    test_result 0 "Configuration examples exist"
else
    test_result 1 "Configuration examples missing"
fi

# Test 10: Script functionality
echo "Test 10: Script functionality..."
if [ -f "run_benchmark.sh" ] && [ -f "quick_test.sh" ] && [ -f "cleanup.sh" ]; then
    test_result 0 "Helper scripts exist"
else
    test_result 1 "Helper scripts missing"
fi

# Cleanup test artifacts
echo "Cleaning up test artifacts..."
rm -rf test_storage test_reports test_config.yaml

# Summary
echo ""
echo "=== Integration Test Summary ==="
echo "Framework integration tests completed."
echo ""
echo "Next steps:"
echo "1. Review any warnings above"
echo "2. Configure implementations in benchmark_config.yaml"
echo "3. Run './run_benchmark.sh' for actual benchmarks"
echo "4. Check './reports' directory for results"
echo ""

# Check if we're in a Nix environment
if [ -n "$IN_NIX_SHELL" ]; then
    info "Running in Nix environment - framework should work correctly"
else
    warning "Not in Nix environment - consider running 'nix develop'"
    info "Or use 'nix develop -c ./run_benchmark.sh' for commands"
fi

echo "Integration tests completed successfully!"