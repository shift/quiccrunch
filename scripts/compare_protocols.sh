#!/bin/bash

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
QUIC_PORT="50055"
GRPC_PORT="50056"
OUTPUT_DIR="protocol_comparison_$(date +%Y%m%d_%H%M%S)"
BENCHMARK_DURATION="10"
ITERATIONS="10000"
MESSAGE_SIZES=(64 256 1024 4096 16384)
CONCURRENCY_LEVELS=(1 10 50 100)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 QuicCrunch Protocol Comparison Benchmark${NC}"
echo -e "${BLUE}==============================================${NC}"
echo ""

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Build the project
echo -e "${YELLOW}📦 Building benchmark binary...${NC}"
cargo build --release --bin benchmark

# Function to run server in background
start_server() {
    local protocol=$1
    local port=$2
    local log_file="$OUTPUT_DIR/server_${protocol}.log"
    
    echo -e "${GREEN}🔧 Starting $protocol server on port $port...${NC}"
    ./target/release/benchmark -a "127.0.0.1:$port" -t "$protocol" server > "$log_file" 2>&1 &
    local server_pid=$!
    
    # Wait for server to start
    sleep 2
    
    # Check if server is still running
    if ! kill -0 $server_pid 2>/dev/null; then
        echo -e "${RED}❌ Failed to start $protocol server${NC}"
        cat "$log_file"
        exit 1
    fi
    
    echo $server_pid
}

# Function to stop server
stop_server() {
    local pid=$1
    local protocol=$2
    
    if kill -0 $pid 2>/dev/null; then
        echo -e "${YELLOW}🔧 Stopping $protocol server (PID: $pid)...${NC}"
        kill $pid
        wait $pid 2>/dev/null || true
    fi
}

# Function to run benchmark test
run_benchmark() {
    local protocol=$1
    local port=$2
    local test_case=$3
    local params=$4
    local output_file=$5
    
    echo -e "${GREEN}🧪 Running $protocol $test_case test: $params${NC}"
    
    ./target/release/benchmark -a "127.0.0.1:$port" -t "$protocol" client $test_case $params > "$output_file" 2>/dev/null
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ $protocol $test_case test completed${NC}"
    else
        echo -e "${RED}❌ $protocol $test_case test failed${NC}"
        return 1
    fi
}

# Main benchmark execution
main() {
    echo -e "${BLUE}🏁 Starting protocol comparison benchmarks...${NC}"
    echo ""
    
    # Test configurations
    declare -A test_results
    
    for protocol in "quic" "grpc"; do
        local port
        if [ "$protocol" = "quic" ]; then
            port=$QUIC_PORT
        else
            port=$GRPC_PORT
        fi
        
        echo -e "${BLUE}📊 Testing $protocol protocol${NC}"
        echo "=================================="
        
        # Start server
        local server_pid
        server_pid=$(start_server "$protocol" "$port")
        
        # Create protocol output directory
        local protocol_dir="$OUTPUT_DIR/$protocol"
        mkdir -p "$protocol_dir"
        
        # Run latency tests with different message sizes
        echo -e "${YELLOW}🕒 Running latency tests...${NC}"
        for size in "${MESSAGE_SIZES[@]}"; do
            local test_name="latency_${size}b"
            local output_file="$protocol_dir/${test_name}.json"
            
            if run_benchmark "$protocol" "$port" "latency" "-n $ITERATIONS -s $size" "$output_file"; then
                test_results["${protocol}_${test_name}"]="$output_file"
            fi
        done
        
        # Run flood tests with different concurrency levels
        echo -e "${YELLOW}🌊 Running flood tests...${NC}"
        for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
            local test_name="flood_c${concurrency}"
            local output_file="$protocol_dir/${test_name}.json"
            
            if run_benchmark "$protocol" "$port" "flood" "-d $BENCHMARK_DURATION -c $concurrency -s 1024" "$output_file"; then
                test_results["${protocol}_${test_name}"]="$output_file"
            fi
        done
        
        # Run stream test
        echo -e "${YELLOW}🚰 Running stream test...${NC}"
        local test_name="stream_1gib"
        local output_file="$protocol_dir/${test_name}.json"
        
        if run_benchmark "$protocol" "$port" "stream" "-s 0.1 --chunk-size 65536" "$output_file"; then
            test_results["${protocol}_${test_name}"]="$output_file"
        fi
        
        # Stop server
        stop_server "$server_pid" "$protocol"
        
        echo ""
    done
    
    # Generate comparison report
    echo -e "${BLUE}📈 Generating comparison report...${NC}"
    generate_report
    
    echo -e "${GREEN}🎉 Protocol comparison completed!${NC}"
    echo -e "${GREEN}📁 Results saved in: $OUTPUT_DIR${NC}"
}

# Function to generate comparison report
generate_report() {
    local report_file="$OUTPUT_DIR/protocol_comparison_report.md"
    
    cat > "$report_file" << EOF
# Protocol Comparison Report

Generated: $(date)

## Test Configuration

- **QUIC Port**: $QUIC_PORT
- **gRPC Port**: $GRPC_PORT
- **Test Duration**: ${BENCHMARK_DURATION}s
- **Latency Iterations**: $ITERATIONS
- **Message Sizes**: ${MESSAGE_SIZES[*]} bytes
- **Concurrency Levels**: ${CONCURRENCY_LEVELS[*]}

## Results Summary

### Latency Tests

| Message Size | QUIC P50 (μs) | QUIC P95 (μs) | gRPC P50 (μs) | gRPC P95 (μs) | QUIC Advantage |
|--------------|---------------|---------------|---------------|---------------|-----------------|
EOF

    # Process latency results
    for size in "${MESSAGE_SIZES[@]}"; do
        local quic_file="$OUTPUT_DIR/quic/latency_${size}b.json"
        local grpc_file="$OUTPUT_DIR/grpc/latency_${size}b.json"
        
        if [ -f "$quic_file" ] && [ -f "$grpc_file" ]; then
            local quic_p50=$(jq -r '.metrics.latency_p50_us // "N/A"' "$quic_file")
            local quic_p95=$(jq -r '.metrics.latency_p95_us // "N/A"' "$quic_file")
            local grpc_p50=$(jq -r '.metrics.latency_p50_us // "N/A"' "$grpc_file")
            local grpc_p95=$(jq -r '.metrics.latency_p95_us // "N/A"' "$grpc_file")
            
            local advantage="N/A"
            if [ "$quic_p50" != "N/A" ] && [ "$grpc_p50" != "N/A" ]; then
                advantage=$(echo "scale=2; $grpc_p50 / $quic_p50" | bc -l 2>/dev/null || echo "N/A")
                if [ "$advantage" != "N/A" ]; then
                    advantage="${advantage}x"
                fi
            fi
            
            echo "| ${size}B | $quic_p50 | $quic_p95 | $grpc_p50 | $grpc_p95 | $advantage |" >> "$report_file"
        fi
    done
    
    cat >> "$report_file" << EOF

### Throughput Tests

| Concurrency | QUIC Msgs/s | QUIC Mbps | gRPC Msgs/s | gRPC Mbps | QUIC Advantage |
|-------------|-------------|-----------|-------------|-----------|-----------------|
EOF

    # Process throughput results
    for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
        local quic_file="$OUTPUT_DIR/quic/flood_c${concurrency}.json"
        local grpc_file="$OUTPUT_DIR/grpc/flood_c${concurrency}.json"
        
        if [ -f "$quic_file" ] && [ -f "$grpc_file" ]; then
            local quic_msgs=$(jq -r '.metrics.messages_per_second // "N/A"' "$quic_file")
            local quic_mbps=$(jq -r '.metrics.throughput_mbps // "N/A"' "$quic_file")
            local grpc_msgs=$(jq -r '.metrics.messages_per_second // "N/A"' "$grpc_file")
            local grpc_mbps=$(jq -r '.metrics.throughput_mbps // "N/A"' "$grpc_file")
            
            local advantage="N/A"
            if [ "$quic_msgs" != "N/A" ] && [ "$grpc_msgs" != "N/A" ]; then
                advantage=$(echo "scale=2; $quic_msgs / $grpc_msgs" | bc -l 2>/dev/null || echo "N/A")
                if [ "$advantage" != "N/A" ]; then
                    advantage="${advantage}x"
                fi
            fi
            
            echo "| $concurrency | $quic_msgs | $quic_mbps | $grpc_msgs | $grpc_mbps | $advantage |" >> "$report_file"
        fi
    done
    
    cat >> "$report_file" << EOF

### Stream Test Results

| Protocol | Bandwidth (Mbps) | Transfer Rate (MB/s) |
|----------|------------------|----------------------|
EOF

    # Process stream results
    for protocol in "quic" "grpc"; do
        local stream_file="$OUTPUT_DIR/$protocol/stream_1gib.json"
        
        if [ -f "$stream_file" ]; then
            local bandwidth=$(jq -r '.metrics.bandwidth_mbps // "N/A"' "$stream_file")
            local transfer_rate=$(jq -r '.metrics.transfer_rate_mbs // "N/A"' "$stream_file")
            
            echo "| $protocol | $bandwidth | $transfer_rate |" >> "$stream_file"
        fi
    done
    
    cat >> "$report_file" << EOF

## Analysis

### Key Findings

1. **Latency Performance**: [Analysis of latency comparison]
2. **Throughput Performance**: [Analysis of throughput comparison]
3. **Bandwidth Performance**: [Analysis of bandwidth comparison]

### Recommendations

- For low-latency applications: [Recommendation]
- For high-throughput applications: [Recommendation]  
- For large data transfers: [Recommendation]

### Technical Notes

- Tests run on single machine with loopback interface
- Results may vary on different network conditions
- QUIC backend: $(./target/release/benchmark --help | grep -o "Backend: .*" || echo "Unknown")

## Raw Data

All raw test results are available in the following directories:
- QUIC results: \`$OUTPUT_DIR/quic/\`
- gRPC results: \`$OUTPUT_DIR/grpc/\`
EOF

    echo -e "${GREEN}📄 Report generated: $report_file${NC}"
}

# Check dependencies
check_dependencies() {
    local missing_deps=()
    
    for cmd in cargo jq bc; do
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

# Run the benchmark
check_dependencies
main

echo -e "${GREEN}✨ All done!${NC}"