#!/usr/bin/env bash
set -e

cd /home/shift/code/quiccrunch

echo "Building example..."
cargo build --example bidirectional_streaming --features backend-tquic 2>&1 | grep -E "(Compiling|Finished)" || true

echo ""
echo "=== Starting server in background ==="
./target/debug/examples/bidirectional_streaming server 2>&1 > /tmp/server.log &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"
sleep 2

echo ""
echo "=== Running client ==="
timeout 5 ./target/debug/examples/bidirectional_streaming client 2>&1 | head -50 || true

echo ""
echo "=== Stopping server ==="
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo ""
echo "=== Server log (last 30 lines) ==="
tail -30 /tmp/server.log || true
