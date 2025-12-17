#!/usr/bin/env nix develop -c -- cargo run --bin demo

# Quick start script for gRPC benchmark demo
# This script runs a comprehensive demonstration of the gRPC benchmark capabilities

echo "🚀 Starting gRPC Benchmark Demo for QUICCRUNCH Comparison"
echo "======================================================"

# Run comprehensive benchmark
cargo run --bin demo -- --test-type all --messages 1000 --message-size 1024

echo ""
echo "📊 To run individual tests:"
echo "  Unary:           cargo run --bin demo -- --test-type unary --messages 1000 --message-size 1024"
echo "  Client Streaming: cargo run --bin demo -- --test-type client-streaming --messages 1000 --message-size 1024"
echo "  Server Streaming: cargo run --bin demo -- --test-type server-streaming --messages 1000 --message-size 1024"
echo "  Bidirectional:   cargo run --bin demo -- --test-type bidirectional --messages 1000 --message-size 1024"
echo ""
echo "📈 To test different configurations:"
echo "  High throughput:  cargo run --bin demo -- --test-type all --messages 10000 --message-size 4096"
echo "  Low latency:     cargo run --bin demo -- --test-type all --messages 5000 --message-size 64"
echo ""
echo "🔧 For production benchmarking, use the full gRPC implementation once proto compilation is set up."