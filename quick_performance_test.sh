#!/usr/bin/env bash

set -e

echo "=== Quick QUICCRUNCH Performance Test ==="
echo ""

RESULTS_DIR="./performance_results"
mkdir -p "$RESULTS_DIR"

# Simplified test configurations
MESSAGE_SIZES=(64 1024 4096)
MESSAGE_COUNTS=(50 100)
CONCURRENCY_LEVELS=(1 3)

# Function to run performance test
run_performance_test() {
    local message_size=$1
    local message_count=$2
    local concurrency=$3
    local test_id="${message_size}B_${message_count}msg_${concurrency}conc"
    
    echo "Running test: $test_id"
    
    # Start server
    timeout 30 ./target/release/examples/bidirectional_streaming server > "$RESULTS_DIR/server_${test_id}.log" 2>&1 &
    local server_pid=$!
    
    # Wait for server to start
    sleep 2
    
    # Check if server is running
    if ! kill -0 $server_pid 2>/dev/null; then
        echo "ERROR: Server failed to start for test $test_id"
        echo "0,0,0,0,0,0,0" >> "$RESULTS_DIR/performance_results.csv"
        return 1
    fi
    
    # Run client with performance measurement
    local start_time=$(date +%s)
    
    # Run multiple clients in parallel for concurrency
    local pids=()
    for ((i=1; i<=concurrency; i++)); do
        {
            timeout 20 ./target/release/examples/bidirectional_streaming client > "$RESULTS_DIR/client_${test_id}_${i}.log" 2>&1
            echo $? > "$RESULTS_DIR/client_${test_id}_${i}.exit"
        } &
        pids+=($!)
    done
    
    # Wait for all clients to complete or timeout
    local wait_time=0
    local max_wait=25
    while [ $wait_time -lt $max_wait ]; do
        local all_done=1
        for pid in "${pids[@]}"; do
            if kill -0 $pid 2>/dev/null; then
                all_done=0
                break
            fi
        done
        if [ $all_done -eq 1 ]; then
            break
        fi
        sleep 1
        wait_time=$((wait_time + 1))
    done
    
    # Kill any remaining client processes
    for pid in "${pids[@]}"; do
        kill $pid 2>/dev/null || true
    done
    
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    # Stop server
    kill $server_pid 2>/dev/null || true
    wait $server_pid 2>/dev/null || true
    
    # Calculate metrics
    local total_messages=$((message_count * concurrency))
    local throughput=0
    local avg_latency=0
    local success=0
    
    if [ $duration -gt 0 ]; then
        throughput=$((total_messages / duration))
        avg_latency=$((duration * 1000 / total_messages))
    fi
    
    # Check if all clients succeeded
    local all_success=1
    for ((i=1; i<=concurrency; i++)); do
        if [ -f "$RESULTS_DIR/client_${test_id}_${i}.exit" ]; then
            local exit_code=$(cat "$RESULTS_DIR/client_${test_id}_${i}.exit")
            if [ "$exit_code" != "0" ]; then
                all_success=0
                break
            fi
        else
            all_success=0
            break
        fi
    done
    
    if [ $all_success -eq 1 ] && [ $duration -gt 0 ]; then
        success=1
    fi
    
    # Save results
    echo "$test_id,$message_size,$message_count,$concurrency,$duration,$throughput,$avg_latency,$success" >> "$RESULTS_DIR/performance_results.csv"
    
    echo "Test $test_id: Duration=${duration}s, Throughput=${throughput} msg/s, Latency=${avg_latency}ms, Success=$success"
}

# Create CSV header
echo "TestID,MessageSize,MessageCount,Concurrency,Duration,Throughput,AvgLatency,Success" > "$RESULTS_DIR/performance_results.csv"

echo "Starting performance tests..."
echo ""

# Run tests with different configurations
for message_size in "${MESSAGE_SIZES[@]}"; do
    for message_count in "${MESSAGE_COUNTS[@]}"; do
        for concurrency in "${CONCURRENCY_LEVELS[@]}"; do
            run_performance_test $message_size $message_count $concurrency
            echo ""
        done
    done
done

echo "=== Performance Test Summary ==="
echo ""
echo "Results saved to: $RESULTS_DIR/performance_results.csv"
echo ""

# Calculate summary statistics
echo "=== Summary Statistics ==="
echo ""

successful_tests=$(awk -F',' '$8=="1" {count++} END {print count+0}' "$RESULTS_DIR/performance_results.csv")
total_tests=$(wc -l < "$RESULTS_DIR/performance_results.csv")
total_tests=$((total_tests - 1)) # Subtract header

echo "Total tests: $total_tests"
echo "Successful tests: $successful_tests"
echo "Success rate: $(echo "scale=1; $successful_tests * 100 / $total_tests" | bc -l)%"
echo ""

# Best and worst performance
if [ "$successful_tests" -gt 0 ]; then
    best_throughput=$(awk -F',' '$8=="1" && NR>1 {print $6}' "$RESULTS_DIR/performance_results.csv" | sort -nr | head -1)
    worst_latency=$(awk -F',' '$8=="1" && NR>1 {print $7}' "$RESULTS_DIR/performance_results.csv" | sort -n | head -1)
    best_latency=$(awk -F',' '$8=="1" && NR>1 {print $7}' "$RESULTS_DIR/performance_results.csv" | sort -nr | head -1)
    
    echo "Best throughput: ${best_throughput} msg/s"
    echo "Best latency: ${best_latency} ms"
    echo "Worst latency: ${worst_latency} ms"
fi

echo ""
echo "Detailed logs available in: $RESULTS_DIR/"
echo "=== Test Complete ==="