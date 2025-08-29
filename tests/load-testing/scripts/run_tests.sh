#!/bin/bash

# QuicCrunch Load Testing Framework - Quick Start Script

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

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

show_usage() {
    cat << EOF
QuicCrunch Load Testing Framework - Quick Start

Usage: $0 [COMMAND] [OPTIONS]

Commands:
    setup           Initialize the load testing environment
    simple          Run a simple load test (1 minute)
    realistic       Run a realistic workload test (5 minutes)
    stress          Run stress test suite
    e2e             Run end-to-end tests
    regression      Create and check performance baselines
    dashboard       Start the performance dashboard
    clean           Clean up test artifacts

Examples:
    $0 setup                    # Set up the environment
    $0 simple                   # Quick load test
    $0 realistic --duration 300s # Realistic test for 5 minutes
    $0 stress --quick           # Quick stress test suite
    $0 e2e --suite              # Full E2E test suite
    $0 regression --create baseline-v1.0 # Create baseline
    $0 dashboard --port 8080    # Start dashboard on port 8080

For more options, use:
    cargo run --bin load-test -- --help
    cargo run --bin stress-test -- --help
    cargo run --bin e2e-test -- --help
    cargo run --bin regression-test -- --help

EOF
}

setup_environment() {
    log_info "Setting up QuicCrunch Load Testing environment..."
    
    cd "$PROJECT_ROOT"
    
    # Check if we're in the right directory
    if [ ! -f "Cargo.toml" ]; then
        log_error "Not in a valid Rust project directory"
        exit 1
    fi
    
    # Create necessary directories
    log_info "Creating directories..."
    mkdir -p reports
    mkdir -p baselines
    mkdir -p config
    
    # Build the project
    log_info "Building load testing framework..."
    cargo build --release
    
    if [ $? -eq 0 ]; then
        log_success "Load testing framework built successfully"
    else
        log_error "Failed to build load testing framework"
        exit 1
    fi
    
    # Validate configuration
    log_info "Validating configuration..."
    if [ -f "config/default.toml" ]; then
        cargo run --bin load-test -- config validate config/default.toml
        log_success "Configuration validated"
    else
        log_warning "Default configuration not found, generating..."
        cargo run --bin load-test -- config generate --output config/default.toml
    fi
    
    log_success "Environment setup complete!"
    log_info "You can now run load tests using the available commands"
}

run_simple_test() {
    log_info "Running simple load test..."
    
    cd "$PROJECT_ROOT"
    
    cargo run --bin load-test -- run \
        --scenario simple \
        --duration "${DURATION:-60s}" \
        --output reports/simple_$(date +%Y%m%d_%H%M%S) \
        "$@"
}

run_realistic_test() {
    log_info "Running realistic workload test..."
    
    cd "$PROJECT_ROOT"
    
    cargo run --bin load-test -- run \
        --scenario realistic \
        --duration "${DURATION:-300s}" \
        --output reports/realistic_$(date +%Y%m%d_%H%M%S) \
        "$@"
}

run_stress_tests() {
    log_info "Running stress test suite..."
    
    cd "$PROJECT_ROOT"
    
    cargo run --bin stress-test -- suite \
        --output reports/stress_$(date +%Y%m%d_%H%M%S) \
        "$@"
}

run_e2e_tests() {
    log_info "Running end-to-end test suite..."
    
    cd "$PROJECT_ROOT"
    
    cargo run --bin e2e-test -- suite \
        --output reports/e2e_$(date +%Y%m%d_%H%M%S) \
        "$@"
}

run_regression_tests() {
    log_info "Running regression testing..."
    
    cd "$PROJECT_ROOT"
    
    if [ "$1" = "--create" ]; then
        baseline_name="${2:-baseline-$(date +%Y%m%d_%H%M%S)}"
        log_info "Creating baseline: $baseline_name"
        cargo run --bin regression-test -- create-baseline \
            --name "$baseline_name" \
            --scenario simple \
            --runs 3
    elif [ "$1" = "--check" ]; then
        baseline_name="${2:-latest}"
        log_info "Checking regression against: $baseline_name"
        cargo run --bin regression-test -- check \
            --baseline "$baseline_name" \
            --threshold 10.0 \
            --scenario simple
    else
        log_info "Listing available baselines..."
        cargo run --bin regression-test -- list-baselines
    fi
}

start_dashboard() {
    log_info "Starting load testing dashboard..."
    
    cd "$PROJECT_ROOT"
    
    port="${PORT:-8080}"
    log_info "Dashboard will be available at http://localhost:$port"
    
    cargo run --bin load-dashboard -- \
        --port "$port" \
        --reports-dir reports \
        "$@"
}

clean_artifacts() {
    log_info "Cleaning up test artifacts..."
    
    cd "$PROJECT_ROOT"
    
    rm -rf reports/*
    rm -rf target/release/deps/load_test*
    rm -rf target/release/deps/stress_test*
    rm -rf target/release/deps/e2e_test*
    rm -rf target/release/deps/regression_test*
    
    log_success "Cleanup complete"
}

# Main command handling
case "${1:-help}" in
    "setup")
        shift
        setup_environment "$@"
        ;;
    "simple")
        shift
        run_simple_test "$@"
        ;;
    "realistic")
        shift
        run_realistic_test "$@"
        ;;
    "stress")
        shift
        run_stress_tests "$@"
        ;;
    "e2e")
        shift
        run_e2e_tests "$@"
        ;;
    "regression")
        shift
        run_regression_tests "$@"
        ;;
    "dashboard")
        shift
        start_dashboard "$@"
        ;;
    "clean")
        shift
        clean_artifacts "$@"
        ;;
    "help"|"--help"|"-h")
        show_usage
        ;;
    *)
        log_error "Unknown command: $1"
        echo ""
        show_usage
        exit 1
        ;;
esac