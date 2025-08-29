#!/bin/bash
# Quick performance comparison between baseline and optimized QuicCrunch

set -euo pipefail

echo "🚀 QuicCrunch Performance Comparison Test"
echo "========================================"

# Test parameters
BACKEND="tquic"
CONCURRENCY=1
REQUESTS=1000
MESSAGE_SIZE=1024

echo "📊 Test Configuration:"
echo "  Backend: $BACKEND"
echo "  Concurrency: $CONCURRENCY"
echo "  Requests: $REQUESTS"
echo "  Message Size: $MESSAGE_SIZE bytes"
echo ""

# Run baseline test (no optimizations)
echo "🔥 Running BASELINE test (no optimizations)..."
echo "Features: backend-$BACKEND"
BASELINE_START=$(date +%s%N)
nix develop -c cargo run --release -p quiccrunch-core --bin benchmark \
  --features="backend-$BACKEND" \
  -- --backend $BACKEND --concurrency $CONCURRENCY --requests $REQUESTS --message-size $MESSAGE_SIZE \
  > baseline_results.txt 2>&1
BASELINE_END=$(date +%s%N)
BASELINE_TIME=$((($BASELINE_END - $BASELINE_START) / 1000000))

echo "✅ Baseline test completed in ${BASELINE_TIME}ms"
echo ""

# Run optimized test (all safe optimizations)
echo "⚡ Running OPTIMIZED test (lock-free channels + memory pools + SIMD + adaptive)..."
echo "Features: backend-$BACKEND,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations"
OPTIMIZED_START=$(date +%s%N)
nix develop -c cargo run --release -p quiccrunch-core --bin benchmark \
  --features="backend-$BACKEND,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations" \
  -- --backend $BACKEND --concurrency $CONCURRENCY --requests $REQUESTS --message-size $MESSAGE_SIZE \
  > optimized_results.txt 2>&1
OPTIMIZED_END=$(date +%s%N)
OPTIMIZED_TIME=$((($OPTIMIZED_END - $OPTIMIZED_START) / 1000000))

echo "✅ Optimized test completed in ${OPTIMIZED_TIME}ms"
echo ""

# Calculate improvement
echo "📈 PERFORMANCE RESULTS:"
echo "======================"
echo "Baseline Time:  ${BASELINE_TIME}ms"
echo "Optimized Time: ${OPTIMIZED_TIME}ms"

if [ $OPTIMIZED_TIME -lt $BASELINE_TIME ]; then
    IMPROVEMENT=$(echo "scale=2; $BASELINE_TIME / $OPTIMIZED_TIME" | bc)
    echo "🎉 IMPROVEMENT: ${IMPROVEMENT}x faster!"
else
    REGRESSION=$(echo "scale=2; $OPTIMIZED_TIME / $BASELINE_TIME" | bc)
    echo "⚠️  REGRESSION: ${REGRESSION}x slower"
fi

echo ""
echo "📋 Detailed Results:"
echo "==================="
echo ""
echo "BASELINE OUTPUT:"
cat baseline_results.txt
echo ""
echo "OPTIMIZED OUTPUT:"
cat optimized_results.txt

echo ""
echo "✨ Performance test completed!"