#!/bin/bash

# QuicCrunch Network Performance Test Suite Runner
# Comprehensive testing of all streaming variants with detailed reporting

set -euo pipefail

# Configuration
SERVER_BIN="target/debug/perf_test_server"
CLIENT_BIN="target/debug/perf_test_client"
RESULTS_DIR="perf_test_results"
SERVER_ADDR="127.0.0.1:50051"
LOG_LEVEL="info"

# Test parameters
DEFAULT_MESSAGES=10000
DEFAULT_PAYLOAD_SIZES=(1024 4096 16384 65536 262144)
DEFAULT_CONNECTIONS=(1 5 10 20)
DEFAULT_CONCURRENCY=(1 2 4 8)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Utility functions
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

cleanup() {
    log_info "Cleaning up..."
    if [[ -n "${SERVER_PID:-}" ]]; then
        log_info "Stopping server (PID: $SERVER_PID)"
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}

wait_for_server() {
    local max_attempts=30
    local attempt=0
    
    log_info "Waiting for server to start..."
    
    while [[ $attempt -lt $max_attempts ]]; do
        if nc -z 127.0.0.1 50051 2>/dev/null; then
            log_success "Server is ready"
            return 0
        fi
        sleep 1
        ((attempt++))
    done
    
    log_error "Server failed to start within ${max_attempts} seconds"
    return 1
}

start_server() {
    log_info "Starting performance test server..."
    
    # Ensure server binary exists
    if [[ ! -f "$SERVER_BIN" ]]; then
        log_error "Server binary not found: $SERVER_BIN"
        log_info "Please run: cargo build --bin perf_test_server"
        exit 1
    fi
    
    # Start server in background
    "$SERVER_BIN" --bind "$SERVER_ADDR" --verbose > "$RESULTS_DIR/server.log" 2>&1 &
    SERVER_PID=$!
    
    log_info "Server started with PID: $SERVER_PID"
    
    # Wait for server to be ready
    if ! wait_for_server; then
        cleanup
        exit 1
    fi
}

run_test() {
    local test_type="$1"
    local messages="$2"
    local payload_size="$3"
    local connections="$4"
    local concurrency="$5"
    local output_file="$6"
    
    log_info "Running $test_type test: messages=$messages, payload=$payload_size bytes, conn=$connections, conc=$concurrency"
    
    # Ensure client binary exists
    if [[ ! -f "$CLIENT_BIN" ]]; then
        log_error "Client binary not found: $CLIENT_BIN"
        log_info "Please run: cargo build --bin perf_test_client"
        exit 1
    fi
    
    # Run the test
    if "$CLIENT_BIN" \
        --server "$SERVER_ADDR" \
        --test-type "$test_type" \
        --messages "$messages" \
        --payload-size "$payload_size" \
        --connections "$connections" \
        --concurrency "$concurrency" \
        --warmup 2 \
        --output "$output_file" \
        --verbose >> "$RESULTS_DIR/client.log" 2>&1; then
        log_success "$test_type test completed successfully"
        return 0
    else
        log_error "$test_type test failed"
        return 1
    fi
}

run_streaming_variant_tests() {
    local messages="$1"
    local payload_size="$2"
    
    log_info "Running streaming variant tests with $messages messages, $payload_size bytes payload"
    
    local test_types=("unary" "client-streaming" "server-streaming" "bidirectional" "echo")
    
    for test_type in "${test_types[@]}"; do
        local output_file="$RESULTS_DIR/streaming_${test_type}_${payload_size}bytes_${messages}msgs.json"
        if run_test "$test_type" "$messages" "$payload_size" 1 1 "$output_file"; then
            log_success "Streaming test $test_type completed"
        else
            log_error "Streaming test $test_type failed"
        fi
        sleep 1  # Brief pause between tests
    done
}

run_scalability_tests() {
    local test_type="$1"
    local messages="$2"
    local payload_size="$3"
    
    log_info "Running scalability tests for $test_type"
    
    # Test different connection counts
    for connections in "${DEFAULT_CONNECTIONS[@]}"; do
        for concurrency in "${DEFAULT_CONCURRENCY[@]}"; do
            local output_file="$RESULTS_DIR/scalability_${test_type}_conn${connections}_conc${concurrency}_${payload_size}bytes.json"
            if run_test "$test_type" "$messages" "$payload_size" "$connections" "$concurrency" "$output_file"; then
                log_success "Scalability test (conn=$connections, conc=$concurrency) completed"
            else
                log_error "Scalability test (conn=$connections, conc=$concurrency) failed"
            fi
            sleep 2  # Pause between tests to let server recover
        done
    done
}

run_payload_size_tests() {
    local test_type="$1"
    local messages="$2"
    
    log_info "Running payload size tests for $test_type"
    
    for payload_size in "${DEFAULT_PAYLOAD_SIZES[@]}"; do
        local output_file="$RESULTS_DIR/payload_${test_type}_${payload_size}bytes_${messages}msgs.json"
        if run_test "$test_type" "$messages" "$payload_size" 1 1 "$output_file"; then
            log_success "Payload test ($payload_size bytes) completed"
        else
            log_error "Payload test ($payload_size bytes) failed"
        fi
        sleep 1
    done
}

generate_report() {
    log_info "Generating comprehensive performance report..."
    
    local report_file="$RESULTS_DIR/performance_report.md"
    
    cat > "$report_file" << 'EOF'
# QuicCrunch Network Performance Test Report

## Test Environment

- **Test Date:** $(date)
- **Server Address:** 127.0.0.1:50051
- **Test Suite:** Comprehensive streaming variants and scalability tests

## Test Categories

### 1. Streaming Variants Test
Tests all QuicCrunch streaming patterns:
- **Unary**: Simple request/response
- **Client Streaming**: Client sends stream, server responds once
- **Server Streaming**: Client requests once, server streams responses  
- **Bidirectional**: Both client and server stream
- **Echo**: Simple echo test for baseline latency

### 2. Scalability Tests
Tests performance under different load conditions:
- **Connection Scaling**: 1, 5, 10, 20 concurrent connections
- **Concurrency Scaling**: 1, 2, 4, 8 concurrent operations per connection

### 3. Payload Size Tests
Tests performance with different message sizes:
- **Small**: 1KB messages
- **Medium**: 4KB, 16KB messages  
- **Large**: 64KB, 256KB messages

## Results Summary

EOF

    # Add results from JSON files
    for json_file in "$RESULTS_DIR"/*.json; do
        if [[ -f "$json_file" ]]; then
            local filename=$(basename "$json_file" .json)
            echo "### $filename" >> "$report_file"
            echo '```json' >> "$report_file"
            cat "$json_file" >> "$report_file"
            echo '```' >> "$report_file"
            echo "" >> "$report_file"
        fi
    done
    
    log_success "Report generated: $report_file"
}

show_usage() {
    cat << EOF
Usage: $0 [OPTIONS] [TEST_TYPE]

QuicCrunch Network Performance Test Suite

TEST_TYPE:
    streaming     Run all streaming variant tests (default)
    scalability   Run scalability tests (connections/concurrency)
    payload       Run payload size tests
    all          Run all test categories
    custom       Run custom test with specified parameters

OPTIONS:
    -h, --help           Show this help message
    -m, --messages NUM   Number of messages per test (default: $DEFAULT_MESSAGES)
    -s, --server ADDR    Server address (default: $SERVER_ADDR)
    -r, --results DIR    Results directory (default: $RESULTS_DIR)
    -v, --verbose        Enable verbose logging
    --no-server         Don't start server (assume it's already running)
    --skip-build        Skip building binaries
    
Custom test options (only with 'custom'):
    --test-type TYPE     Test type (unary, client-streaming, etc.)
    --payload SIZE       Payload size in bytes
    --connections NUM    Number of connections
    --concurrency NUM    Concurrency per connection

Examples:
    $0                              # Run streaming variant tests
    $0 all                          # Run all test categories
    $0 scalability                  # Run scalability tests only
    $0 custom --test-type unary --payload 4096 --connections 10
    $0 --messages 50000 --verbose   # Custom message count with verbose output

EOF
}

# Parse command line arguments
MESSAGES="$DEFAULT_MESSAGES"
START_SERVER=true
SKIP_BUILD=false
VERBOSE=false
TEST_CATEGORY="streaming"

# Custom test parameters
CUSTOM_TEST_TYPE=""
CUSTOM_PAYLOAD=""
CUSTOM_CONNECTIONS=""
CUSTOM_CONCURRENCY=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            show_usage
            exit 0
            ;;
        -m|--messages)
            MESSAGES="$2"
            shift 2
            ;;
        -s|--server)
            SERVER_ADDR="$2"
            shift 2
            ;;
        -r|--results)
            RESULTS_DIR="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            LOG_LEVEL="debug"
            shift
            ;;
        --no-server)
            START_SERVER=false
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --test-type)
            CUSTOM_TEST_TYPE="$2"
            shift 2
            ;;
        --payload)
            CUSTOM_PAYLOAD="$2"
            shift 2
            ;;
        --connections)
            CUSTOM_CONNECTIONS="$2"
            shift 2
            ;;
        --concurrency)
            CUSTOM_CONCURRENCY="$2"
            shift 2
            ;;
        streaming|scalability|payload|all|custom)
            TEST_CATEGORY="$1"
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    log_info "Starting QuicCrunch Network Performance Test Suite"
    log_info "Test category: $TEST_CATEGORY"
    log_info "Messages per test: $MESSAGES"
    log_info "Results directory: $RESULTS_DIR"
    
    # Create results directory
    mkdir -p "$RESULTS_DIR"
    
    # Build binaries if needed
    if [[ "$SKIP_BUILD" == false ]]; then
        log_info "Building performance test binaries..."
        if ! cargo build --bin perf_test_server --bin perf_test_client; then
            log_error "Failed to build binaries"
            exit 1
        fi
        log_success "Binaries built successfully"
    fi
    
    # Start server if needed
    if [[ "$START_SERVER" == true ]]; then
        start_server
        trap cleanup EXIT
    fi
    
    # Run tests based on category
    case "$TEST_CATEGORY" in
        streaming)
            log_info "Running streaming variant tests..."
            run_streaming_variant_tests "$MESSAGES" 4096
            ;;
        scalability)
            log_info "Running scalability tests..."
            run_scalability_tests "echo" "$MESSAGES" 4096
            ;;
        payload)
            log_info "Running payload size tests..."
            run_payload_size_tests "echo" "$MESSAGES"
            ;;
        all)
            log_info "Running comprehensive test suite..."
            
            # Streaming variants
            run_streaming_variant_tests "$MESSAGES" 4096
            sleep 5
            
            # Scalability tests
            run_scalability_tests "echo" "$MESSAGES" 4096
            sleep 5
            
            # Payload size tests
            run_payload_size_tests "echo" "$MESSAGES"
            ;;
        custom)
            if [[ -z "$CUSTOM_TEST_TYPE" ]]; then
                log_error "Custom test requires --test-type parameter"
                exit 1
            fi
            
            local payload=${CUSTOM_PAYLOAD:-4096}
            local connections=${CUSTOM_CONNECTIONS:-1}
            local concurrency=${CUSTOM_CONCURRENCY:-1}
            local output_file="$RESULTS_DIR/custom_${CUSTOM_TEST_TYPE}_${payload}bytes.json"
            
            log_info "Running custom test..."
            run_test "$CUSTOM_TEST_TYPE" "$MESSAGES" "$payload" "$connections" "$concurrency" "$output_file"
            ;;
        *)
            log_error "Unknown test category: $TEST_CATEGORY"
            exit 1
            ;;
    esac
    
    # Generate report
    generate_report
    
    log_success "Performance test suite completed successfully!"
    log_info "Results available in: $RESULTS_DIR"
}

# Run main function
main "$@"