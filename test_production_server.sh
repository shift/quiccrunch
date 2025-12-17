#!/usr/bin/env bash

set -e

echo "=== QuicCrunch Server & Client Test ==="
echo ""

# Build binaries
echo "[1/4] Building server and client..."
cd "$(dirname "$0")"

if command -v nix &> /dev/null; then
    echo "Using Nix devShell for build..."
    nix develop -c bash -c "cd quiccrunch-core && cargo build --bin quic_server --bin quic_client" > /tmp/quic_build.log 2>&1
else
    echo "Building with cargo (requires cmake, perl, go)..."
    cd quiccrunch-core && cargo build --bin quic_server --bin quic_client > /tmp/quic_build.log 2>&1
    cd ..
fi

if [ ! -f target/debug/quic_server ]; then
    echo "Error: Build failed. Check /tmp/quic_build.log"
    exit 1
fi

echo "Build successful"
echo ""

# Start server
echo "[2/4] Starting server..."
if command -v nix &> /dev/null; then
    nix develop -c bash -c "RUST_LOG=info ./target/debug/quic_server 127.0.0.1:14433" > /tmp/quic_server.log 2>&1 &
else
    RUST_LOG=info ./target/debug/quic_server 127.0.0.1:14433 > /tmp/quic_server.log 2>&1 &
fi
SERVER_PID=$!

# Wait for server to start
sleep 2

# Check if server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "Error: Server failed to start. Check /tmp/quic_server.log"
    exit 1
fi

echo "Server started (PID: $SERVER_PID)"
echo ""

# Run client
echo "[3/4] Running client..."
if command -v nix &> /dev/null; then
    nix develop -c bash -c "RUST_LOG=info ./target/debug/quic_client 127.0.0.1:14433 5" > /tmp/quic_client.log 2>&1
    CLIENT_EXIT=$?
else
    RUST_LOG=info ./target/debug/quic_client 127.0.0.1:14433 5 > /tmp/quic_client.log 2>&1
    CLIENT_EXIT=$?
fi

echo ""

# Show client output
cat /tmp/quic_client.log

echo ""

# Stop server
echo "[4/4] Stopping server..."
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true
echo "Server stopped"
echo ""

# Check results
if [ $CLIENT_EXIT -eq 0 ]; then
    echo "=== TEST PASSED ==="
    echo ""
    echo "Server and client binaries are working correctly."
    echo "See SERVER_README.md for usage instructions."
    echo ""
    echo "Logs saved to:"
    echo "  - /tmp/quic_server.log"
    echo "  - /tmp/quic_client.log"
    echo "  - /tmp/quic_build.log"
else
    echo "=== TEST FAILED ==="
    echo ""
    echo "Client exited with code: $CLIENT_EXIT"
    echo "Check logs for details:"
    echo "  - /tmp/quic_server.log"
    echo "  - /tmp/quic_client.log"
    exit 1
fi
