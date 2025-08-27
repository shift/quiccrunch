#!/usr/bin/env bash

# QUIC + Cap'n Proto Backend Comparison Script
# Runs comprehensive performance tests comparing quinn and tquic backends

set -euo pipefail

# Configuration
TEST_DURATION="${TEST_DURATION:-10}"
OUTPUT_DIR="${OUTPUT_DIR:-./benchmark_results}"
COMPARISON_DIR="${COMPARISON_DIR:-$OUTPUT_DIR/comparison_$(date +%Y%m%d_%H%M%S)}"
ADDRESS="${ADDRESS:-127.0.0.1:50055}"
PROFILING="${PROFILING:-false}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m'

print_header() {
    echo -e "${PURPLE}🏁 QuicCrunch Backend Comparison Suite${NC}"
    echo -e "${PURPLE}========================================${NC}"
    echo "Test Duration: ${TEST_DURATION}s"
    echo "Output Directory: $COMPARISON_DIR"
    echo "Profiling: $PROFILING"
    echo ""
}

check_requirements() {
    echo -e "${YELLOW}🔍 Checking requirements...${NC}"
    
    if [[ -z "${IN_NIX_SHELL:-}" ]]; then
        echo -e "${RED}❌ This script must be run inside 'nix develop'${NC}"
        echo "Usage: nix develop -c ./scripts/compare_backends.sh"
        exit 1
    fi
    
    # Check if both backends can be built
    echo "Checking quinn backend build..."
    if ! cargo build --release --bin benchmark --features backend-quinn --quiet; then
        echo -e "${RED}❌ Failed to build quinn backend${NC}"
        exit 1
    fi
    
    echo "Checking tquic backend build..."
    if ! cargo build --release --bin benchmark --features backend-tquic --quiet; then
        echo -e "${RED}❌ Failed to build tquic backend${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✅ Both backends build successfully${NC}"
    echo ""
}

create_directories() {
    mkdir -p "$COMPARISON_DIR"/{quinn,tquic,reports}
    echo "Created comparison directory: $COMPARISON_DIR"
}

run_backend_tests() {
    local backend=$1
    local backend_dir="$COMPARISON_DIR/$backend"
    
    echo -e "${BLUE}🧪 Running tests for $backend backend${NC}"
    echo "=================================="
    
    # Build with specific backend
    echo "Building $backend backend..."
    cargo build --release --bin benchmark --features "backend-$backend" --quiet
    
    # Test cases with parameters optimized for comparison
    local test_cases=(
        "latency --iterations 5000 --message-size 64"
        "latency --iterations 5000 --message-size 1024"
        "flood --duration $TEST_DURATION --concurrency 50 --message-size 64"
        "flood --duration $TEST_DURATION --concurrency 100 --message-size 64"
        "flood --duration $TEST_DURATION --concurrency 50 --message-size 1024"
        "stream --size-gib 0.1 --chunk-size 65536"
        "stream --size-gib 0.5 --chunk-size 65536"
    )
    
    # Start server
    echo "Starting $backend server..."
    ./target/release/benchmark --address "$ADDRESS" --verbose server > "$backend_dir/server.log" 2>&1 &
    local server_pid=$!
    sleep 2
    
    if ! kill -0 "$server_pid" 2>/dev/null; then
        echo -e "${RED}❌ Failed to start $backend server${NC}"
        cat "$backend_dir/server.log"
        return 1
    fi
    
    echo -e "${GREEN}✅ $backend server started (PID: $server_pid)${NC}"
    
    # Run each test case
    for i in "${!test_cases[@]}"; do
        local test_case="${test_cases[$i]}"
        local test_name=$(echo "$test_case" | cut -d' ' -f1)
        local test_params=$(echo "$test_case" | cut -d' ' -f2-)
        local output_file="$backend_dir/${test_name}_$(printf '%02d' $i).json"
        
        echo "Running: $test_name ($test_params)"
        
        if timeout 300 ./target/release/benchmark --address "$ADDRESS" client $test_case > "$output_file" 2>"$backend_dir/${test_name}_$(printf '%02d' $i).log"; then
            echo -e "${GREEN}  ✅ $test_name completed${NC}"
        else
            echo -e "${RED}  ❌ $test_name failed or timed out${NC}"
            echo "Error log:" >> "$output_file"
            tail -20 "$backend_dir/server.log" >> "$output_file"
        fi
    done
    
    # Stop server
    echo "Stopping $backend server..."
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
    
    echo -e "${GREEN}✅ $backend tests completed${NC}"
    echo ""
}

run_profiling_comparison() {
    if [[ "$PROFILING" == "true" ]]; then
        echo -e "${YELLOW}📊 Running profiling comparison...${NC}"
        
        # Profile flood test for both backends (most CPU intensive)
        for backend in quinn tquic; do
            echo "Profiling $backend backend..."
            
            BACKEND="$backend" \
            TEST_CASE="flood" \
            DURATION="$TEST_DURATION" \
            OUTPUT_DIR="$COMPARISON_DIR/$backend/profiling" \
            PROFILING_TOOL="perf" \
            ./scripts/profile_benchmark.sh
        done
        
        echo -e "${GREEN}✅ Profiling comparison completed${NC}"
        echo ""
    fi
}

generate_comparison_report() {
    echo -e "${YELLOW}📋 Generating comparison report...${NC}"
    
    local report_file="$COMPARISON_DIR/reports/backend_comparison.md"
    
    cat > "$report_file" << EOF
# QUIC + Cap'n Proto Backend Comparison

**Test Configuration:**
- Date: $(date)
- Test Duration: ${TEST_DURATION}s
- Server Address: $ADDRESS
- Profiling: $PROFILING

## Summary

This report compares the performance of quinn and tquic QUIC backends when used with Cap'n Proto serialization.

## Test Results

### Raw Data Locations
- Quinn backend results: \`quinn/\`
- tQUIC backend results: \`tquic/\`
- Profiling data: \`{backend}/profiling/\` (if enabled)

### Key Metrics Comparison

EOF
    
    # Process results for each test type
    for test_type in latency flood stream; do
        echo "## $test_type Tests" >> "$report_file"
        echo "" >> "$report_file"
        
        # Find all test files for this type
        local quinn_files=($(find "$COMPARISON_DIR/quinn" -name "${test_type}_*.json" | sort))
        local tquic_files=($(find "$COMPARISON_DIR/tquic" -name "${test_type}_*.json" | sort))
        
        if [[ ${#quinn_files[@]} -gt 0 && ${#tquic_files[@]} -gt 0 ]]; then
            echo "| Test Configuration | Quinn | tQUIC | Winner |" >> "$report_file"
            echo "|-------------------|-------|-------|--------|" >> "$report_file"
            
            for i in "${!quinn_files[@]}"; do
                if [[ $i -lt ${#tquic_files[@]} ]]; then
                    local quinn_file="${quinn_files[$i]}"
                    local tquic_file="${tquic_files[$i]}"
                    
                    # Extract key metrics (this is a simplified extraction)
                    local config_name="Configuration $((i+1))"
                    echo "| $config_name | [Results]($(basename "$quinn_file")) | [Results]($(basename "$tquic_file")) | TBD |" >> "$report_file"
                fi
            done
        else
            echo "No results found for $test_type tests." >> "$report_file"
        fi
        
        echo "" >> "$report_file"
    done
    
    cat >> "$report_file" << EOF

## Analysis Instructions

### Automated Analysis
To generate detailed performance metrics and visualizations:

\`\`\`bash
# Install Python dependencies
pip install pandas matplotlib seaborn

# Run analysis script
python scripts/analyze_comparison.py $COMPARISON_DIR
\`\`\`

### Manual Analysis

1. **Latency Comparison:**
   Compare mean, p95, p99 latencies between backends

2. **Throughput Comparison:**
   Compare messages/sec and bandwidth between backends

3. **Resource Usage:**
   If profiling was enabled, compare CPU and memory usage patterns

### Performance Criteria

According to the specification, we're looking for:
- **Latency:** Lower is better (measured in microseconds)
- **Throughput:** Higher is better (messages/sec or Gbps)
- **Resource Usage:** Lower CPU/memory usage is better
- **Consistency:** Lower variance in performance is better

## Recommendations

Based on the test results, consider these factors when choosing a backend:

1. **Primary Use Case:** Latency-sensitive vs. high-throughput
2. **Resource Constraints:** CPU and memory limitations
3. **Consistency Requirements:** Performance predictability needs
4. **Feature Requirements:** Specific QUIC features needed

## Next Steps

1. Run additional tests with different configurations
2. Test on production-like hardware
3. Conduct longer-duration stability tests
4. Profile memory allocation patterns
5. Test with real network conditions (not loopback)

EOF
    
    echo -e "${GREEN}✅ Comparison report saved to $report_file${NC}"
}

create_analysis_script() {
    local analysis_script="$COMPARISON_DIR/reports/analyze_results.py"
    
    cat > "$analysis_script" << 'EOF'
#!/usr/bin/env python3
"""
Backend Comparison Analysis Script

Analyzes JSON results from benchmark runs and generates detailed comparison metrics.
"""

import json
import sys
import os
from pathlib import Path
from statistics import mean, median, stdev
from collections import defaultdict

def load_test_results(results_dir):
    """Load all JSON test results from the comparison directory."""
    quinn_results = []
    tquic_results = []
    
    quinn_dir = Path(results_dir) / "quinn"
    tquic_dir = Path(results_dir) / "tquic"
    
    # Load Quinn results
    for json_file in quinn_dir.glob("*.json"):
        try:
            with open(json_file) as f:
                result = json.load(f)
                result['file'] = json_file.name
                quinn_results.append(result)
        except json.JSONDecodeError:
            print(f"Warning: Could not parse {json_file}")
    
    # Load tQUIC results
    for json_file in tquic_dir.glob("*.json"):
        try:
            with open(json_file) as f:
                result = json.load(f)
                result['file'] = json_file.name
                tquic_results.append(result)
        except json.JSONDecodeError:
            print(f"Warning: Could not parse {json_file}")
    
    return quinn_results, tquic_results

def analyze_latency_tests(quinn_results, tquic_results):
    """Analyze latency test results."""
    print("\n=== LATENCY TEST ANALYSIS ===")
    
    quinn_latency = [r for r in quinn_results if r['test_case'] == 'latency']
    tquic_latency = [r for r in tquic_results if r['test_case'] == 'latency']
    
    print(f"Quinn latency tests: {len(quinn_latency)}")
    print(f"tQUIC latency tests: {len(tquic_latency)}")
    
    for i, (q, t) in enumerate(zip(quinn_latency, tquic_latency)):
        q_metrics = q['metrics']
        t_metrics = t['metrics']
        
        print(f"\nTest {i+1} - Message size: {q_metrics['message_size']} bytes")
        print(f"  Mean latency:   Quinn {q_metrics['mean_us']:.1f}μs  |  tQUIC {t_metrics['mean_us']:.1f}μs")
        print(f"  P95 latency:    Quinn {q_metrics['p95_us']:.1f}μs  |  tQUIC {t_metrics['p95_us']:.1f}μs")
        print(f"  P99 latency:    Quinn {q_metrics['p99_us']:.1f}μs  |  tQUIC {t_metrics['p99_us']:.1f}μs")
        
        # Determine winner
        q_better = 0
        t_better = 0
        
        if q_metrics['mean_us'] < t_metrics['mean_us']:
            q_better += 1
        else:
            t_better += 1
            
        if q_metrics['p95_us'] < t_metrics['p95_us']:
            q_better += 1
        else:
            t_better += 1
            
        if q_metrics['p99_us'] < t_metrics['p99_us']:
            q_better += 1
        else:
            t_better += 1
        
        winner = "Quinn" if q_better > t_better else "tQUIC" if t_better > q_better else "Tie"
        print(f"  Winner: {winner}")

def analyze_throughput_tests(quinn_results, tquic_results):
    """Analyze throughput (flood) test results."""
    print("\n=== THROUGHPUT TEST ANALYSIS ===")
    
    quinn_flood = [r for r in quinn_results if r['test_case'] == 'flood']
    tquic_flood = [r for r in tquic_results if r['test_case'] == 'flood']
    
    print(f"Quinn throughput tests: {len(quinn_flood)}")
    print(f"tQUIC throughput tests: {len(tquic_flood)}")
    
    for i, (q, t) in enumerate(zip(quinn_flood, tquic_flood)):
        q_metrics = q['metrics']
        t_metrics = t['metrics']
        
        q_config = q['configuration']
        t_config = t['configuration']
        
        print(f"\nTest {i+1} - Concurrency: {q_config['concurrency']}, Message size: {q_config['message_size']} bytes")
        print(f"  Messages/sec:   Quinn {q_metrics['messages_per_second']:.0f}  |  tQUIC {t_metrics['messages_per_second']:.0f}")
        print(f"  Throughput:     Quinn {q_metrics['throughput_mbps']:.1f} Mbps  |  tQUIC {t_metrics['throughput_mbps']:.1f} Mbps")
        print(f"  Mean latency:   Quinn {q_metrics['mean_latency_us']:.1f}μs  |  tQUIC {t_metrics['mean_latency_us']:.1f}μs")
        
        # Determine winner
        q_score = 0
        t_score = 0
        
        if q_metrics['messages_per_second'] > t_metrics['messages_per_second']:
            q_score += 2  # Throughput is weighted more heavily
        else:
            t_score += 2
            
        if q_metrics['mean_latency_us'] < t_metrics['mean_latency_us']:
            q_score += 1
        else:
            t_score += 1
        
        winner = "Quinn" if q_score > t_score else "tQUIC" if t_score > q_score else "Tie"
        print(f"  Winner: {winner}")

def analyze_streaming_tests(quinn_results, tquic_results):
    """Analyze streaming (bandwidth) test results."""
    print("\n=== STREAMING TEST ANALYSIS ===")
    
    quinn_stream = [r for r in quinn_results if r['test_case'] == 'stream']
    tquic_stream = [r for r in tquic_results if r['test_case'] == 'stream']
    
    print(f"Quinn streaming tests: {len(quinn_stream)}")
    print(f"tQUIC streaming tests: {len(tquic_stream)}")
    
    for i, (q, t) in enumerate(zip(quinn_stream, tquic_stream)):
        q_metrics = q['metrics']
        t_metrics = t['metrics']
        
        q_config = q['configuration']
        
        print(f"\nTest {i+1} - Data size: {q_config['size_gib']:.1f} GiB")
        print(f"  Throughput:     Quinn {q_metrics['throughput_gbps']:.2f} Gbps  |  tQUIC {t_metrics['throughput_gbps']:.2f} Gbps")
        print(f"  Duration:       Quinn {q_metrics['duration_s']:.1f}s  |  tQUIC {t_metrics['duration_s']:.1f}s")
        print(f"  Chunk rate:     Quinn {q_metrics['chunk_rate_per_s']:.0f}/s  |  tQUIC {t_metrics['chunk_rate_per_s']:.0f}/s")
        
        winner = "Quinn" if q_metrics['throughput_gbps'] > t_metrics['throughput_gbps'] else "tQUIC"
        print(f"  Winner: {winner}")

def generate_summary(quinn_results, tquic_results):
    """Generate overall summary."""
    print("\n=== OVERALL SUMMARY ===")
    print(f"Total Quinn tests: {len(quinn_results)}")
    print(f"Total tQUIC tests: {len(tquic_results)}")
    
    # Count test types
    quinn_by_type = defaultdict(int)
    tquic_by_type = defaultdict(int)
    
    for result in quinn_results:
        quinn_by_type[result['test_case']] += 1
    
    for result in tquic_results:
        tquic_by_type[result['test_case']] += 1
    
    print("\nTest distribution:")
    for test_type in ['latency', 'flood', 'stream']:
        print(f"  {test_type}: Quinn {quinn_by_type[test_type]}, tQUIC {tquic_by_type[test_type]}")

def main():
    if len(sys.argv) != 2:
        print("Usage: python analyze_results.py <comparison_directory>")
        sys.exit(1)
    
    results_dir = sys.argv[1]
    
    if not os.path.exists(results_dir):
        print(f"Error: Directory {results_dir} does not exist")
        sys.exit(1)
    
    print(f"Analyzing results from: {results_dir}")
    
    quinn_results, tquic_results = load_test_results(results_dir)
    
    if not quinn_results or not tquic_results:
        print("Error: No valid results found for one or both backends")
        sys.exit(1)
    
    generate_summary(quinn_results, tquic_results)
    analyze_latency_tests(quinn_results, tquic_results)
    analyze_throughput_tests(quinn_results, tquic_results)
    analyze_streaming_tests(quinn_results, tquic_results)
    
    print(f"\nAnalysis complete. Results saved in {results_dir}/reports/")

if __name__ == "__main__":
    main()
EOF
    
    chmod +x "$analysis_script"
    echo "Created analysis script: $analysis_script"
}

cleanup() {
    # Kill any remaining server processes
    pkill -f "benchmark server" 2>/dev/null || true
    echo -e "${BLUE}🧹 Cleanup complete${NC}"
}

main() {
    trap cleanup EXIT INT TERM
    
    print_header
    check_requirements
    create_directories
    
    # Run tests for both backends
    run_backend_tests "quinn"
    run_backend_tests "tquic"
    
    # Optional profiling comparison
    run_profiling_comparison
    
    # Generate reports
    generate_comparison_report
    create_analysis_script
    
    echo ""
    echo -e "${GREEN}🎉 Backend comparison complete!${NC}"
    echo -e "${GREEN}Results saved to: $COMPARISON_DIR${NC}"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. View comparison report: cat $COMPARISON_DIR/reports/backend_comparison.md"
    echo "2. Run detailed analysis: python $COMPARISON_DIR/reports/analyze_results.py $COMPARISON_DIR"
    echo "3. Compare profiling results (if enabled): ls $COMPARISON_DIR/*/profiling/"
}

# Show usage
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat << EOF
QUIC + Cap'n Proto Backend Comparison Script

Usage: nix develop -c ./scripts/compare_backends.sh

Environment Variables:
  TEST_DURATION   Duration for each test in seconds [default: 10]
  OUTPUT_DIR      Base output directory [default: ./benchmark_results]
  ADDRESS         Server address [default: 127.0.0.1:50055]
  PROFILING       Enable profiling comparison (true/false) [default: false]

Examples:
  # Basic comparison
  nix develop -c ./scripts/compare_backends.sh

  # Longer tests with profiling
  TEST_DURATION=30 PROFILING=true nix develop -c ./scripts/compare_backends.sh

This script will:
1. Build both quinn and tquic backends
2. Run comprehensive tests for each backend
3. Generate comparison reports
4. Optionally run profiling analysis
EOF
    exit 0
fi

main "$@"