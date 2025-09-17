# tQUIC RPC Examples

This directory contains examples demonstrating all major RPC patterns using tQUIC and Cap'n Proto:

- **client_server_unary.rs**: Client→Server unary RPC. Client sends a request, server replies with a response.
- **server_client_stream.rs**: Server→Client streaming RPC. Client requests a stream, server sends multiple chunks.
- **bidistreaming.rs**: Bidirectional streaming RPC. Both client and server send/receive stream chunks.

All examples use Cap'n Proto for message serialization/deserialization and tQUIC for transport.

## Running Examples

Each example is a standalone Tokio async binary. Run with:

```sh
cargo run --example client_server_unary
cargo run --example server_client_stream
cargo run --example bidistreaming
```

## Requirements
- tQUIC transport enabled (default)
- Cap'n Proto schemas built (via build.rs)

## How It Works
- Each example sets up a tQUIC client and server.
- Messages are encoded/decoded using Cap'n Proto.
- Streams are used for sending/receiving data according to the pattern.

See the source files for details and usage.
