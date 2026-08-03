# Phase 2: Stream Operations Fix - Context

## Objective
Fix stream send/receive/open/accept operations to use correct tQUIC connection IDs and actually transmit data over QUIC.

## Current State Analysis

### Problem: Wrong Connection ID in Stream Operations
All stream operations in `worker.rs` use our custom `ConnId` but call `endpoint.conn_get_mut(our_id)` which fails because tQUIC only knows its own `ConnectionId`.

```rust
// CURRENT BROKEN CODE - worker.rs:649-669
fn handle_send(...) -> Result<()> {
    if let Some(ref mut endpoint) = self.endpoint {
        if let Some(conn) = endpoint.conn_get_mut(connection_id) {  // ❌ Wrong ID type!
            // ... never executes
        }
    }
}
```

### Required Fix Pattern
Every stream operation must:
1. Look up our `ConnId` → tQUIC `ConnectionId` in `self.connections`
2. Use tQUIC ID for `endpoint.conn_get_mut()`
3. Perform operation on retrieved connection

## Implementation Details

### 1. Fix handle_send() (worker.rs:649-669)
```rust
fn handle_send(
    &mut self,
    connection_id: ConnId,
    stream_id: u64,
    data: Vec<u8>,
    fin: bool,
) -> Result<()> {
    if let Some(ref mut endpoint) = self.endpoint {
        // ✅ Translate our connection ID to tQUIC's ID
        let tquic_conn_id = self.connections.get(&connection_id)
            .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;
        
        if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
            let bytes_data = Bytes::from(data);
            match conn.stream_write(stream_id, bytes_data, fin) {
                Ok(written) => {
                    self.stats.bytes_sent += written as u64;
                    Ok(())
                }
                Err(e) => Err(TransportError::Other(format!("Send failed: {:?}", e))),
            }
        } else {
            Err(TransportError::ConnectionFailed("Connection not in endpoint".into()))
        }
    } else {
        Err(TransportError::Other("Endpoint not initialized".into()))
    }
}
```

### 2. Fix handle_receive() (worker.rs:671-694)
```rust
fn handle_receive(
    &mut self,
    connection_id: ConnId,
    stream_id: u64,
    max_length: usize,
) -> Result<Vec<u8>> {
    if let Some(ref mut endpoint) = self.endpoint {
        // ✅ Translate our connection ID to tQUIC's ID
        let tquic_conn_id = self.connections.get(&connection_id)
            .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;
        
        if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
            let mut buf = vec![0u8; max_length];
            match conn.stream_read(stream_id, &mut buf) {
                Ok((len, _fin)) => {
                    buf.truncate(len);
                    self.stats.bytes_received += len as u64;
                    Ok(buf)
                }
                Err(tquic::Error::Done) => {
                    // No data available yet - NOT an error, return empty
                    Ok(Vec::new())
                }
                Err(e) => Err(TransportError::Other(format!("Receive failed: {:?}", e))),
            }
        } else {
            Err(TransportError::ConnectionFailed("Connection not in endpoint".into()))
        }
    } else {
        Err(TransportError::Other("Endpoint not initialized".into()))
    }
}
```

### 3. Fix handle_open_stream() (worker.rs:702-722)
```rust
fn handle_open_stream(&mut self, connection_id: ConnId) -> Result<u64> {
    if let Some(ref mut endpoint) = self.endpoint {
        // ✅ Translate our connection ID to tQUIC's ID
        let tquic_conn_id = self.connections.get(&connection_id)
            .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;
        
        if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
            // Open bidirectional stream (urgency=0, incremental=false)
            match conn.stream_bidi_new(0, false) {
                Ok(stream_id) => {
                    self.stats.total_streams += 1;
                    Ok(stream_id)
                }
                Err(e) => Err(TransportError::Other(format!("Stream creation failed: {:?}", e))),
            }
        } else {
            Err(TransportError::ConnectionFailed("Connection not in endpoint".into()))
        }
    } else {
        Err(TransportError::Other("Endpoint not initialized".into()))
    }
}
```

### 4. Fix handle_accept_stream() (worker.rs:724-748)
```rust
fn handle_accept_stream(&mut self, connection_id: ConnId) -> Result<u64> {
    // ✅ Validate connection exists in our mapping
    let tquic_conn_id = self.connections.get(&connection_id)
        .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;
    
    // Verify connection still exists in endpoint
    if let Some(ref mut endpoint) = self.endpoint {
        if endpoint.conn_get_mut(*tquic_conn_id).is_none() {
            return Err(TransportError::ConnectionFailed("Connection not in endpoint".into()));
        }
    }

    // Check stream queue using our connection ID
    // Note: Stream queue keys should be our ConnId (set in on_stream_created fix)
    if let Ok(mut queue) = self.stream_queue.lock() {
        if let Some(stream_ids) = queue.get_mut(&connection_id) {
            if let Some(stream_id) = stream_ids.pop() {
                self.stats.total_streams += 1;
                return Ok(stream_id);
            }
        }
    }

    Err(TransportError::WouldBlock)
}
```

### 5. Fix on_stream_created() ID Translation (worker.rs:101-114)
```rust
fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
    let tquic_conn_id = conn.trace_id();
    
    // ✅ Translate tQUIC ID to our ConnId for stream queue
    if let Ok(our_conn_id) = self.get_our_connection_id(tquic_conn_id) {
        if let Ok(mut queue) = self.stream_queue.lock() {
            queue.entry(our_conn_id)
                .or_insert_with(Vec::new)
                .push(stream_id);
        }
    } else {
        // Connection not yet mapped - store with tQUIC ID as fallback
        // This handles race where stream created before connection fully established
        if let Ok(mut queue) = self.stream_queue.lock() {
            // Use a deterministic mapping from tQUIC ID bytes
            let fallback_key = ConnId::from_be_bytes(tquic_conn_id.as_bytes()[0..8].try_into().unwrap_or([0;8]));
            queue.entry(fallback_key)
                .or_insert_with(Vec::new)
                .push(stream_id);
        }
    }
}

// Helper method needed in TquicWorker
fn get_our_connection_id(&self, tquic_id: tquic::ConnectionId) -> Result<ConnId> {
    self.reverse_connections.get(&tquic_id)
        .copied()
        .ok_or_else(|| TransportError::Other("Connection ID not mapped".into()))
}
```

**Alternative**: Modify `WorkerTransportHandler` to have access to `reverse_connections`:
```rust
struct WorkerTransportHandler {
    // ... existing ...
    reverse_connections: Arc<Mutex<HashMap<tquic::ConnectionId, ConnId>>>,
}
```

## Testing Strategy

### Unit Tests
```rust
#[tokio::test]
async fn test_stream_send_receive() {
    // Setup: connect client -> server
    // Open stream on both sides
    // Send data from client
    // Receive on server
    // Verify data matches
}

#[tokio::test]
async fn test_bidirectional_stream() {
    // Open stream
    // Send both directions concurrently
    // Verify both receive correct data
}

#[tokio::test]
async fn test_stream_fin_handling() {
    // Send with fin=true
    // Verify receiver gets empty read with fin
}
```

## Validation Checklist
- [ ] `handle_send()` transmits actual QUIC packets (verify with Wireshark/logs)
- [ ] `handle_receive()` returns real data from network
- [ ] `handle_open_stream()` creates valid QUIC streams
- [ ] `handle_accept_stream()` returns stream IDs from queue
- [ ] `on_stream_created()` populates queue with correct keys
- [ ] FIN handling works correctly
- [ ] Error handling for invalid connection/stream IDs

## Dependencies
- Phase 1 complete (connection ID mapping must exist)
- tQUIC v1.6.0 stream API: `stream_write`, `stream_read`, `stream_bidi_new`

## Estimated Effort
1-2 hours focused development

---
*Context for Phase 2 of: Fix tQUIC Integration Gaps*
*Rust Implementation Context*
*Last updated: 2025-01-16*