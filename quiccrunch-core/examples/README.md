# QuicCrunch Examples

## Bidirectional Streaming Example

The `bidirectional_streaming.rs` example demonstrates the QuicCrunch API for bidirectional QUIC streaming between a client and server.

### Current Status: Mock Mode

⚠️ **This example runs in mock mode** - the API structure is correct, but the underlying tQUIC worker pool implementation uses stubs that return empty data.

### What Works

- ✅ API structure and usage patterns are demonstrated correctly
- ✅ Client and server can be started without errors
- ✅ Connection establishment API works
- ✅ Stream opening/accepting API works  
- ✅ Send/receive method signatures are correct

### What Doesn't Work Yet

- ❌ Actual data transmission (stubs return empty `Vec`)
- ❌ Real QUIC protocol communication
- ❌ Network I/O between client and server

### Running the Example

Start the server:
```bash
cargo run --example bidirectional_streaming -- server
```

Run the client:
```bash
cargo run --example bidirectional_streaming -- client
```

Both will execute successfully and demonstrate the API flow, but no actual data will be transmitted.

### Implementation Status

The worker pool in `src/transport/worker_pool.rs` currently has stub implementations:

- `TquicCommand::Send` → Returns `Ok(())` without sending data (line 530)
- `TquicCommand::Receive` → Returns empty `Vec::new()` (line 545)
- Other commands return mock IDs

A complete `TquicWorker` implementation exists in `src/transport/worker.rs` using real tQUIC endpoints, but the worker pool doesn't instantiate or use it yet.

### To Enable Real Communication

Connect the worker pool to the real TquicWorker:

1. Modify `TquicWorkerPool::new()` to create actual `TquicWorker` instances
2. Replace stub command handlers with calls to the real worker's tQUIC endpoint
3. Wire up the event channels between workers and the adapter layer
4. Implement proper stream event forwarding from tQUIC callbacks

The API is ready - only the backend wiring needs completion.
