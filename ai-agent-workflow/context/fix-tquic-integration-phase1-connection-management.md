# Phase 1: Connection Management Fix - Context

## Objective
Implement the connection ID translation layer and server accept path to enable connection lifecycle management.

## Current State Analysis

### Problem: Connection ID Space Mismatch
```rust
// In worker.rs:26-42 - Current TquicWorker struct
connections: HashMap<ConnId, ConnId>  // WRONG: Should map to tQUIC ConnectionId

// In handle_connect() - worker.rs:636-642
Ok(conn) => {
    self.connections.insert(conn_id, conn);  // Type mismatch! conn is tquic::Connection
}
```

### Root Cause
1. tQUIC `Connection` type is **NOT `Send` or `Sync`** - cannot store in HashMap across threads
2. We generate our own `ConnId` (sequential u64), but tQUIC has internal `ConnectionId`
3. `endpoint.conn_get_mut()` requires tQUIC's `ConnectionId`, not our wrapper ID
4. No bidirectional mapping between the two ID spaces

### Required Data Structures
```rust
pub struct TquicWorker {
    // ... existing fields ...
    
    // FIX: Bidirectional connection ID mapping
    connections: HashMap<ConnId, tquic::ConnectionId>,           // Our ID → tQUIC ID
    reverse_connections: HashMap<tquic::ConnectionId, ConnId>,   // tQUIC ID → Our ID
    
    // FIX: Pending connection queue for server accept
    pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,
}
```

## Implementation Details

### 1. Update TquicWorker Struct (worker.rs:26-42)
```rust
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tquic::ConnectionId;

pub struct TquicWorker {
    // ... existing fields ...
    
    // Connection ID translation layer
    connections: HashMap<ConnId, ConnectionId>,
    reverse_connections: HashMap<ConnectionId, ConnId>,
    
    // Server accept support
    pending_connections: Arc<Mutex<VecDeque<(ConnectionId, SocketAddr)>>>,
}
```

### 2. Fix handle_connect() (worker.rs:623-647)
```rust
fn handle_connect(&mut self, remote_addr: SocketAddr) -> Result<ConnId> {
    if let Some(ref mut endpoint) = self.endpoint {
        let our_conn_id = self.next_connection_id;
        self.next_connection_id += 1;

        match endpoint.connect(
            self.config.bind_addr,
            remote_addr,
            None, None, None, None,
        ) {
            Ok(tquic_conn_id) => {  // Returns ConnectionId, not Connection!
                // Store bidirectional mapping
                self.connections.insert(our_conn_id, tquic_conn_id);
                self.reverse_connections.insert(tquic_conn_id, our_conn_id);
                
                self.stats.total_connections += 1;
                self.stats.active_connections = self.connections.len();
                
                Ok(our_conn_id)
            }
            Err(e) => Err(TransportError::Other(format!("Connection failed: {:?}", e))),
        }
    } else {
        Err(TransportError::Other("Endpoint not initialized".into()))
    }
}
```

### 3. Implement handle_accept() (worker.rs:696-700)
```rust
fn handle_accept(&mut self) -> Result<(ConnId, SocketAddr)> {
    if let Ok(mut queue) = self.pending_connections.lock() {
        if let Some((tquic_conn_id, remote_addr)) = queue.pop_front() {
            let our_conn_id = self.next_connection_id;
            self.next_connection_id += 1;
            
            // Store bidirectional mapping
            self.connections.insert(our_conn_id, tquic_conn_id);
            self.reverse_connections.insert(tquic_conn_id, our_conn_id);
            
            self.stats.total_connections += 1;
            self.stats.active_connections = self.connections.len();
            
            return Ok((our_conn_id, remote_addr));
        }
    }
    
    // No pending connections - return WouldBlock to try again later
    Err(TransportError::WouldBlock)
}
```

### 4. Update TransportHandler::on_conn_established() (worker.rs:72-86)
```rust
fn on_conn_established(&mut self, conn: &mut Connection) {
    let tquic_conn_id = conn.trace_id();
    
    // Get peer address - need to investigate tQUIC v1.6.0 API
    // For now, extract from connection if possible
    let remote_addr = extract_peer_address(conn)  // Implement this helper
        .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
    
    // Add to pending connections queue
    if let Ok(mut queue) = self.pending_connections.lock() {
        queue.push_back((tquic_conn_id, remote_addr));
    }
    
    // Emit event for monitoring
    if let Some(ref tx) = self.event_tx {
        let _ = tx.try_send(TquicEvent::ConnectionEstablished {
            connection_id: 0,  // Will be assigned in handle_accept()
            remote_addr,
        });
    }
}

// Helper to extract peer address from tQUIC connection
fn extract_peer_address(conn: &Connection) -> Option<SocketAddr> {
    // tQUIC v1.6.0 - try conn.paths_iter().next().map(|p| p.remote)
    conn.paths_iter().next().map(|p| p.remote)
}
```

### 5. Update WorkerTransportHandler (worker.rs:44-48)
```rust
struct WorkerTransportHandler {
    worker_id: usize,
    event_tx: Option<mpsc::Sender<TquicEvent>>,
    stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
    pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,
}
```

## Testing Strategy

### Unit Tests (add to test_worker_pool.rs)
```rust
#[tokio::test]
async fn test_connection_id_translation() {
    // Create worker, connect, verify bidirectional mapping
    // Check connections.get(our_id) == Some(tquic_id)
    // Check reverse_connections.get(tquic_id) == Some(our_id)
}

#[tokio::test]
async fn test_server_accept_creates_mapping() {
    // Start server worker
    // Simulate on_conn_established
    // Call handle_accept()
    // Verify connection added to both maps
}
```

## Validation Checklist
- [ ] `cargo build --features backend-tquic` compiles without errors
- [ ] Connection ID translation works bidirectionally
- [ ] Server can accept incoming connections
- [ ] Worker stats track connections correctly
- [ ] No memory leaks in connection tracking

## Dependencies
- tQUIC v1.6.0 API for `ConnectionId` and peer address extraction
- Existing worker pool infrastructure (already complete)

## Estimated Effort
2-3 hours focused development

---
*Context for Phase 1 of: Fix tQUIC Integration Gaps*
*Rust Implementation Context*
*Last updated: 2025-01-16*