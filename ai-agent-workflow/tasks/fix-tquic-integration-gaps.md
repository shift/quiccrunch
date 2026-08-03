# Task: Fix tQUIC Integration Gaps - Core Transport Functionality

## Objective
Fix critical tQUIC v1.6.0 integration gaps that prevent real data transmission. The architecture is complete but connection ID translation, server accept, and stream operations are broken, making the core transport non-functional.

## Acceptance Criteria
- [ ] **Phase 1**: Connection ID translation layer implemented (our ConnId ↔ tQUIC ConnectionId)
- [ ] **Phase 1**: Server `handle_accept()` implemented with pending connection queue
- [ ] **Phase 2**: Stream `send()` actually transmits data over QUIC
- [ ] **Phase 2**: Stream `receive()` returns real data from network
- [ ] **Phase 2**: Bidirectional stream open/accept works end-to-end
- [ ] **Phase 3**: Connection close cleans up all tracking state properly
- [ ] **Phase 4**: Integration test passes - real client/server data exchange
- [ ] **Phase 4**: Bidirectional streaming example works with real data
- [ ] **Phase 4**: Benchmark suite runs successfully with measurable throughput
- [ ] All existing tests pass (`cargo test --features backend-tquic`)
- [ ] No compiler warnings (`cargo clippy --all-targets --all-features -- -D warnings`)
- [ ] Code formatted (`cargo fmt --all`)

## Implementation Plan (Based on TQUIC_INTEGRATION_ANALYSIS.md)

### Phase 1: Connection Management Fix (2-3 hours)
1. Update `TquicWorker` data structures in `worker.rs`:
   - Add `connections: HashMap<ConnId, tquic::ConnectionId>` (our ID → tQUIC ID)
   - Add `reverse_connections: HashMap<tquic::ConnectionId, ConnId>` (tQUIC ID → our ID)
   - Add `pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>`
2. Fix `handle_connect()` to store bidirectional mapping
3. Implement `handle_accept()` using pending connection queue
4. Update `TransportHandler::on_conn_established()` to populate pending queue
5. Update `WorkerTransportHandler` to include pending queue reference

### Phase 2: Stream Operations Fix (1-2 hours)
1. Fix `handle_send()` - translate our ConnId to tQUIC ConnectionId before `conn_get_mut()`
2. Fix `handle_receive()` - same translation, handle `tquic::Error::Done` properly
3. Fix `handle_open_stream()` - translate ConnId, use `stream_bidi_new()`
4. Fix `handle_accept_stream()` - validate connection exists, use translated IDs
5. Fix `on_stream_created()` - store stream IDs with tQUIC connection ID (translate in accept)

### Phase 3: Connection Cleanup (30 minutes)
1. Fix `handle_close_connection()` - remove from both mapping directions
2. Fix `on_conn_closed()` - emit proper events with correct IDs
3. Clean up stream queue on connection close

### Phase 4: Testing & Validation (2-3 hours)
1. Add unit tests for connection ID translation in `test_worker_pool.rs`
2. Create integration test `tests/tquic_integration.rs` with real client/server
3. Run bidirectional_streaming example - verify real data flow
4. Run benchmark suite - verify measurable throughput

## Technical Details

### Files to Modify
- `quiccrunch-core/src/transport/worker.rs` - Core fixes (Phases 1-3)
- `quiccrunch-core/src/transport/test_worker_pool.rs` - Unit tests
- `quiccrunch-core/tests/tquic_integration.rs` - New integration test file

### Key tQUIC v1.6.0 APIs
```rust
// Connection ID types
let tquic_conn_id: tquic::ConnectionId = conn.trace_id();  // tQUIC's internal ID
endpoint.conn_get_mut(tquic_conn_id)  // Requires tQUIC ID

// Stream operations
conn.stream_write(stream_id, Bytes, fin) -> Result<usize>
conn.stream_read(stream_id, &mut buf) -> Result<(usize, bool)>
conn.stream_bidi_new(urgency, incremental) -> Result<u64>
conn.close(app, error_code, reason)
```

### Risk Assessment
- **Low**: Architecture is sound, only translation layer missing
- **Medium**: tQUIC v1.6.0 API documentation sparse, may need experimentation
- **High**: None - issues are implementation, not design

## Resources
- [TQUIC_INTEGRATION_ANALYSIS.md](../docs/TQUIC_INTEGRATION_ANALYSIS.md) - Detailed gap analysis
- `quiccrunch-core/src/transport/worker.rs` - Main implementation
- `quiccrunch-core/src/transport/worker_pool.rs` - Worker pool manager
- `quiccrunch-core/src/transport/adapter.rs` - Tokio async layer

## Rust-Specific Requirements
- **Memory Safety**: Proper `Arc<Mutex<>>` for shared state across worker threads
- **Performance**: Minimal overhead in ID translation (HashMap lookup)
- **Error Handling**: `thiserror` for transport errors, graceful degradation
- **Async**: All operations in worker thread via mpsc channels
- **Thread Safety**: `Send + Sync` bounds on all shared structures
- **Testing**: Integration tests with real QUIC connections
- **Linting**: `cargo clippy -- -D warnings` must pass

## Status
- **Created**: 2025-01-16
- **Priority**: P0 CRITICAL - Blocks all real streaming and benchmarking
- **Current Phase**: Planning
- **Assigned Agent**: Rust QUIC Transport Specialist
- **Blockers**: None
- **Next Steps**: Create context files and begin Phase 1 implementation

## Context References
- [Phase 1: Connection Management](../context/fix-tquic-integration-phase1-connection-management.md)
- [Phase 2: Stream Operations](../context/fix-tquic-integration-phase2-stream-operations.md)
- [Phase 3: Connection Cleanup](../context/fix-tquic-integration-phase3-cleanup.md)
- [Phase 4: Testing & Validation](../context/fix-tquic-integration-phase4-testing.md)

## Multi-Agent Workflow Status
- **Implementation Agent**: Not started
- **Testing Agent**: Not started
- **Documentation Agent**: Not started

---
*Task Definition for: Fix tQUIC Integration Gaps*
*Rust Implementation Task - Core Transport Functionality*
*Last updated: 2025-01-16*