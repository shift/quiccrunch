#!/bin/bash

echo "QuicCrunch Performance Comparison"
echo "=================================="

cd /home/shift/code/quiccrunch

echo "1. Testing Quinn Backend..."
echo "----------------------------"
nix develop --command bash -c "cargo run --example complete_comparison --no-default-features --features backend-quinn,tls" 2>/dev/null | grep -E "(✅|❌|Average latency|Throughput)"

echo ""
echo "2. Testing tQUIC Backend..."
echo "----------------------------"  
nix develop --command bash -c "cargo run --example complete_comparison --no-default-features --features backend-tquic,tls" 2>/dev/null | grep -E "(✅|❌|Average latency|Throughput)"

echo ""
echo "Performance Comparison Complete!"