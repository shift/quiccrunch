#!/usr/bin/env bash
set -e

PORT=8888
ADDR="127.0.0.1:$PORT"

echo "Starting server on $ADDR..."
RUST_LOG=quiccrunch_core::transport::worker=debug ./target/release/quic_server $ADDR 1 > server.log 2>&1 &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

sleep 2

echo "Starting client..."
RUST_LOG=quiccrunch_core::transport=debug ./target/release/quic_client $ADDR 2 1 > client.log 2>&1

echo "Client finished"
sleep 1

echo "Stopping server..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo ""
echo "===== CLIENT LOG ====="
cat client.log | grep -E "(INFO|WARN|Buffered|readable)"

echo ""
echo "===== SERVER LOG ====="
cat server.log | grep -E "(INFO|WARN|DEBUG.*Buffered|DEBUG.*readable|DEBUG.*received)"

echo ""
echo "===== Test Summary ====="
if grep -q "Bytes received: 0" client.log; then
    echo "❌ FAILED: Client received 0 bytes"
    exit 1
else
    echo "✅ SUCCESS: Client received data"
    exit 0
fi
