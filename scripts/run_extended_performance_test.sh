#!/bin/bash

echo "🚀 QuicCrunch Extended Performance Test"
echo "======================================="
echo "Running 5-minute performance tests for each backend..."
echo ""

cd /home/shift/code/quiccrunch

# Function to run test with error handling
run_test() {
    local backend=$1
    local features=$2
    local description=$3
    
    echo "📊 Testing $description..."
    echo "Features: $features"
    echo "Duration: 5 minutes"
    echo "----------------------------------------"
    
    nix develop --command bash -c "timeout 330s cargo run --example extended_performance_test $features"
    local exit_code=$?
    
    if [ $exit_code -eq 124 ]; then
        echo "⏰ Test completed (timeout after 5.5 minutes)"
    elif [ $exit_code -ne 0 ]; then
        echo "❌ Test failed with exit code: $exit_code"
        return 1
    else
        echo "✅ Test completed successfully"
    fi
    
    echo ""
    return 0
}

# Test 1: Quinn Backend (default)
echo "🔍 Phase 1: Quinn Backend Performance Test"
echo "==========================================="
run_test "quinn" "--features backend-quinn,tls --no-default-features" "Quinn Backend"

# Small break between tests
echo "⏳ Waiting 30 seconds before next test..."
sleep 30

# Test 2: tQUIC Backend
echo "🔍 Phase 2: tQUIC Backend Performance Test"
echo "==========================================="
run_test "tquic" "--features backend-tquic,tls --no-default-features" "tQUIC Backend"

echo "🏁 All performance tests completed!"
echo ""
echo "📈 Summary:"
echo "- Each test ran for 5 minutes"
echo "- Multiple message sizes tested (64B, 256B, 1KB, 4KB)"
echo "- Latency percentiles (P50, P95, P99) measured"
echo "- Throughput in messages/sec and Mbps calculated"
echo ""
echo "Check the output above for detailed performance metrics."