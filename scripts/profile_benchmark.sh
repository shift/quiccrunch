#!/usr/bin/env bash

# QUIC + Cap'n Proto Performance Test Profiling Script
# This script runs the benchmark with profiling tools as specified in the test specification

set -euo pipefail

# Configuration
BACKEND="${BACKEND:-quinn}"  # quinn or tquic
TEST_CASE="${TEST_CASE:-flood}"  # latency, flood, or stream
ADDRESS="${ADDRESS:-127.0.0.1:50055}"
DURATION="${DURATION:-10}"
OUTPUT_DIR="${OUTPUT_DIR:-./profiling_results}"
PROFILING_TOOL="${PROFILING_TOOL:-perf}"  # perf, heaptrack, or both

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BLUE}🔬 QuicCrunch Profiling Suite${NC}"
    echo -e "${BLUE}==============================${NC}"
    echo "Backend: $BACKEND"
    echo "Test Case: $TEST_CASE"
    echo "Profiling Tool: $PROFILING_TOOL"
    echo "Output Directory: $OUTPUT_DIR"
    echo ""
}

check_requirements() {
    echo -e "${YELLOW}🔍 Checking requirements...${NC}"
    
    # Check if running in nix develop
    if [[ -z "${IN_NIX_SHELL:-}" ]]; then
        echo -e "${RED}❌ This script must be run inside 'nix develop'${NC}"
        echo "Usage: nix develop -c ./scripts/profile_benchmark.sh"
        exit 1
    fi
    
    # Check profiling tools
    case $PROFILING_TOOL in
        "perf")
            if ! command -v perf &> /dev/null; then
                echo -e "${RED}❌ perf not found. Install with: sudo apt install linux-perf${NC}"
                exit 1
            fi
            if ! command -v flamegraph &> /dev/null; then
                echo -e "${YELLOW}⚠️  flamegraph not found. Install with: cargo install flamegraph${NC}"
                echo "Continuing without flamegraph generation..."
            fi
            ;;
        "heaptrack")
            if ! command -v heaptrack &> /dev/null; then
                echo -e "${RED}❌ heaptrack not found. Install with: sudo apt install heaptrack${NC}"
                exit 1
            fi
            ;;
        "both")
            check_requirements_perf=true
            check_requirements_heaptrack=true
            ;;
    esac
    
    # Check if benchmark binary exists
    if [[ ! -f "./target/release/benchmark" ]]; then
        echo -e "${YELLOW}🔨 Building benchmark binary...${NC}"
        cargo build --release --bin benchmark --features "backend-$BACKEND"
    fi
    
    echo -e "${GREEN}✅ Requirements check complete${NC}"
    echo ""
}

create_output_dir() {
    mkdir -p "$OUTPUT_DIR"
    echo "Created output directory: $OUTPUT_DIR"
}

start_server() {
    echo -e "${YELLOW}🚀 Starting benchmark server...${NC}"
    ./target/release/benchmark --address "$ADDRESS" --verbose server > "$OUTPUT_DIR/server.log" 2>&1 &
    SERVER_PID=$!
    
    # Wait for server to start
    sleep 2
    
    if kill -0 "$SERVER_PID" 2>/dev/null; then
        echo -e "${GREEN}✅ Server started (PID: $SERVER_PID)${NC}"
    else
        echo -e "${RED}❌ Failed to start server${NC}"
        cat "$OUTPUT_DIR/server.log"
        exit 1
    fi
}

stop_server() {
    if [[ -n "${SERVER_PID:-}" ]]; then
        echo -e "${YELLOW}🛑 Stopping server...${NC}"
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
        echo -e "${GREEN}✅ Server stopped${NC}"
    fi
}

run_perf_profiling() {
    echo -e "${YELLOW}📊 Running perf profiling...${NC}"
    
    local perf_data="$OUTPUT_DIR/perf_${BACKEND}_${TEST_CASE}.data"
    local client_cmd="./target/release/benchmark --address $ADDRESS client"
    
    case $TEST_CASE in
        "latency")
            client_cmd="$client_cmd latency --iterations 10000"
            ;;
        "flood")
            client_cmd="$client_cmd flood --duration $DURATION --concurrency 100"
            ;;
        "stream")
            client_cmd="$client_cmd stream --size-gib 0.1"  # Smaller size for profiling
            ;;
        *)
            echo -e "${RED}❌ Unknown test case: $TEST_CASE${NC}"
            exit 1
            ;;
    esac
    
    # Record performance data
    echo "Recording performance data..."
    perf record --call-graph dwarf --output "$perf_data" $client_cmd > "$OUTPUT_DIR/client_perf.log" 2>&1
    
    # Generate perf report
    echo "Generating perf report..."
    perf report --input "$perf_data" > "$OUTPUT_DIR/perf_report_${BACKEND}_${TEST_CASE}.txt"
    
    # Generate flamegraph if available
    if command -v flamegraph &> /dev/null; then
        echo "Generating flamegraph..."
        flamegraph --perfdata "$perf_data" --output "$OUTPUT_DIR/flamegraph_${BACKEND}_${TEST_CASE}.svg"
        echo -e "${GREEN}✅ Flamegraph saved to flamegraph_${BACKEND}_${TEST_CASE}.svg${NC}"
    fi
    
    echo -e "${GREEN}✅ Perf profiling complete${NC}"
}

run_heaptrack_profiling() {
    echo -e "${YELLOW}🧠 Running heaptrack profiling...${NC}"
    
    local heaptrack_output="$OUTPUT_DIR/heaptrack_${BACKEND}_${TEST_CASE}"
    local client_cmd="./target/release/benchmark --address $ADDRESS client"
    
    case $TEST_CASE in
        "latency")
            client_cmd="$client_cmd latency --iterations 5000"  # Fewer iterations for memory profiling
            ;;
        "flood")
            client_cmd="$client_cmd flood --duration $((DURATION / 2)) --concurrency 50"  # Reduced load
            ;;
        "stream")
            client_cmd="$client_cmd stream --size-gib 0.05"  # Even smaller size for memory profiling
            ;;
        *)
            echo -e "${RED}❌ Unknown test case: $TEST_CASE${NC}"
            exit 1
            ;;
    esac
    
    # Record heap allocations
    echo "Recording heap allocations..."
    heaptrack --output "$heaptrack_output" $client_cmd > "$OUTPUT_DIR/client_heaptrack.log" 2>&1
    
    # Generate analysis
    echo "Generating heap analysis..."
    heaptrack_print "$heaptrack_output".gz > "$OUTPUT_DIR/heaptrack_analysis_${BACKEND}_${TEST_CASE}.txt"
    
    echo -e "${GREEN}✅ Heaptrack profiling complete${NC}"
}

generate_summary() {
    echo -e "${YELLOW}📋 Generating summary report...${NC}"
    
    local summary_file="$OUTPUT_DIR/profiling_summary_${BACKEND}_${TEST_CASE}.md"
    
    cat > "$summary_file" << EOF
# Profiling Summary

**Test Configuration:**
- Backend: $BACKEND
- Test Case: $TEST_CASE
- Date: $(date)
- Duration: ${DURATION}s
- Address: $ADDRESS

**Files Generated:**
EOF
    
    if [[ "$PROFILING_TOOL" == "perf" || "$PROFILING_TOOL" == "both" ]]; then
        cat >> "$summary_file" << EOF

## Performance Profiling (perf)
- Raw data: \`perf_${BACKEND}_${TEST_CASE}.data\`
- Report: \`perf_report_${BACKEND}_${TEST_CASE}.txt\`
EOF
        if [[ -f "$OUTPUT_DIR/flamegraph_${BACKEND}_${TEST_CASE}.svg" ]]; then
            echo "- Flamegraph: \`flamegraph_${BACKEND}_${TEST_CASE}.svg\`" >> "$summary_file"
        fi
    fi
    
    if [[ "$PROFILING_TOOL" == "heaptrack" || "$PROFILING_TOOL" == "both" ]]; then
        cat >> "$summary_file" << EOF

## Memory Profiling (heaptrack)
- Raw data: \`heaptrack_${BACKEND}_${TEST_CASE}.gz\`
- Analysis: \`heaptrack_analysis_${BACKEND}_${TEST_CASE}.txt\`
EOF
    fi
    
    cat >> "$summary_file" << EOF

## Usage Instructions

### Viewing Results

**Perf Report:**
\`\`\`bash
less $OUTPUT_DIR/perf_report_${BACKEND}_${TEST_CASE}.txt
\`\`\`

**Flamegraph:**
Open \`$OUTPUT_DIR/flamegraph_${BACKEND}_${TEST_CASE}.svg\` in a web browser.

**Heaptrack Analysis:**
\`\`\`bash
less $OUTPUT_DIR/heaptrack_analysis_${BACKEND}_${TEST_CASE}.txt
\`\`\`

### Interactive Analysis

**Perf:**
\`\`\`bash
perf report --input $OUTPUT_DIR/perf_${BACKEND}_${TEST_CASE}.data
\`\`\`

**Heaptrack:**
\`\`\`bash
heaptrack_gui $OUTPUT_DIR/heaptrack_${BACKEND}_${TEST_CASE}.gz
\`\`\`
EOF
    
    echo -e "${GREEN}✅ Summary saved to $summary_file${NC}"
}

cleanup() {
    stop_server
    echo -e "${BLUE}🧹 Cleanup complete${NC}"
}

main() {
    # Set up signal handling
    trap cleanup EXIT INT TERM
    
    print_header
    check_requirements
    create_output_dir
    start_server
    
    case $PROFILING_TOOL in
        "perf")
            run_perf_profiling
            ;;
        "heaptrack")
            run_heaptrack_profiling
            ;;
        "both")
            run_perf_profiling
            run_heaptrack_profiling
            ;;
        *)
            echo -e "${RED}❌ Unknown profiling tool: $PROFILING_TOOL${NC}"
            echo "Valid options: perf, heaptrack, both"
            exit 1
            ;;
    esac
    
    generate_summary
    
    echo ""
    echo -e "${GREEN}🎉 Profiling complete!${NC}"
    echo -e "${GREEN}Results saved to: $OUTPUT_DIR${NC}"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. View the summary: cat $OUTPUT_DIR/profiling_summary_${BACKEND}_${TEST_CASE}.md"
    if [[ -f "$OUTPUT_DIR/flamegraph_${BACKEND}_${TEST_CASE}.svg" ]]; then
        echo "2. Open flamegraph: firefox $OUTPUT_DIR/flamegraph_${BACKEND}_${TEST_CASE}.svg"
    fi
    echo "3. Compare with other backends by changing BACKEND environment variable"
}

# Show usage if --help is passed
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat << EOF
QUIC + Cap'n Proto Performance Test Profiling Script

Usage: nix develop -c ./scripts/profile_benchmark.sh

Environment Variables:
  BACKEND         Backend to use (quinn, tquic) [default: quinn]
  TEST_CASE       Test case to run (latency, flood, stream) [default: flood]
  ADDRESS         Server address [default: 127.0.0.1:50055]
  DURATION        Test duration in seconds [default: 10]
  OUTPUT_DIR      Output directory [default: ./profiling_results]
  PROFILING_TOOL  Profiling tool (perf, heaptrack, both) [default: perf]

Examples:
  # Basic perf profiling with quinn backend
  nix develop -c ./scripts/profile_benchmark.sh

  # Memory profiling with tquic backend
  BACKEND=tquic PROFILING_TOOL=heaptrack nix develop -c ./scripts/profile_benchmark.sh

  # Both profiling tools for latency test
  TEST_CASE=latency PROFILING_TOOL=both nix develop -c ./scripts/profile_benchmark.sh

Requirements:
  - Must run inside 'nix develop'
  - perf: sudo apt install linux-perf
  - flamegraph: cargo install flamegraph
  - heaptrack: sudo apt install heaptrack
EOF
    exit 0
fi

main "$@"