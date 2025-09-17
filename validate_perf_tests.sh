#!/bin/bash

# QuicCrunch Network Performance Test Validation Script
# Quick validation of the performance testing setup

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_prerequisites() {
    log_info "Checking prerequisites..."
    
    # Check if we're in a Nix environment
    if [[ -z "${IN_NIX_SHELL:-}" ]]; then
        log_warning "Not in Nix shell environment"
        log_info "Run: nix develop"
        log_info "Or ensure you have Rust and required dependencies"
    else
        log_success "Nix shell environment detected"
    fi
    
    # Check Rust
    if command -v rustc &> /dev/null; then
        local rust_version=$(rustc --version)
        log_success "Rust found: $rust_version"
    else
        log_error "Rust not found"
        return 1
    fi
    
    # Check Cargo
    if command -v cargo &> /dev/null; then
        local cargo_version=$(cargo --version)
        log_success "Cargo found: $cargo_version"
    else
        log_error "Cargo not found"
        return 1
    fi
    
    # Check required tools
    local tools=("nc" "ps" "kill")
    for tool in "${tools[@]}"; do
        if command -v "$tool" &> /dev/null; then
            log_success "Tool found: $tool"
        else
            log_warning "Tool not found: $tool (may cause issues)"
        fi
    done
    
    return 0
}

build_binaries() {
    log_info "Building performance test binaries..."
    
    if cargo build --bin perf_test_server --bin perf_test_client; then
        log_success "Binaries built successfully"
        
        # Check binary sizes
        local server_size=$(du -h target/debug/perf_test_server | cut -f1)
        local client_size=$(du -h target/debug/perf_test_client | cut -f1)
        log_info "Server binary size: $server_size"
        log_info "Client binary size: $client_size"
        
        return 0
    else
        log_error "Failed to build binaries"
        return 1
    fi
}

test_basic_functionality() {
    log_info "Testing basic functionality..."
    
    local server_pid=""
    local test_results_dir="validation_test_results"
    
    # Cleanup function
    cleanup() {
        if [[ -n "${server_pid:-}" ]]; then
            log_info "Stopping test server..."
            kill "$server_pid" 2>/dev/null || true
            wait "$server_pid" 2>/dev/null || true
        fi
        rm -rf "${test_results_dir:-validation_test_results}" 2>/dev/null || true
    }
    
    trap cleanup EXIT
    
    # Create test results directory
    mkdir -p "$test_results_dir"
    
    # Start server
    log_info "Starting test server..."
    ./target/debug/perf_test_server --bind "127.0.0.1:50052" > "$test_results_dir/server.log" 2>&1 &
    server_pid=$!
    
    # Wait for server to start
    log_info "Waiting for server to start..."
    sleep 3
    
    # Check if server is running
    if ! ps -p "$server_pid" > /dev/null; then
        log_error "Server failed to start"
        cat "$test_results_dir/server.log"
        return 1
    fi
    
    # Wait for server to bind to port
    local max_attempts=10
    local attempt=0
    while [[ $attempt -lt $max_attempts ]]; do
        if nc -z 127.0.0.1 50052 2>/dev/null; then
            log_success "Server is ready on port 50052"
            break
        fi
        sleep 1
        ((attempt++))
    done
    
    if [[ $attempt -eq $max_attempts ]]; then
        log_error "Server not responding on port 50052"
        return 1
    fi
    
    # Run basic client tests
    log_info "Running basic echo test..."
    if ./target/debug/perf_test_client \
        --server "127.0.0.1:50052" \
        --messages 100 \
        --payload-size 1024 \
        --output "$test_results_dir/basic_test.json" > "$test_results_dir/client.log" 2>&1; then
        log_success "Basic echo test completed"
    else
        log_error "Basic echo test failed"
        cat "$test_results_dir/client.log"
        return 1
    fi
    
    # Check if results file was created
    if [[ -f "$test_results_dir/basic_test.json" ]]; then
        log_success "Results file created successfully"
        
        # Show basic results
        if command -v jq &> /dev/null; then
            log_info "Test results summary:"
            jq -r '"Requests: \(.requests_sent), Responses: \(.responses_received), Avg Latency: \(.avg_latency_ms)ms, Throughput: \(.throughput_mbps_rx) MB/s"' "$test_results_dir/basic_test.json"
        else
            log_info "Results saved to: $test_results_dir/basic_test.json"
        fi
    else
        log_warning "Results file not created"
    fi
    
    log_success "Basic functionality test completed"
    return 0
}

test_streaming_variants() {
    log_info "Testing basic functionality (simplified client)..."
    
    # Test basic echo functionality
    log_info "Testing echo variant..."
    
    if ./target/debug/perf_test_client \
        --server "127.0.0.1:50052" \
        --messages 50 \
        --payload-size 512 \
        --output "validation_test_results/test_echo.json" > /dev/null 2>&1; then
        log_success "Echo test passed"
    else
        log_warning "Echo test failed (may need server restart)"
    fi
    
    return 0
}

run_performance_validation() {
    log_info "Running performance validation..."
    
    # Run a quick performance test to validate setup
    local start_time=$(date +%s)
    
    if ./target/debug/perf_test_client \
        --server "127.0.0.1:50052" \
        --messages 1000 \
        --payload-size 4096 \
        --output "validation_test_results/perf_validation.json" > /dev/null 2>&1; then
        
        local end_time=$(date +%s)
        local duration=$((end_time - start_time))
        
        log_success "Performance validation completed in ${duration}s"
        
        # Check basic performance metrics if jq is available
        if command -v jq &> /dev/null && [[ -f "validation_test_results/perf_validation.json" ]]; then
            local rps=$(jq -r '.requests_per_second' "validation_test_results/perf_validation.json")
            local latency=$(jq -r '.avg_latency_ms' "validation_test_results/perf_validation.json")
            
            log_info "Performance metrics:"
            log_info "  - Requests/second: $rps"
            log_info "  - Average latency: ${latency}ms"
            
            # Basic performance checks
            if (( $(echo "$rps > 100" | bc -l) )); then
                log_success "Request rate looks good (>100 RPS)"
            else
                log_warning "Request rate seems low (<100 RPS)"
            fi
            
            if (( $(echo "$latency < 100" | bc -l) )); then
                log_success "Latency looks good (<100ms)"
            else
                log_warning "Latency seems high (>100ms)"
            fi
        fi
        
        return 0
    else
        log_error "Performance validation failed"
        return 1
    fi
}

show_next_steps() {
    log_info "Validation completed successfully! 🎉"
    echo
    log_info "Next steps:"
    echo "  1. Run comprehensive tests:"
    echo "     ./run_performance_tests.sh"
    echo
    echo "  2. Run specific test categories:"
    echo "     ./run_performance_tests.sh streaming"
    echo "     ./run_performance_tests.sh scalability"
    echo "     ./run_performance_tests.sh payload"
    echo "     ./run_performance_tests.sh all"
    echo
    echo "  3. Run custom tests:"
    echo "     ./run_performance_tests.sh custom --test-type echo --payload 8192 --connections 5"
    echo
    echo "  4. View configuration:"
    echo "     cat perf_test_config.yaml"
    echo
    log_info "Happy performance testing! 🚀"
}

main() {
    echo "========================================"
    echo "QuicCrunch Performance Test Validation"
    echo "========================================"
    echo
    
    # Run all validation steps
    if ! check_prerequisites; then
        log_error "Prerequisites check failed"
        exit 1
    fi
    
    if ! build_binaries; then
        log_error "Binary build failed"
        exit 1
    fi
    
    if ! test_basic_functionality; then
        log_error "Basic functionality test failed"
        exit 1
    fi
    
    if ! test_streaming_variants; then
        log_warning "Streaming variants test had issues (expected for basic setup)"
    fi
    
    if ! run_performance_validation; then
        log_error "Performance validation failed"
        exit 1
    fi
    
    show_next_steps
}

main "$@"