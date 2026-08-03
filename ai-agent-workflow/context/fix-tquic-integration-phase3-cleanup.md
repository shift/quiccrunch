# Phase 3: Connection Cleanup - Context

## Objective
Ensure proper cleanup of all connection tracking state when connections close, preventing memory leaks and stale state.

## Current State Analysis

### Problems
1. `handle_close_connection()` doesn't clean up bidirectional mappings
2. `on_conn_closed()` handler can't access worker's connection maps
3. Stream queue entries not cleaned up on connection close
4. Worker stats not updated correctly

## Implementation Details

### 1. Fix handle_close_connection() (worker.rs:759-768)
```rust
fn handle_close_connection(&mut self, connection_id: ConnId, error_code: u64, reason: Vec<u8>) {
    // ✅ Get tQUIC connection ID before removing from map
    let tquic_conn_id = self.connections.get(&connection_id).copied();
    
    // Close at tQUIC level if connection exists
    if let Some(tquic_id) = tquic_conn_id {
        if let Some(ref mut endpoint) = self.endpoint {
            if let Some(conn) = endpoint.conn_get_mut(tquic_id) {
                let _ = conn.close(true, error_code, &reason);
            }
        }
        
        // ✅ Remove from BOTH directions of mapping
        self.reverse_connections.remove(&tquic_id);
    }
    
    // Remove from our connections map
    self.connections.remove(&connection_id);
    
    // ✅ Clean up stream queue for this connection
    if let Ok(mut queue) = self.stream_queue.lock() {
        queue.remove(&connection_id);
    }
    
    // ✅ Update stats
    self.stats.active_connections = self.connections.len();
}
```

### 2. Fix on_conn_closed() Handler (worker.rs:88-99)
```rust
fn on_conn_closed(&mut self, conn: &mut Connection) {
    let tquic_conn_id = conn.trace_id();
    
    // ✅ We need access to worker's connection maps
    // Option A: Add reverse_connections to WorkerTransportHandler
    // Option B: Emit event with tQUIC ID, let worker handle cleanup
    
    // Current approach: Emit event for monitoring
    if let Some(ref tx) = self.event_tx {
        let _ = tx.try_send(TquicEvent::ConnectionClosed {
            connection_id: 0,  // Will be resolved by worker
            error_code: 0,     // Extract from conn if possible
            reason: Vec::new(),
        });
    }
    
    // Note: Actual cleanup happens in handle_close_connection() 
    // when application initiates close, or we need a background
    // cleanup task for orphaned connections
}
```

### 3. Add WorkerTransportHandler Access to Reverse Mapping
```rust
// In WorkerTransportHandler struct (worker.rs:44-48)
struct WorkerTransportHandler {
    worker_id: usize,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
    pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,
    // ✅ Add for on_conn_closed cleanup
    reverse_connections: Arc<Mutex<HashMap<tquic::ConnectionId, ConnId>>>,
    connections: Arc<Mutex<HashMap<ConnId, tquic::ConnectionId>>>,
}
```

Then in `on_conn_closed()`:
```rust
fn on_conn_closed(&mut self, conn: &mut Connection) {
    let tquic_conn_id = conn.trace_id();
    
    // ✅ Clean up mappings immediately
    if let Ok(mut reverse) = self.reverse_connections.lock() {
        if let Some(our_id) = reverse.remove(&tquic_conn_id) {
            if let Ok(mut conns) = self.connections.lock() {
                conns.remove(&our_id);
            }
            if let Ok(mut queue) = self.stream_queue.lock() {
                queue.remove(&our_id);
            }
        }
    }
    
    // Emit event
    if let Some(ref tx) = self.event_tx {
        let _ = tx.try_send(TquicEvent::ConnectionClosed {
            connection_id: 0,
            error_code: 0,
            reason: Vec::new(),
        });
    }
}
```

### 4. Connection Stats Cleanup
Ensure stats are updated in all close paths:
```rust
// In handle_close_connection() - already done above
self.stats.active_connections = self.connections.len();

// In on_conn_closed() - if we clean up there too
self.stats.active_connections = self.connections.len();  // Need access to worker stats
```

## Testing Strategy

### Unit Tests
```rust
#[tokio::test]
async fn test_connection_close_cleans_mappings() {
    // Connect, verify mappings exist
    // Close connection
    // Verify both connections and reverse_connections are empty
    // Verify stream queue cleaned up
}

#[tokio::test]
async fn test_on_conn_closed_cleans_state() {
    // Simulate on_conn_closed event
    // Verify mappings cleaned up
    // Verify stats updated
}

#[tokio::test]
async fn test_close_idempotent() {
    // Close same connection twice
    // Should not panic or error
}
```

## Validation Checklist
- [ ] Closing connection removes from `connections` map
- [ ] Closing connection removes from `reverse_connections` map
- [ ] Closing connection removes from `stream_queue`
- [ ] `active_connections` stat stays accurate
- [ ] No memory leaks after many connect/close cycles
- [ ] `on_conn_closed` handler cleans up properly
- [ ] Double-close is safe

## Dependencies
- Phase 1 (connection mappings must exist)
- Phase 2 (stream queue keys use our ConnId)

## Estimated Effort
30 minutes focused development

---
*Context for Phase 3 of: Fix tQUIC Integration Gaps*
*Rust Implementation Context*
*Last updated: 2025-01-16*