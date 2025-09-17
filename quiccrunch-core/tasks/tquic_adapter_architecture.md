# Task: Multi-threaded tQUIC Adapter Architecture

## Task Overview

Implement a multi-threaded adapter architecture to make tQUIC v1.6.0 compatible with tokio async/await while achieving high performance through parallelism.

## Current Status

**Status**: pending

**What's Done**: 
- Architecture design completed
- Task documentation framework established  
- AGENTS.md created for future AI sessions

**In Progress**: 
- Creating detailed task documentation

**What's Remaining**:
- All implementation tasks (worker pool, workers, adapter interface, socket reuse, config updates)

## Context

### Problem Background
- tQUIC v1.6.0 uses `Rc<RefCell<T>>` internally, making it not `Send`  
- This creates fundamental incompatibility with tokio's multi-threaded async runtime
- 58+ compilation errors due to thread safety violations
- Direct async bridge approach failed due to these constraints

### Architectural Decision
**Multi-threaded Adapter Pattern**: Use configurable number of tQUIC worker threads (each with mio::Poll + tQUIC::Endpoint) that share sockets via SO_REUSEPORT, with a tokio async adapter layer on top.

This design provides:
- ✅ True multithreading performance
- ✅ Socket reuse efficiency  
- ✅ Tokio async ergonomics
- ✅ tQUIC v1.6.0 compatibility

## Technical Requirements

### Core Components

1. **TquicWorkerPool**
   - Manages N worker threads
   - Load balancing between workers
   - Graceful shutdown handling
   - Worker health monitoring

2. **TquicWorker** 
   - Individual worker thread with mio::Poll + tQUIC::Endpoint
   - UDP socket with SO_REUSEPORT 
   - Message queue for requests from tokio
   - Response channels back to tokio

3. **TquicAdapter**
   - Async tokio interface that delegates to workers
   - Routes requests to appropriate workers
   - Provides QuicListener/QuicClient/QuicStream APIs

4. **Socket Management**
   - SO_REUSEPORT for efficient socket sharing on Linux
   - Fallback to shared socket + packet distribution for other platforms

### Threading Model

```
Tokio Runtime (async/await)
    ↓ (channels: mpsc)
Worker Thread Pool (mio + tQUIC)
├── Thread 1: mio::Poll + tQUIC Endpoint + UDP Socket (port reuse)
├── Thread 2: mio::Poll + tQUIC Endpoint + UDP Socket (port reuse)  
└── Thread N: mio::Poll + tQUIC Endpoint + UDP Socket (port reuse)
```

### Message Types

- **Connect**(remote_addr, response_channel)
- **Send**(connection_id, data, response_channel)  
- **Accept**(response_channel)
- **Close**(connection_id)

### Load Balancing Strategy

- Round-robin for new connections
- Connection affinity (existing connections stick to same worker)
- Consider least-loaded balancing for future optimization

## Dependencies

**Must Complete First**: None (this is the foundational task)

**Blocks**: All other tQUIC integration work

## Implementation Plan

### Phase 1: Core Worker Infrastructure
1. Create `TquicWorker` struct with mio event loop
2. Implement message passing between tokio and workers  
3. Basic connection lifecycle management

### Phase 2: Worker Pool Management
1. Create `TquicWorkerPool` for managing multiple workers
2. Implement load balancing logic
3. Add worker health monitoring and restart capabilities

### Phase 3: Socket Sharing
1. Implement SO_REUSEPORT socket sharing for Linux
2. Add fallback mechanism for other platforms
3. Test socket distribution efficiency

### Phase 4: Async Adapter Interface  
1. Create `TquicAdapter` with async APIs
2. Implement QuicListener/QuicClient/QuicStream wrappers
3. Error propagation and timeout handling

### Phase 5: Configuration & Integration
1. Add worker count configuration to config.rs
2. Update transport/tquic.rs to use new adapter
3. Replace existing async_bridge usage

## Files to Modify

### New Files
- `src/transport/worker_pool.rs` - TquicWorkerPool implementation
- `src/transport/worker.rs` - Individual TquicWorker implementation  
- `src/transport/adapter.rs` - TquicAdapter async interface
- `src/transport/messages.rs` - Message types for worker communication

### Modified Files
- `src/transport/tquic.rs` - Export new adapter instead of broken bridge
- `src/config.rs` - Add worker thread configuration
- `src/transport/mod.rs` - Update module exports
- `src/server.rs` - Use new adapter APIs
- `src/client.rs` - Use new adapter APIs

## Testing Strategy

### Unit Tests
- Worker message handling correctness
- Load balancing algorithm verification  
- Socket sharing functionality
- Error propagation through adapter

### Integration Tests
- End-to-end connection establishment
- Multiple concurrent connections
- Worker failure and recovery
- Performance under load

### Benchmarks
- Compare single-threaded vs multi-threaded performance
- Socket reuse efficiency measurement
- Latency impact of message passing overhead

## Notes

### Key Technical Challenges

1. **Connection Affinity**: tQUIC connections tied to specific endpoints
   - Solution: Hash connection IDs to workers or maintain connection→worker mapping

2. **Error Handling**: Worker thread panics shouldn't crash system
   - Solution: Supervisor pattern with worker restart capability

3. **Graceful Shutdown**: Drain in-flight requests cleanly
   - Solution: Shutdown signaling + request completion awaiting

### Performance Considerations

- Channel overhead vs threading benefits
- Memory usage of multiple tQUIC endpoints
- Connection distribution efficiency
- Worker count optimization (probably CPU cores * 2)

### Platform Compatibility

- SO_REUSEPORT available on Linux/macOS but not Windows
- Need fallback strategy for Windows (shared socket + packet forwarding)
- Consider async-std compatibility for broader ecosystem support

### Future Optimizations

- Zero-copy packet forwarding where possible
- NUMA-aware worker pinning
- Dynamic worker scaling based on load
- Connection migration between workers for load balancing