#!/usr/bin/env bash

set -e

echo "Building example..."
cargo build --example bidirectional_streaming

echo ""
echo "Testing compilation: SUCCESS ✅"
echo ""
echo "To test the bidirectional streaming example:"
echo ""
echo "Terminal 1 (server):"
echo "  cargo run --example bidirectional_streaming -- server"
echo ""
echo "Terminal 2 (client):"
echo "  cargo run --example bidirectional_streaming -- client"
echo ""
echo "The client will send 5 messages and receive echoed responses."
