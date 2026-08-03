#!/usr/bin/env bash
# Multi-threaded benchmark script - runs inside nix develop shell
# Usage: nix develop --command bash ./benchmark_network_mt_fixed.sh

set -e

echo "=== QUICCRUNCH Multi-threaded Benchmark ==="
echo ""

RESULTS_DIR="./benchmark_results_mt"
mkdir -p "$RESULTS_DIR"

# Test configurations - minimal for quick validation
MESSAGE_SIZES=(64)
MESSAGE_COUNTS=(100 1000)
CONCURRENCY_LEVELS=(1 5)

# Path to binaries
SERVER_BIN="./target/debug/mt_server"
CLIENT_BIN="./target/debug/quic_client"
CERT_FILE="quiccrunch-core/tests/certs/cert.pem"
KEY_FILE="quiccrunch-core/tests/certs/key.pem"

# Check binaries exist
if [ ! -f "$SERVER_BIN" ] || [ ! -f "$CLIENT_BIN" ]; then
    echo "ERROR: Binaries not found. Run: cargo build --bin quic_server --bin quic_client --features backend-tquic"
    exit 1
fi

# Check dependencies
if ! command -v bc &> /dev/null; then
    echo "ERROR: bc is required for calculations but not found"
    exit 1
fi

# Create CSV header
echo "TestID,MessageSize,MessageCount,Concurrency,Duration,ThroughputMsgPerSec,ThroughputMbps,AvgLatencyMs,Success,TotalBytesSent,TotalBytesReceived" > "$RESULTS_DIR/benchmark_results.csv"

echo "Starting benchmarks..."
echo ""

# Function to run single benchmark
run_benchmark() {
    local msg_size=$1
    local msg_count=$2
    local concurrency=$3
    local test_id="${msg_size}B_${msg_count}msg_${concurrency}c"
    local port=$((5000 + RANDOM % 1000))
    
    echo "Running: $test_id (port $port)"
    
    # Start server
    RUST_LOG=warn $SERVER_BIN "127.0.0.1:$port" 1 "$CERT_FILE" "$KEY_FILE" > "$RESULTS_DIR/server_${test_id}.log" 2>&1 &
    local server_pid=$!
    
    # Wait for server to start
    sleep 2
    
    # Check if server is running
    if ! kill -0 $server_pid 2>/dev/null; then
        echo "  ERROR: Server failed to start"
        cat "$RESULTS_DIR/server_${test_id}.log"
        return 1
    fi
    
    # Run clients in parallel
    local start_time=$(date +%s.%N)
    local client_pids=()
    
    for ((i=1; i<=concurrency; i++)); do
        {
            $CLIENT_BIN "127.0.0.1:$port" $msg_count 1 > "$RESULTS_DIR/client_${test_id}_${i}.log" 2>&1
            echo $? > "$RESULTS_DIR/client_${test_id}_${i}.exit"
        } &
        client_pids+=($!)
    done
    
    # Wait for all clients with timeout
    local timeout=60
    local start_wait=$(date +%s)
    for pid in "${client_pids[@]}"; do
        if ! wait $pid; then
            echo "  Client $pid failed or timed out"
        fi
        # Check overall timeout
        if [ $(($(date +%s) - start_wait)) -gt $timeout ]; then
            echo "  Overall timeout reached, killing remaining clients"
            kill "${client_pids[@]}" 2>/dev/null || true
            break
        fi
    done
    
    local end_time=$(date +%s.%N)
    local duration=$(echo "$end_time - $start_time" | bc -l)
    
    # Stop server
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true
    sleep 1  # Allow port to be released
    
    # Check results
    local success=1
    local total_bytes_sent=0
    local total_bytes_received=0
    
    for ((i=1; i<=concurrency; i++)); do
        if [ -f "$RESULTS_DIR/client_${test_id}_${i}.exit" ]; then
            local exit_code=$(cat "$RESULTS_DIR/client_${test_id}_${i}.exit")
            if [ "$exit_code" != "0" ]; then
                success=0
            fi
            
            # Parse bytes from log
            local bytes_sent=$(grep "Bytes sent:" "$RESULTS_DIR/client_${test_id}_${i}.log" | sed 's/.*Bytes sent: \([0-9]*\).*/\1/')
            local bytes_received=$(grep "Bytes received:" "$RESULTS_DIR/client_${test_id}_${i}.log" | sed 's/.*Bytes received: \([0-9]*\).*/\1/')
            total_bytes_sent=$((total_bytes_sent + ${bytes_sent:-0}))
            total_bytes_received=$((total_bytes_received + ${bytes_received:-0}))
        else
            success=0
        fi
    done
    
    # Calculate metrics
    local total_messages=$((msg_count * concurrency))
    local throughput=$(echo "scale=2; $total_messages / $duration" | bc -l)
    local throughput_mbps=$(echo "scale=2; ($total_bytes_sent + $total_bytes_received) * 8 / $duration / 1000000" | bc -l)
    local avg_latency_ms=$(echo "scale=4; ($duration / $total_messages) * 1000" | bc -l)
    
    # Save results
    echo "$test_id,$msg_size,$msg_count,$concurrency,$duration,$throughput,$throughput_mbps,$avg_latency_ms,$success,$total_bytes_sent,$total_bytes_received" >> "$RESULTS_DIR/benchmark_results.csv"
    
    echo "  Duration: ${duration}s, Throughput: ${throughput} msg/s (${throughput_mbps} Mbps), Latency: ${avg_latency_ms} ms, Success: $success"
}

echo "Starting multi-threaded benchmarks..."
echo ""

# Run benchmarks
for msg_size in "${MESSAGE_SIZES[@]}"; do
    for msg_count in "${MESSAGE_COUNTS[@]}"; do
        for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
            run_benchmark $msg_size $msg_count $concurrency
            echo ""
            sleep 2
        done
    done
done

echo "=== Multi-threaded Benchmark Summary ==="
echo ""

successful_tests=$(awk -F',' '$9=="1" {count++} END {print count+0}' "$RESULTS_DIR/benchmark_results.csv")
total_tests=$(wc -l < "$RESULTS_DIR/benchmark_results.csv")
total_tests=$((total_tests - 1))

echo "Total tests: $total_tests"
echo "Successful tests: $successful_tests"
if [ "$successful_tests" -gt 0 ]; then
    echo "Success rate: $(echo "scale=1; $successful_tests * 100 / $total_tests" | bc -l)%"
    echo ""
    
    echo "Best results by throughput:"
    awk -F',' '$9=="1" && NR>1 {print $0}' "$RESULTS_DIR/benchmark_results.csv" | sort -t',' -k6 -nr | head -5 | while IFS=',' read -r test_id msg_size msg_count concurrency duration throughput throughput_mbps latency success bytes_sent bytes_received; do
        echo "  $test_id: ${throughput} msg/s (${throughput_mbps} Mbps), ${latency} ms latency"
    done
    
    echo ""
    echo "Best results by latency:"
    awk -F',' '$9=="1" && NR>1 {print $0}' "$RESULTS_DIR/benchmark_results.csv" | sort -t',' -k7 -n | head -5 | while IFS=',' read -r test_id msg_size msg_count concurrency duration throughput throughput_mbps latency success bytes_sent bytes_received; do
        echo "  $test_id: ${latency} ms latency, ${throughput} msg/s"
    done
fi

echo ""
echo "Results saved to: $RESULTS_DIR/benchmark_results.csv"
echo "Detailed logs in: $RESULTS_DIR/"
echo "=== Benchmark Complete ==="
