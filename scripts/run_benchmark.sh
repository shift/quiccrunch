#!/bin/bash

# QUIC + Cap'n Proto Performance Test Runner
# Supports profiling with perf and flamegraph

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Default values
BACKEND="quinn"
TEST_TYPE="all"
PROFILE=false
PROFILE_TYPE="cpu"
OUTPUT_DIR="benchmark_results"
SERVER_ADDR="127.0.0.1:50055"
VERBOSE=false

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

usage() {
    cat << EOF
Usage: $0 [OPTIONS]

QUIC + Cap'n Proto Performance Test Runner

OPTIONS:
    -b, --backend BACKEND       Backend to test (quinn|tquic) [default: quinn]
    -t, --test TEST            Test type (latency|flood|stream|all) [default: all]
    -p, --profile              Enable profiling
    --profile-type TYPE        Profiling type (cpu|memory) [default: cpu]
    -o, --output DIR           Output directory [default: benchmark_results]
    -a, --address ADDR         Server address [default: 127.0.0.1:50055]
    -v, --verbose              Verbose output
    -h, --help                 Show this help

EXAMPLES:
    # Run all tests with Quinn backend
    $0 --backend quinn --test all

    # Run latency test with profiling
    $0 --backend quinn --test latency --profile

    # Compare both backends
    $0 --backend quinn --test all
    $0 --backend tquic --test all

    # Run with memory profiling
    $0 --backend quinn --test flood --profile --profile-type memory

PROFILING:
    CPU Profiling: Uses perf record + flamegraph
    Memory Profiling: Uses heaptrack
    
    Results are saved to OUTPUT_DIR with timestamps.
EOF
}

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

check_dependencies() {
    local missing_deps=()
    
    # Check basic tools
    if ! command -v cargo &> /dev/null; then
        missing_deps+=("cargo")
    fi
    
    if [ "$PROFILE" = true ]; then
        if [ "$PROFILE_TYPE" = "cpu" ]; then
            if ! command -v perf &> /dev/null; then
                missing_deps+=("perf")
            fi
            if ! command -v flamegraph &> /dev/null; then
                log_warning "flamegraph not found, installing..."
                cargo install flamegraph || missing_deps+=("flamegraph")
            fi
        elif [ "$PROFILE_TYPE" = "memory" ]; then
            if ! command -v heaptrack &> /dev/null; then
                missing_deps+=("heaptrack")
            fi
        fi
    fi
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        log_error "Missing dependencies: ${missing_deps[*]}"
        echo "Please install the missing dependencies and try again."
        exit 1
    fi
}

build_benchmark() {
    local backend=$1
    log_info "Building benchmark binary with $backend backend..."
    
    cd "$PROJECT_ROOT"
    
    local features="backend-$backend"
    cargo build --release --bin benchmark --features "$features"
    
    if [ $? -ne 0 ]; then
        log_error "Failed to build benchmark binary"
        exit 1
    fi
    
    log_success "Benchmark binary built successfully"
}

start_server() {
    local backend=$1
    local addr=$2
    
    log_info "Starting server with $backend backend on $addr..."
    
    local cmd="$PROJECT_ROOT/target/release/benchmark"
    local args="server --address $addr"
    
    if [ "$VERBOSE" = true ]; then
        args="$args --verbose"
    fi
    
    if [ "$PROFILE" = true ] && [ "$PROFILE_TYPE" = "cpu" ]; then
        # Profile the server
        local profile_output="$OUTPUT_DIR/${backend}_server_profile_$(date +%Y%m%d_%H%M%S)"
        mkdir -p "$OUTPUT_DIR"
        
        log_info "Starting server with CPU profiling..."
        perf record -g --call-graph dwarf -o "$profile_output.perf.data" $cmd $args &
        SERVER_PID=$!
    elif [ "$PROFILE" = true ] && [ "$PROFILE_TYPE" = "memory" ]; then
        # Profile the server with heaptrack
        local profile_output="$OUTPUT_DIR/${backend}_server_memory_$(date +%Y%m%d_%H%M%S)"
        mkdir -p "$OUTPUT_DIR"
        
        log_info "Starting server with memory profiling..."
        heaptrack -o "$profile_output" $cmd $args &
        SERVER_PID=$!
    else
        # Normal server start
        $cmd $args &
        SERVER_PID=$!
    fi
    
    # Wait for server to start
    sleep 2
    
    # Check if server is running
    if ! kill -0 $SERVER_PID 2>/dev/null; then
        log_error "Server failed to start"
        exit 1
    fi
    
    log_success "Server started (PID: $SERVER_PID)"
}

stop_server() {
    if [ -n "$SERVER_PID" ]; then
        log_info "Stopping server (PID: $SERVER_PID)..."
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
        
        if [ "$PROFILE" = true ] && [ "$PROFILE_TYPE" = "cpu" ]; then
            # Generate flamegraph
            local profile_files=$(find "$OUTPUT_DIR" -name "*server_profile_*.perf.data" -newer /tmp/benchmark_start 2>/dev/null | head -1)
            if [ -n "$profile_files" ]; then
                local flamegraph_output="${profile_files%.perf.data}.svg"
                log_info "Generating flamegraph: $flamegraph_output"
                perf script -i "$profile_files" | flamegraph > "$flamegraph_output"
                log_success "Flamegraph generated: $flamegraph_output"
            fi
        fi
        
        log_success "Server stopped"
    fi
}

run_test() {
    local backend=$1
    local test_type=$2
    local addr=$3
    
    log_info "Running $test_type test with $backend backend..."
    
    local cmd="$PROJECT_ROOT/target/release/benchmark"
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local output_file="$OUTPUT_DIR/${backend}_${test_type}_${timestamp}.json"
    
    mkdir -p "$OUTPUT_DIR"
    
    local args="client"
    if [ "$VERBOSE" = true ]; then
        args="$args --verbose"
    fi
    args="$args --address $addr"
    
    case $test_type in
        "latency")
            args="$args latency --iterations 10000 --message-size 64"
            ;;
        "flood")
            args="$args flood --duration 10 --concurrency 100 --message-size 64"
            ;;
        "stream")
            args="$args stream --size-gib 1.0 --chunk-size 65536"
            ;;
        *)
            log_error "Unknown test type: $test_type"
            return 1
            ;;
    esac
    
    if [ "$PROFILE" = true ] && [ "$PROFILE_TYPE" = "cpu" ]; then
        # Profile the client
        local profile_output="$OUTPUT_DIR/${backend}_${test_type}_client_profile_${timestamp}"
        
        log_info "Running client with CPU profiling..."
        perf record -g --call-graph dwarf -o "$profile_output.perf.data" $cmd $args > "$output_file"
        
        # Generate flamegraph
        local flamegraph_output="$profile_output.svg"
        log_info "Generating flamegraph: $flamegraph_output"
        perf script -i "$profile_output.perf.data" | flamegraph > "$flamegraph_output"
        log_success "Flamegraph generated: $flamegraph_output"
        
    elif [ "$PROFILE" = true ] && [ "$PROFILE_TYPE" = "memory" ]; then
        # Profile the client with heaptrack
        local profile_output="$OUTPUT_DIR/${backend}_${test_type}_client_memory_${timestamp}"
        
        log_info "Running client with memory profiling..."
        heaptrack -o "$profile_output" $cmd $args > "$output_file"
        
    else
        # Normal client run
        $cmd $args > "$output_file"
    fi
    
    if [ $? -eq 0 ]; then
        log_success "Test completed: $output_file"
        
        # Show summary if not profiling
        if [ "$PROFILE" = false ] || [ "$VERBOSE" = true ]; then
            echo ""
            echo "=== Test Summary ==="
            if command -v jq &> /dev/null; then
                jq -r '.metrics | to_entries[] | "\(.key): \(.value)"' "$output_file" | head -10
            else
                echo "Install jq for formatted output, or check: $output_file"
            fi
            echo ""
        fi
    else
        log_error "Test failed"
        return 1
    fi
}

cleanup() {
    stop_server
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -b|--backend)
            BACKEND="$2"
            shift 2
            ;;
        -t|--test)
            TEST_TYPE="$2"
            shift 2
            ;;
        -p|--profile)
            PROFILE=true
            shift
            ;;
        --profile-type)
            PROFILE_TYPE="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -a|--address)
            SERVER_ADDR="$2"
            shift 2
            ;;
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Validate arguments
if [[ "$BACKEND" != "quinn" && "$BACKEND" != "tquic" ]]; then
    log_error "Invalid backend: $BACKEND (must be quinn or tquic)"
    exit 1
fi

if [[ "$TEST_TYPE" != "latency" && "$TEST_TYPE" != "flood" && "$TEST_TYPE" != "stream" && "$TEST_TYPE" != "all" ]]; then
    log_error "Invalid test type: $TEST_TYPE (must be latency, flood, stream, or all)"
    exit 1
fi

if [[ "$PROFILE_TYPE" != "cpu" && "$PROFILE_TYPE" != "memory" ]]; then
    log_error "Invalid profile type: $PROFILE_TYPE (must be cpu or memory)"
    exit 1
fi

# Set up cleanup trap
trap cleanup EXIT INT TERM

# Create start marker for profiling
touch /tmp/benchmark_start

# Main execution
log_info "QuicCrunch Performance Test Runner"
log_info "Backend: $BACKEND"
log_info "Test: $TEST_TYPE"
log_info "Profiling: $PROFILE ($PROFILE_TYPE)"
log_info "Output: $OUTPUT_DIR"
echo ""

check_dependencies
build_benchmark "$BACKEND"
start_server "$BACKEND" "$SERVER_ADDR"

if [ "$TEST_TYPE" = "all" ]; then
    run_test "$BACKEND" "latency" "$SERVER_ADDR"
    run_test "$BACKEND" "flood" "$SERVER_ADDR"
    run_test "$BACKEND" "stream" "$SERVER_ADDR"
else
    run_test "$BACKEND" "$TEST_TYPE" "$SERVER_ADDR"
fi

log_success "All tests completed successfully!"