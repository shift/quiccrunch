#!/bin/bash
# gRPC vs QuicCrunch Performance Comparison Script
# This script runs honest performance comparisons and generates reports

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RESULTS_DIR="$PROJECT_ROOT/benchmark_results"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[$(date +'%H:%M:%S')]${NC} $*"
}

error() {
    echo -e "${RED}[ERROR]${NC} $*"
    exit 1
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

# Create results directory
mkdir -p "$RESULTS_DIR"

# Check if we have gRPC comparison features
check_grpc_support() {
    log "Checking gRPC comparison support..."
    
    if ! grep -q 'grpc-comparison' "$PROJECT_ROOT/quiccrunch-core/Cargo.toml"; then
        error "gRPC comparison feature not found in Cargo.toml"
    fi
    
    if ! grep -q 'tonic' "$PROJECT_ROOT/quiccrunch-core/Cargo.toml"; then
        error "Tonic dependency not found - gRPC comparison not available"
    fi
    
    success "gRPC comparison support detected"
}

# Build the benchmark with appropriate features
build_benchmark() {
    log "Building QuicCrunch with gRPC comparison features..."
    
    cd "$PROJECT_ROOT"
    
    # Use the optimized build system
    if nix develop -c -- cargo build --example grpc_comparison --features="backend-tquic,grpc-comparison" --release; then
        success "Benchmark built successfully"
    else
        error "Failed to build benchmark"
    fi
}

# Run the comparison benchmark
run_benchmark() {
    log "Running gRPC vs QuicCrunch performance comparison..."
    
    cd "$PROJECT_ROOT"
    
    # Set up environment for optimal performance
    export RUST_LOG=warn
    export RUST_BACKTRACE=1
    
    # Run the benchmark
    if nix develop -c -- cargo run --example grpc_comparison --features="backend-tquic,grpc-comparison" --release; then
        success "Benchmark completed successfully"
    else
        error "Benchmark execution failed"
    fi
}

# Run QuicCrunch-only tests if gRPC comparison is not available
run_quiccrunch_only() {
    warn "gRPC comparison not available - running QuicCrunch-only benchmark"
    
    cd "$PROJECT_ROOT"
    
    if nix develop -c -- cargo run --example grpc_comparison --features="backend-tquic" --release; then
        success "QuicCrunch benchmark completed"
    else
        error "QuicCrunch benchmark failed"
    fi
}

# Move results to organized location
organize_results() {
    log "Organizing benchmark results..."
    
    cd "$PROJECT_ROOT"
    
    # Find latest result files
    LATEST_JSON=$(ls -t grpc_comparison_results_*.json 2>/dev/null | head -1 || echo "")
    LATEST_MD=$(ls -t grpc_comparison_summary_*.md 2>/dev/null | head -1 || echo "")
    
    if [[ -n "$LATEST_JSON" ]]; then
        mv "$LATEST_JSON" "$RESULTS_DIR/"
        log "Moved JSON results to $RESULTS_DIR/$LATEST_JSON"
    fi
    
    if [[ -n "$LATEST_MD" ]]; then
        mv "$LATEST_MD" "$RESULTS_DIR/"
        log "Moved summary report to $RESULTS_DIR/$LATEST_MD"
        
        # Also show the summary
        echo
        echo "📄 BENCHMARK SUMMARY:"
        echo "===================="
        cat "$RESULTS_DIR/$LATEST_MD"
    fi
}

# Generate release comparison report
generate_release_report() {
    log "Generating release comparison report..."
    
    local release_version="${1:-dev}"
    local report_file="$RESULTS_DIR/release_comparison_${release_version}_$(date +%Y%m%d_%H%M%S).md"
    
    cat > "$report_file" << EOF
# QuicCrunch Release Performance Report

**Release Version:** $release_version  
**Test Date:** $(date -Iseconds)  
**Environment:** $(uname -a)  
**Rust Version:** $(rustc --version)  

## Performance vs gRPC

EOF

    # Append the latest summary if available
    LATEST_MD=$(ls -t "$RESULTS_DIR"/grpc_comparison_summary_*.md 2>/dev/null | head -1 || echo "")
    if [[ -n "$LATEST_MD" ]]; then
        cat "$LATEST_MD" >> "$report_file"
    else
        echo "No gRPC comparison results available." >> "$report_file"
    fi
    
    cat >> "$report_file" << EOF

## Build Configuration

\`\`\`toml
features = ["backend-tquic", "grpc-comparison"]
profile = "release"
\`\`\`

## Test Environment

- **OS:** $(uname -s)
- **CPU:** $(nproc) cores
- **Memory:** $(free -h | awk '/^Mem:/ { print $2 }')
- **Network:** Localhost loopback

## Reproducibility

To reproduce these results:

\`\`\`bash
./scripts/run_grpc_comparison.sh
\`\`\`

Or manually:

\`\`\`bash
cargo run --example grpc_comparison --features="backend-tquic,grpc-comparison" --release
\`\`\`
EOF

    success "Release report generated: $report_file"
}

# Show usage information
show_usage() {
    echo "Usage: $0 [OPTIONS] [RELEASE_VERSION]"
    echo ""
    echo "Options:"
    echo "  --check-only    Only check gRPC support, don't run benchmarks"
    echo "  --quic-only     Run QuicCrunch benchmarks only (skip gRPC)"
    echo "  --help          Show this help"
    echo ""
    echo "Examples:"
    echo "  $0                    # Run full comparison"
    echo "  $0 v0.1.0             # Run comparison and tag as release v0.1.0"
    echo "  $0 --quic-only        # Test QuicCrunch performance only"
    echo "  $0 --check-only       # Check if gRPC comparison is available"
}

# Main execution
main() {
    local check_only=false
    local quic_only=false
    local release_version="dev"
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --check-only)
                check_only=true
                shift
                ;;
            --quic-only)
                quic_only=true
                shift
                ;;
            --help)
                show_usage
                exit 0
                ;;
            *)
                release_version="$1"
                shift
                ;;
        esac
    done
    
    log "Starting gRPC vs QuicCrunch performance comparison"
    log "Release version: $release_version"
    
    # Check gRPC support
    if ! check_grpc_support 2>/dev/null; then
        if [[ "$check_only" == "true" ]]; then
            error "gRPC comparison not available"
        else
            warn "gRPC comparison not available - will run QuicCrunch-only tests"
            quic_only=true
        fi
    fi
    
    if [[ "$check_only" == "true" ]]; then
        success "gRPC comparison is available"
        exit 0
    fi
    
    # Build the benchmark
    build_benchmark
    
    # Run appropriate benchmark
    if [[ "$quic_only" == "true" ]]; then
        run_quiccrunch_only
    else
        run_benchmark
    fi
    
    # Organize results
    organize_results
    
    # Generate release report
    generate_release_report "$release_version"
    
    success "Performance comparison completed!"
    log "Results available in: $RESULTS_DIR"
}

# Execute main with all arguments
main "$@"