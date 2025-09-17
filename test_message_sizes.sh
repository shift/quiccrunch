#!/bin/bash

set -euo pipefail

echo "QuicCrunch Message Size Performance Test"
echo "======================================="

# Message sizes to test (in bytes)
SIZES=(512 1024 2048 4096 8192 16384 32768 65536 131072 262144 524288 1048576 2097152)
DURATION=10
CONCURRENCY=100

echo "Test configuration:"
echo "  Duration: ${DURATION}s per test"
echo "  Concurrency: $CONCURRENCY (actual load testing)"
echo "  Message sizes: 0.5K, 1K, 2K, 4K, 8K, 16K, 32K, 64K, 128K, 256K, 512K, 1MB, 2MB"
echo ""

# Clear previous results
> message_size_results.json

echo "[" >> message_size_results.json

FIRST=true
for SIZE in "${SIZES[@]}"; do
    SIZE_KB=$((SIZE / 1024))
    if [ $SIZE_KB -eq 0 ]; then
        SIZE_DISPLAY="${SIZE}B"
    elif [ $SIZE_KB -lt 1024 ]; then
        SIZE_DISPLAY="${SIZE_KB}K"
    else
        SIZE_MB=$((SIZE_KB / 1024))
        SIZE_DISPLAY="${SIZE_MB}MB"
    fi
    
    echo "Testing message size: $SIZE_DISPLAY ($SIZE bytes)"
    
    if [ "$FIRST" = false ]; then
        echo "," >> message_size_results.json
    fi
    FIRST=false
    
    RESULT=$(nix develop -c ./target/release/benchmark client flood \
        --concurrency $CONCURRENCY \
        --duration $DURATION \
        --message-size $SIZE 2>/dev/null | tail -n1)
    echo "$RESULT" >> message_size_results.json
        
    echo "  Completed $SIZE_DISPLAY"
done

echo "" >> message_size_results.json
echo "]" >> message_size_results.json

echo ""
echo "All tests completed. Results saved to message_size_results.json"
echo ""
echo "Summary:"
cat message_size_results.json | jq -r '.[] | "Size: \(.configuration.message_size) bytes, Throughput: \(.metrics.throughput_mbps | floor) Mbps, Messages/sec: \(.metrics.messages_per_second | floor)"'