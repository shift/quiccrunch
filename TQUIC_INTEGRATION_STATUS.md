# TQUIC Integration Status

## Current State (Oct 25, 2025)

### ✅ Working Components

1. **Low-level TquicAdapter** (`adapter.rs:169-310`)
   - Synchronous mio-based event loop
   - Proper implementation of TQUIC's `TransportHandler` and `PacketSendHandler` traits
   - Successfully establishes QUIC connections (verified with `simple_tquic_test` example)
   - Handles connection lifecycle events via internal channel

2. **Core TQUIC Integration**
   - Correct API usage: `conn.paths_iter()`, `FourTuple.remote`, public fields on connection/stream
   - BBR congestion control configured
   - TLS support for both client and server

### ⚠️ Broken/Incomplete Components

1. **Async Wrapper APIs** (`adapter.rs:342-551`)
   - `TquicClient::new()` - stub that returns error
   - `TquicServer::new()` - stub that returns error
   - `TquicListener::accept()` - stub that returns error
   - `TquicConnection::*` - stubs that return errors
   - `TquicStream::*` - stubs that return errors
   
   **Root Cause**: These were designed for async/tokio but TQUIC requires synchronous mio event loop

2. **Worker Architecture** (`worker.rs`, `worker_pool.rs`)
   - `TquicWorker` has its own mio event loop (duplicated effort)
   - `TquicWorkerPool` uses async command channels (architectural mismatch)
   - Complex state management with trace IDs and connection mappings
   
   **Root Cause**: Built on assumption that TQUIC could be wrapped in async, which is incorrect

3. **Examples**
   - `bidirectional_streaming.rs` uses old async API (broken)
   - Most examples expect `TquicClient`/`TquicServer` to work

### 🎯 The Core Problem

TQUIC uses a **synchronous, mio-based event loop model**:
```rust
loop {
    endpoint.process_connections();  // Process pending work
    poll.poll(&mut events, timeout);  // Wait for I/O
    for event in events {
        // Handle socket events
    }
    endpoint.on_timeout(Instant::now());  // Handle timeouts
}
```

The original code tried to wrap this in async/tokio with channels, creating fundamental mismatches:
- Can't `await` inside mio event loop
- Can't do blocking operations in tokio tasks
- Connection state split between mio thread and tokio tasks

### 📋 Recommended Path Forward

#### Option 1: Pure Synchronous API (Simplest)
Expose TquicAdapter directly. Users run mio event loop themselves:
```rust
let mut adapter = TquicAdapter::new(config)?;
let conn_index = adapter.connect(addr, "localhost")?;
adapter.run_event_loop()?;  // Blocking
```

**Pros**: 
- Minimal code, matches TQUIC's design
- No architectural impedance mismatch
- Already working (verified)

**Cons**:
- Not compatible with tokio-based applications
- Breaks existing examples
- Less ergonomic than async

#### Option 2: Proper Mio→Tokio Bridge (Complex but Idiomatic)
Run mio loop in dedicated thread, expose async API:
```rust
// Internal: mio thread runs TquicAdapter event loop
// Public: async API with channels for commands/events

let client = TquicClient::new().await?;  // Spawns mio thread
let conn = client.connect("addr").await?;  // Sends command via channel
conn.send_stream(data).await?;  // Async interface
```

**Implementation**:
1. Create `AdapterCommand` enum (Connect, OpenStream, Send, Receive, Close)
2. Spawn thread running `TquicAdapter::run_event_loop()`
3. Use tokio channels for commands and `tokio::sync::Notify` for wakeup
4. Map TQUIC events to async notifications

**Pros**:
- Compatible with tokio ecosystem
- Ergonomic async API
- Can reuse existing examples

**Cons**:
- Complex: thread synchronization, channel coordination
- Performance overhead from channel crossing
- Harder to debug (split between thread and tasks)

#### Option 3: Hybrid (Pragmatic)
Expose both APIs:
- `TquicAdapter` for performance-critical paths (synchronous)
- `TquicClient`/`TquicServer` for convenience (async wrapper)

### 🔧 Immediate Next Steps

1. **Decision Point**: Choose Option 1, 2, or 3 based on project requirements
2. **Clean Up**: Remove or mark broken async APIs as `#[deprecated]`
3. **Update Examples**: 
   - Keep `simple_tquic_test.rs` as synchronous reference
   - Update `bidirectional_streaming.rs` based on chosen approach
4. **Fix Worker Pool**: Either rewrite to use chosen approach or remove if redundant
5. **Documentation**: Update README with correct API usage

### 📝 Files Requiring Changes

| File | Status | Action Required |
|------|--------|----------------|
| `adapter.rs:342-551` | Broken | Rewrite async API (Option 2/3) or remove (Option 1) |
| `worker.rs` | Redundant | Remove or integrate with TquicAdapter |
| `worker_pool.rs` | Broken | Rewrite or remove |
| `examples/bidirectional_streaming.rs` | Broken | Update to working API |
| `examples/simple_tquic_test.rs` | Working | Keep as reference |

### 🧪 Testing

**What Works**: 
- Connection establishment (client→server)
- TLS handshake
- Event callbacks

**Not Tested Yet**:
- Stream creation
- Data send/receive
- Connection close
- Multiple connections
- Error handling

### 💡 Key Insights

1. TQUIC is fundamentally synchronous - don't fight it
2. The `Endpoint` must be called from the same thread (no `Send`)
3. All I/O happens via callbacks, not direct API calls
4. Connection/stream IDs are internal indices, not user-facing handles

---

**Last Updated**: Oct 25, 2025 by OpenCode AI
**Compilation Status**: ✅ Success (with warnings)
**Runtime Status**: ✅ Basic connectivity works
