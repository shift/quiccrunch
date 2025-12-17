#!/usr/bin/env bash
set -e

echo "Testing TQUIC mio→tokio bridge implementation"
echo "=============================================="

cd /home/shift/code/quiccrunch

echo ""
echo "Step 1: Building simple_tquic_test example..."
nix develop -c -- cargo build --example simple_tquic_test 2>&1 | grep -v "^warning" | tail -3

echo ""
echo "Step 2: Starting test server in background..."
nix develop -c -- cargo run --example simple_tquic_test > /tmp/tquic_server.log 2>&1 &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

sleep 2

echo ""
echo "Step 3: Checking if server is listening..."
if ! lsof -i :4433 > /dev/null 2>&1; then
    echo "ERROR: Server not listening on port 4433"
    kill $SERVER_PID 2>/dev/null || true
    exit 1
fi
echo "✓ Server listening on port 4433"

echo ""
echo "Step 4: Running test client..."
echo "Test client connecting to 127.0.0.1:4433" | nix develop -c -- cargo run --example simple_tquic_test -- client 127.0.0.1:4433 2>&1 | grep -v "^warning" || true

echo ""
echo "Step 5: Cleaning up..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo ""
echo "Step 6: Checking server logs..."
if [ -f /tmp/tquic_server.log ]; then
    echo "Server log output:"
    tail -20 /tmp/tquic_server.log
else
    echo "No server log found"
fi

echo ""
echo "=============================================="
echo "Test complete!"
