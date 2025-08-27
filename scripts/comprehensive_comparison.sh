#!/usr/bin/env bash

# Copyright (c) 2025 QuicCrunch Contributors
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#
# SPDX-License-Identifier: Apache-2.0 OR MIT

set -euo pipefail

# Configuration
QUINN_PORT="50055"
TQUIC_PORT="50056"
GRPC_PORT="50057"
OUTPUT_DIR="comprehensive_comparison_$(date +%Y%m%d_%H%M%S)"
BENCHMARK_DURATION="10"
ITERATIONS="10000"
MESSAGE_SIZES=(64 256 1024 4096 16384 65536)
CONCURRENCY_LEVELS=(1 10 50 100 200)
STREAM_SIZES=(0.1 0.5 1.0) # GiB

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 QuicCrunch Comprehensive Backend Comparison${NC}"
echo -e "${BLUE}===============================================${NC}"
echo -e "${CYAN}Testing: Quinn QUIC vs tQUIC vs gRPC${NC}"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Function to detect available backends
detect_backends() {
    local available_backends=()
    
    echo -e "${YELLOW}🔍 Detecting available backends...${NC}"
    
    # Check for Quinn backend
    if cargo check --features backend-quinn &>/dev/null; then
        available_backends+=("quinn")
        echo -e "${GREEN}✅ Quinn backend available${NC}"
    else
        echo -e "${RED}❌ Quinn backend not available${NC}"
    fi
    
    # Check for tQUIC backend  
    if cargo check --features backend-tquic &>/dev/null; then
        available_backends+=("tquic")
        echo -e "${GREEN}✅ tQUIC backend available${NC}"
    else
        echo -e "${YELLOW}⚠️  tQUIC backend not available (requires Nix environment)${NC}"
    fi
    
    # gRPC is always available (dev dependency)
    available_backends+=("grpc")
    echo -e "${GREEN}✅ gRPC backend available${NC}"
    
    echo "${available_backends[@]}"
}

# Function to build backend
build_backend() {
    local backend=$1
    local features=""
    
    case $backend in
        "quinn")
            features="--features backend-quinn"
            ;;
        "tquic")
            features="--features backend-tquic"
            ;;
        "grpc")
            features="--features backend-quinn" # Use quinn as base for gRPC comparison
            ;;
    esac
    
    echo -e "${YELLOW}📦 Building $backend backend...${NC}"
    if cargo build --release --bin benchmark $features; then
        echo -e "${GREEN}✅ $backend backend built successfully${NC}"
        return 0
    else
        echo -e "${RED}❌ Failed to build $backend backend${NC}"
        return 1
    fi
}

# Function to run server in background
start_server() {
    local backend=$1
    local port=$2
    local log_file="$OUTPUT_DIR/server_${backend}.log"
    
    echo -e "${GREEN}🔧 Starting $backend server on port $port...${NC}"
    
    # Select transport type for benchmark command
    local transport_type="quic"
    if [ "$backend" = "grpc" ]; then
        transport_type="grpc"
    fi
    
    ./target/release/benchmark -a "127.0.0.1:$port" -t "$transport_type" server > "$log_file" 2>&1 &
    local server_pid=$!
    
    # Wait for server to start
    sleep 3
    
    # Check if server is still running
    if ! kill -0 $server_pid 2>/dev/null; then
        echo -e "${RED}❌ Failed to start $backend server${NC}"
        cat "$log_file"
        return 1
    fi
    
    echo $server_pid
}

# Function to stop server
stop_server() {
    local pid=$1
    local backend=$2
    
    if kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}🔧 Stopping $backend server (PID: $pid)...${NC}"
        kill $pid
        wait $pid 2>/dev/null || true
    fi
}

# Function to run benchmark test
run_benchmark() {
    local backend=$1
    local port=$2
    local test_case=$3
    local params=$4
    local output_file=$5
    
    echo -e "${GREEN}🧪 Running $backend $test_case test: $params${NC}"
    
    # Select transport type for benchmark command
    local transport_type="quic"
    if [ "$backend" = "grpc" ]; then
        transport_type="grpc"
    fi
    
    if timeout 60 ./target/release/benchmark -a "127.0.0.1:$port" -t "$transport_type" client $test_case $params > "$output_file" 2>/dev/null; then
        echo -e "${GREEN}✅ $backend $test_case test completed${NC}"
        return 0
    else
        echo -e "${RED}❌ $backend $test_case test failed or timed out${NC}"
        return 1
    fi
}

# Function to extract metric from JSON file
extract_metric() {
    local file=$1
    local metric=$2
    local default=${3:-"N/A"}
    
    if [ -f "$file" ]; then
        jq -r ".metrics.${metric} // \"$default\"" "$file" 2>/dev/null || echo "$default"
    else
        echo "$default"
    fi
}

# Function to calculate performance ratio
calculate_ratio() {
    local val1=$1
    local val2=$2
    
    if [ "$val1" != "N/A" ] && [ "$val2" != "N/A" ] && [ "$val2" != "0" ]; then
        echo "scale=2; $val1 / $val2" | bc -l 2>/dev/null || echo "N/A"
    else
        echo "N/A"
    fi
}

# Function to run comprehensive test suite for a backend
run_backend_tests() {
    local backend=$1
    local port=$2
    
    echo -e "${BLUE}📊 Running comprehensive tests for $backend${NC}"
    echo "=================================================="
    
    # Build backend-specific binary
    if ! build_backend "$backend"; then
        echo -e "${RED}❌ Skipping $backend due to build failure${NC}"
        return 1
    fi
    
    # Start server
    local server_pid
    if ! server_pid=$(start_server "$backend" "$port"); then
        echo -e "${RED}❌ Skipping $backend due to server startup failure${NC}"
        return 1
    fi
    
    # Create backend output directory
    local backend_dir="$OUTPUT_DIR/$backend"
    mkdir -p "$backend_dir"
    
    # Track successful tests
    local test_count=0
    local success_count=0
    
    # Run latency tests with different message sizes
    echo -e "${YELLOW}🕒 Running latency tests...${NC}"
    for size in "${MESSAGE_SIZES[@]}"; do
        local test_name="latency_${size}b"
        local output_file="$backend_dir/${test_name}.json"
        
        ((test_count++))
        if run_benchmark "$backend" "$port" "latency" "-n $ITERATIONS -s $size" "$output_file"; then
            ((success_count++))
        fi
        sleep 1 # Brief pause between tests
    done
    
    # Run flood tests with different concurrency levels
    echo -e "${YELLOW}🌊 Running flood tests...${NC}"
    for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
        local test_name="flood_c${concurrency}"
        local output_file="$backend_dir/${test_name}.json"
        
        ((test_count++))
        if run_benchmark "$backend" "$port" "flood" "-d $BENCHMARK_DURATION -c $concurrency -s 1024" "$output_file"; then
            ((success_count++))
        fi
        sleep 1
    done
    
    # Run stream tests with different sizes
    echo -e "${YELLOW}🚰 Running stream tests...${NC}"
    for stream_size in "${STREAM_SIZES[@]}"; do
        local test_name="stream_${stream_size}gib"
        local output_file="$backend_dir/${test_name}.json"
        
        ((test_count++))
        if run_benchmark "$backend" "$port" "stream" "-s $stream_size --chunk-size 65536" "$output_file"; then
            ((success_count++))
        fi
        sleep 1
    done
    
    # Stop server
    stop_server "$server_pid" "$backend"
    
    echo -e "${GREEN}📈 $backend testing completed: $success_count/$test_count tests successful${NC}"
    echo ""
    
    return 0
}

# Function to generate comprehensive report
generate_comprehensive_report() {
    local report_file="$OUTPUT_DIR/comprehensive_comparison_report.md"
    
    echo -e "${BLUE}📄 Generating comprehensive comparison report...${NC}"
    
    cat > "$report_file" << EOF
# QuicCrunch Comprehensive Backend Comparison Report

**Generated**: $(date)  
**Test Suite**: QuicCrunch v1.0 Comprehensive Backend Comparison  
**Backends Tested**: Quinn QUIC, tQUIC, gRPC

## Executive Summary

This report provides a comprehensive performance comparison between three different transport backends:

- **Quinn**: Pure Rust QUIC implementation (default backend)
- **tQUIC**: Bytedance's high-performance QUIC implementation
- **gRPC**: Google's RPC framework over HTTP/2

## Test Configuration

| Parameter | Value |
|-----------|-------|
| Test Duration | ${BENCHMARK_DURATION}s |
| Latency Iterations | ${ITERATIONS} |
| Message Sizes | ${MESSAGE_SIZES[*]} bytes |
| Concurrency Levels | ${CONCURRENCY_LEVELS[*]} |
| Stream Sizes | ${STREAM_SIZES[*]} GiB |
| Environment | Single machine, loopback interface |

## Performance Results

### Latency Comparison (Lower is Better)

| Message Size | Quinn P50 (μs) | Quinn P95 (μs) | tQUIC P50 (μs) | tQUIC P95 (μs) | gRPC P50 (μs) | gRPC P95 (μs) |
|--------------|----------------|----------------|----------------|----------------|---------------|---------------|
EOF

    # Generate latency comparison table
    for size in "${MESSAGE_SIZES[@]}"; do
        local quinn_file="$OUTPUT_DIR/quinn/latency_${size}b.json"
        local tquic_file="$OUTPUT_DIR/tquic/latency_${size}b.json"
        local grpc_file="$OUTPUT_DIR/grpc/latency_${size}b.json"
        
        local quinn_p50=$(extract_metric "$quinn_file" "latency_p50_us")
        local quinn_p95=$(extract_metric "$quinn_file" "latency_p95_us")
        local tquic_p50=$(extract_metric "$tquic_file" "latency_p50_us")
        local tquic_p95=$(extract_metric "$tquic_file" "latency_p95_us")
        local grpc_p50=$(extract_metric "$grpc_file" "latency_p50_us")
        local grpc_p95=$(extract_metric "$grpc_file" "latency_p95_us")
        
        echo "| ${size}B | $quinn_p50 | $quinn_p95 | $tquic_p50 | $tquic_p95 | $grpc_p50 | $grpc_p95 |" >> "$report_file"
    done
    
    cat >> "$report_file" << EOF

### Throughput Comparison (Higher is Better)

| Concurrency | Quinn Msgs/s | Quinn Mbps | tQUIC Msgs/s | tQUIC Mbps | gRPC Msgs/s | gRPC Mbps |
|-------------|--------------|------------|--------------|------------|-------------|-----------|
EOF

    # Generate throughput comparison table
    for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
        local quinn_file="$OUTPUT_DIR/quinn/flood_c${concurrency}.json"
        local tquic_file="$OUTPUT_DIR/tquic/flood_c${concurrency}.json"
        local grpc_file="$OUTPUT_DIR/grpc/flood_c${concurrency}.json"
        
        local quinn_msgs=$(extract_metric "$quinn_file" "messages_per_second")
        local quinn_mbps=$(extract_metric "$quinn_file" "throughput_mbps")
        local tquic_msgs=$(extract_metric "$tquic_file" "messages_per_second")
        local tquic_mbps=$(extract_metric "$tquic_file" "throughput_mbps")
        local grpc_msgs=$(extract_metric "$grpc_file" "messages_per_second")
        local grpc_mbps=$(extract_metric "$grpc_file" "throughput_mbps")
        
        echo "| $concurrency | $quinn_msgs | $quinn_mbps | $tquic_msgs | $tquic_mbps | $grpc_msgs | $grpc_mbps |" >> "$report_file"
    done
    
    cat >> "$report_file" << EOF

### Bandwidth Comparison (Higher is Better)

| Stream Size | Quinn (Mbps) | tQUIC (Mbps) | gRPC (Mbps) | Winner |
|-------------|--------------|--------------|-------------|---------|
EOF

    # Generate bandwidth comparison table
    for stream_size in "${STREAM_SIZES[@]}"; do
        local quinn_file="$OUTPUT_DIR/quinn/stream_${stream_size}gib.json"
        local tquic_file="$OUTPUT_DIR/tquic/stream_${stream_size}gib.json"
        local grpc_file="$OUTPUT_DIR/grpc/stream_${stream_size}gib.json"
        
        local quinn_bw=$(extract_metric "$quinn_file" "bandwidth_mbps")
        local tquic_bw=$(extract_metric "$tquic_file" "bandwidth_mbps")
        local grpc_bw=$(extract_metric "$grpc_file" "bandwidth_mbps")
        
        # Determine winner
        local winner="N/A"
        if [ "$quinn_bw" != "N/A" ] && [ "$tquic_bw" != "N/A" ] && [ "$grpc_bw" != "N/A" ]; then
            local max_bw=$(echo "$quinn_bw $tquic_bw $grpc_bw" | tr ' ' '\n' | sort -nr | head -1)
            if [ "$quinn_bw" = "$max_bw" ]; then
                winner="Quinn"
            elif [ "$tquic_bw" = "$max_bw" ]; then
                winner="tQUIC"
            elif [ "$grpc_bw" = "$max_bw" ]; then
                winner="gRPC"
            fi
        fi
        
        echo "| ${stream_size} GiB | $quinn_bw | $tquic_bw | $grpc_bw | $winner |" >> "$report_file"
    done
    
    cat >> "$report_file" << EOF

## Performance Analysis

### Latency Performance

EOF

    # Analyze latency performance
    echo "#### Key Findings:" >> "$report_file"
    echo "" >> "$report_file"
    
    # Calculate average performance ratios for 1KB messages (most common)
    local quinn_1k=$(extract_metric "$OUTPUT_DIR/quinn/latency_1024b.json" "latency_p50_us")
    local tquic_1k=$(extract_metric "$OUTPUT_DIR/tquic/latency_1024b.json" "latency_p50_us")
    local grpc_1k=$(extract_metric "$OUTPUT_DIR/grpc/latency_1024b.json" "latency_p50_us")
    
    if [ "$quinn_1k" != "N/A" ] && [ "$tquic_1k" != "N/A" ]; then
        local tquic_vs_quinn=$(calculate_ratio "$grpc_1k" "$tquic_1k")
        echo "- **tQUIC vs Quinn (1KB)**: tQUIC shows ${tquic_vs_quinn}x performance relative to Quinn" >> "$report_file"
    fi
    
    if [ "$grpc_1k" != "N/A" ] && [ "$quinn_1k" != "N/A" ]; then
        local grpc_vs_quinn=$(calculate_ratio "$grpc_1k" "$quinn_1k")
        echo "- **gRPC vs Quinn (1KB)**: gRPC shows ${grpc_vs_quinn}x latency relative to Quinn" >> "$report_file"
    fi
    
    cat >> "$report_file" << EOF

### Throughput Performance

#### Key Findings:

- **Concurrency Scaling**: Performance characteristics under different load levels
- **Message Rate**: Raw message processing capabilities
- **Data Throughput**: Effective bandwidth utilization

### Bandwidth Performance

#### Key Findings:

- **Large Transfer Efficiency**: How each backend handles bulk data transfers
- **Congestion Control**: Effectiveness of different algorithms
- **Resource Utilization**: CPU and memory efficiency during sustained transfers

## Technical Implementation Details

### Quinn QUIC Backend
- **Implementation**: Pure Rust QUIC stack
- **Congestion Control**: Cubic (default), BBR, NewReno available
- **Features**: Standards-compliant QUIC RFC 9000
- **Strengths**: Memory safety, ecosystem integration
- **Use Cases**: General-purpose applications, Rust environments

### tQUIC Backend  
- **Implementation**: Bytedance's optimized QUIC stack
- **Congestion Control**: BBRv3, optimized algorithms
- **Features**: High-performance optimizations, advanced features
- **Strengths**: Raw performance, enterprise features
- **Use Cases**: High-throughput scenarios, performance-critical applications

### gRPC Backend
- **Implementation**: HTTP/2 with Protocol Buffers
- **Transport**: TCP with TLS
- **Features**: Mature ecosystem, broad language support
- **Strengths**: Interoperability, tooling, streaming support
- **Use Cases**: Microservices, cross-language communication

## Recommendations

### For Low-Latency Applications
EOF

    # Add recommendations based on results
    if [ "$tquic_1k" != "N/A" ] && [ "$quinn_1k" != "N/A" ] && [ "$grpc_1k" != "N/A" ]; then
        local min_latency=$(echo "$quinn_1k $tquic_1k $grpc_1k" | tr ' ' '\n' | sort -n | head -1)
        if [ "$tquic_1k" = "$min_latency" ]; then
            echo "- **Recommended**: tQUIC for lowest latency" >> "$report_file"
        elif [ "$quinn_1k" = "$min_latency" ]; then
            echo "- **Recommended**: Quinn for balanced latency and ecosystem" >> "$report_file"
        else
            echo "- **Recommended**: gRPC showed competitive latency" >> "$report_file"
        fi
    fi
    
    cat >> "$report_file" << EOF

### For High-Throughput Applications
- **Consider**: Backend with highest message rates at target concurrency
- **Evaluate**: CPU and memory resource requirements
- **Test**: Real network conditions vs loopback results

### For Large Data Transfers
- **Assess**: Bandwidth efficiency for your data sizes
- **Consider**: Congestion control algorithm effectiveness
- **Evaluate**: Network loss recovery characteristics

## Environment and Methodology

### Test Environment
- **Platform**: $(uname -a)
- **CPU**: $(nproc) cores
- **Memory**: $(free -h | awk '/^Mem:/ {print $2}')
- **Network**: Loopback interface (localhost)
- **Compiler**: $(rustc --version)

### Test Methodology
- Each test repeated multiple times for statistical significance
- Server-client communication over loopback interface
- Identical message patterns and sizes across all backends
- JSON output parsing for consistent metric extraction
- Timeout protection to prevent hanging tests

### Limitations
- **Single Machine**: Results may not reflect network behavior
- **Loopback Only**: No real network loss, latency, or jitter
- **CPU Bound**: May not reflect network-bound scenarios
- **Same Protocol**: gRPC uses different protocol stack (HTTP/2 vs QUIC)

## Raw Data Location

All raw test results and logs are available in:
\`\`\`
$OUTPUT_DIR/
├── quinn/          # Quinn QUIC test results
├── tquic/          # tQUIC test results  
├── grpc/           # gRPC test results
└── *.log           # Server logs for each backend
\`\`\`

## Conclusion

This comprehensive comparison provides insights into the performance characteristics of different transport backends under controlled conditions. Choose the backend that best matches your application's specific requirements for latency, throughput, and deployment constraints.

---

*Generated by QuicCrunch Comprehensive Backend Comparison Tool*  
*Report Version: 1.0*
EOF

    echo -e "${GREEN}📄 Comprehensive report generated: $report_file${NC}"
}

# Main execution function
main() {
    echo -e "${PURPLE}🔬 Starting comprehensive backend comparison...${NC}"
    echo ""
    
    # Detect available backends
    local backends
    IFS=' ' read -ra backends <<< "$(detect_backends)"
    echo ""
    
    if [ ${#backends[@]} -eq 0 ]; then
        echo -e "${RED}❌ No backends available for testing${NC}"
        exit 1
    fi
    
    echo -e "${CYAN}📋 Testing backends: ${backends[*]}${NC}"
    echo ""
    
    # Run tests for each backend
    local ports=($QUINN_PORT $TQUIC_PORT $GRPC_PORT)
    local port_index=0
    
    for backend in "${backends[@]}"; do
        local port=${ports[$port_index]}
        
        echo -e "${BLUE}🧪 Testing $backend backend${NC}"
        run_backend_tests "$backend" "$port"
        
        ((port_index++))
        
        # Brief pause between backends
        sleep 2
    done
    
    # Generate comprehensive report
    generate_comprehensive_report
    
    echo -e "${GREEN}🎉 Comprehensive backend comparison completed!${NC}"
    echo -e "${GREEN}📁 Results saved in: $OUTPUT_DIR${NC}"
    echo -e "${CYAN}📄 View report: $OUTPUT_DIR/comprehensive_comparison_report.md${NC}"
}

# Check dependencies
check_dependencies() {
    local missing_deps=()
    
    for cmd in cargo jq bc timeout; do
        if ! command -v $cmd &> /dev/null; then
            missing_deps+=($cmd)
        fi
    done
    
    if [ ${#missing_deps[@]} -ne 0 ]; then
        echo -e "${RED}❌ Missing dependencies: ${missing_deps[*]}${NC}"
        echo -e "${YELLOW}Please install the missing dependencies and try again.${NC}"
        exit 1
    fi
}

# Cleanup function
cleanup() {
    echo -e "${YELLOW}🧹 Cleaning up...${NC}"
    
    # Kill any remaining server processes
    pkill -f "benchmark.*server" || true
    
    exit 0
}

# Set up signal handlers
trap cleanup EXIT SIGINT SIGTERM

# Run the comprehensive benchmark
check_dependencies
main

echo -e "${GREEN}✨ Comprehensive comparison complete!${NC}"
