# tQUIC Worker Pool Integration Analysis

**Date**: 2025-10-21  
**Status**: Critical Blocker for Production Use  
**Priority**: P0 - Blocks all real streaming and benchmarking

## Executive Summary

The QuicCrunch project has a well-architected worker pool system with complete API structure, but the underlying tQUIC v1.6.0 integration has critical gaps preventing actual data transmission. The architecture is sound—the issue is solely in connecting worker operations to real tQUIC stream I/O.

### Current State
- ✅ **Architecture**: Complete multi-threaded worker pool design
- ✅ **API Layer**: Full tokio-compatible async interface  
- ✅ **Message Passing**: Command/response infrastructure working
- ✅ **Load Balancing**: Intelligent worker selection with health monitoring
- ⚠️ **tQUIC Integration**: Partially implemented, critical gaps in stream operations
- ❌ **Data Transmission**: Returns empty data (stub mode only)

---

## Architecture Overview

### Layer Structure

```
┌─────────────────────────────────────────────────────┐
│         Tokio Async Layer (adapter.rs)              │
│  TquicAdapter → TquicClient/Server → TquicStream    │
└──────────────────┬──────────────────────────────────┘
                   │ oneshot channels
                   ▼
┌─────────────────────────────────────────────────────┐
│      Worker Pool Manager (worker_pool.rs)           │
│  - Load balancing (round-robin, least-loaded)       │
│  - Connection affinity mapping                      │
│  - Health monitoring & worker restart               │
│  - Performance metrics collection                   │
└──────────────────┬──────────────────────────────────┘
                   │ mpsc channels
                   ▼
┌─────────────────────────────────────────────────────┐
│      Individual Workers (worker.rs)                  │
│  Thread-local tQUIC Endpoint + mio event loop       │
│  - Socket I/O (UDP packet send/recv)               │
│  - tQUIC protocol processing                        │
│  - Connection & stream lifecycle                    │
└─────────────────────────────────────────────────────┘
```

### Data Flow

#### Successful Path (Intended)
```
Client API Call
    ↓ (tokio task)
TquicAdapter.send(connection_id, stream_id, data)
    ↓ (oneshot channel)
WorkerPool.submit_command(Send { connection_id, stream_id, data })
    ↓ (select worker via connection affinity)
Worker.handle_send(connection_id, stream_id, data)
    ↓ (tQUIC API)
endpoint.conn_get_mut(connection_id).stream_write(stream_id, data, fin)
    ↓ (packet send handler)
socket.send_to(packet_data, dest_addr)
    ↓ (network)
[QUIC packets transmitted]
```

#### Current Reality (What Happens Now)
```
Client API Call
    ↓
TquicAdapter.send(...) 
    ↓
WorkerPool.submit_command(Send {...})
    ↓
Worker.handle_send(...)
    ↓
endpoint.conn_get_mut(connection_id) → None (connection not properly tracked)
    ↓
Returns: TransportError("Connection not found")
```

---

## Critical Implementation Gaps

### 1. Connection Lifecycle Management ⚠️ **CRITICAL**

**File**: `quiccrunch-core/src/transport/worker.rs:623-647`

**Problem**: 
- `handle_connect()` calls `endpoint.connect()` and returns a `tquic::Connection` 
- This connection is stored in `HashMap<ConnId, ConnId>` (wrong type!)
- The connection should be stored so it can be retrieved later
- `endpoint.conn_get_mut()` expects the tQUIC internal connection ID, not our wrapper ID

**Current Code** (worker.rs:636-642):
```rust
Ok(conn) => {
    self.connections.insert(conn_id, conn);  // ❌ Type mismatch: ConnId not Connection
    self.stats.total_connections += 1;
    self.stats.active_connections = self.connections.len();
    Ok(conn_id)
}
```

**Issue**: The type signature is:
```rust
connections: HashMap<ConnId, ConnId>  // ❌ Should be HashMap<ConnId, Connection>
```

But tQUIC `endpoint.connect()` returns `tquic::Connection`, and we're trying to store just IDs.

**Root Cause**: 
- tQUIC `Connection` type is NOT `Send` or `Sync`
- Cannot store it in a HashMap that crosses thread boundaries
- Must use tQUIC's internal connection tracking via `endpoint.conn_get_mut(internal_id)`

**Solution Required**:
1. Map our `ConnId` to tQUIC's internal connection ID
2. Store mapping: `HashMap<ConnId, tquic::ConnectionId>` (just IDs, not connections)
3. Always retrieve via `endpoint.conn_get_mut(internal_tquic_id)`

### 2. Connection ID Translation ⚠️ **CRITICAL**

**File**: Multiple worker.rs functions

**Problem**: We generate our own connection IDs (`next_connection_id`), but tQUIC has its own internal IDs. Need bidirectional mapping.

**Current State**:
```rust
// Our ID generation (worker.rs:625-626)
let conn_id = self.next_connection_id;  // Our sequential ID
self.next_connection_id += 1;

// tQUIC's ID from Connection
let tquic_conn_id = conn.trace_id();  // tQUIC's internal ConnectionId (special type)
```

**Missing**: No mapping between these two ID spaces!

**Required Data Structure**:
```rust
struct TquicWorker {
    // ... existing fields ...
    connections: HashMap<ConnId, tquic::ConnectionId>,  // Our ID → tQUIC ID
    reverse_connections: HashMap<tquic::ConnectionId, ConnId>,  // tQUIC ID → Our ID
}
```

**Where Used**:
- `handle_send()` - needs tQUIC ID to call `conn_get_mut()`
- `handle_receive()` - needs tQUIC ID to call `conn_get_mut()`
- `handle_open_stream()` - needs tQUIC ID to call `conn_get_mut()`
- `handle_accept_stream()` - needs tQUIC ID to call `conn_get_mut()`
- All other connection operations

### 3. Accept Path Implementation ❌ **NOT IMPLEMENTED**

**File**: `quiccrunch-core/src/transport/worker.rs:696-700`

**Current Code**:
```rust
fn handle_accept(&mut self) -> Result<(ConnId, SocketAddr)> {
    // For accepting connections, we need to wait for the transport handler to notify us
    // This is a simplified implementation - in reality we'd need a queue of pending connections
    Err(TransportError::Other("Accept not yet implemented".into()))
}
```

**Problem**: Server mode completely non-functional without this.

**What's Needed**:
1. **Pending Connection Queue**: Store incoming connections notified by `TransportHandler::on_conn_established()`
2. **Blocking/Async Wait**: Block `handle_accept()` until a connection arrives
3. **Connection Registration**: Add accepted connection to our tracking maps

**Design**:
```rust
struct TquicWorker {
    // ... existing fields ...
    pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,
}

impl TransportHandler for WorkerTransportHandler {
    fn on_conn_established(&mut self, conn: &mut Connection) {
        let conn_id = conn.trace_id();
        let remote_addr = /* extract from connection */;
        
        // Add to pending queue
        if let Ok(mut queue) = self.pending_connections.lock() {
            queue.push_back((conn_id, remote_addr));
        }
    }
}

fn handle_accept(&mut self) -> Result<(ConnId, SocketAddr)> {
    // Poll pending connections queue
    if let Ok(mut queue) = self.pending_connections.lock() {
        if let Some((tquic_conn_id, remote_addr)) = queue.pop_front() {
            let our_conn_id = self.next_connection_id;
            self.next_connection_id += 1;
            
            // Store mapping
            self.connections.insert(our_conn_id, tquic_conn_id);
            self.reverse_connections.insert(tquic_conn_id, our_conn_id);
            
            return Ok((our_conn_id, remote_addr));
        }
    }
    
    // No pending connections
    Err(TransportError::WouldBlock)
}
```

### 4. Stream Data Flow Verification ⚠️ **NEEDS VALIDATION**

**Files**: 
- `worker.rs:649-669` - `handle_send()`
- `worker.rs:671-694` - `handle_receive()`

**Current Implementation**:
```rust
fn handle_send(...) -> Result<()> {
    if let Some(ref mut endpoint) = self.endpoint {
        if let Some(conn) = endpoint.conn_get_mut(connection_id) {  // ❌ Wrong ID type
            let bytes_data = Bytes::from(data);
            match conn.stream_write(stream_id, bytes_data, fin) {
                Ok(_) => Ok(()),  // ✅ API correct
                Err(e) => Err(...)
            }
        } else {
            Err(TransportError::Other("Connection not found".into()))  // ❌ Always hits this!
        }
    }
}
```

**Issues**:
1. `connection_id` passed is our ID, not tQUIC's ID
2. `endpoint.conn_get_mut(our_id)` fails because tQUIC doesn't know our ID
3. Never actually writes data to stream

**Fix Required**:
```rust
fn handle_send(...) -> Result<()> {
    if let Some(ref mut endpoint) = self.endpoint {
        // ✅ Translate our ID to tQUIC's ID
        let tquic_conn_id = self.connections.get(&connection_id)
            .ok_or_else(|| TransportError::Other("Connection not found".into()))?;
        
        if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
            let bytes_data = Bytes::from(data);
            match conn.stream_write(stream_id, bytes_data, fin) {
                Ok(written) => {
                    self.stats.bytes_sent += written as u64;
                    Ok(())
                },
                Err(e) => Err(TransportError::Other(format!("Send failed: {:?}", e)))
            }
        } else {
            Err(TransportError::Other("Connection not found in endpoint".into()))
        }
    } else {
        Err(TransportError::Other("Endpoint not initialized".into()))
    }
}
```

### 5. TransportHandler Integration ⚠️ **PARTIAL**

**File**: `quiccrunch-core/src/transport/worker.rs:44-143`

**Current State**: Basic handlers implemented for lifecycle events

**Working**:
- ✅ `on_conn_created()` - logs connection creation
- ✅ `on_conn_closed()` - emits event  
- ✅ `on_stream_created()` - adds to stream queue
- ✅ Stream readable/writable notifications

**Missing**:
- ❌ Connection acceptance queue (for server mode)
- ❌ Peer address extraction in `on_conn_established()` (placeholder used)
- ❌ Stream data availability signaling (for async receive)

**Code Issues** (worker.rs:76-78):
```rust
let conn_id = conn.trace_id();
// Remove peer_addr call - not available in tQUIC v1.6.0
let remote_addr = "unknown:0".parse().unwrap(); // ❌ Placeholder
```

**Required**: 
- Investigate tQUIC v1.6.0 API for peer address extraction
- Possible: Store peer address during `endpoint.connect()` or socket receive

### 6. Stream Queue Mechanism ⚠️ **NEEDS TESTING**

**File**: `worker.rs:724-748` - `handle_accept_stream()`

**Current Implementation**:
```rust
fn handle_accept_stream(&mut self, connection_id: ConnId) -> Result<u64> {
    // ... validation ...
    
    if let Ok(mut queue) = self.stream_queue.lock() {
        if let Some(stream_ids) = queue.get_mut(&connection_id) {
            if let Some(stream_id) = stream_ids.pop() {
                self.stats.total_streams += 1;
                return Ok(stream_id);
            }
        }
    }
    
    Err(TransportError::Other("No pending streams available".into()))
}
```

**Architecture**: Stream IDs added by `TransportHandler::on_stream_created()`

**Issues**:
1. ⚠️ Uses our `ConnId`, but `on_stream_created()` extracts from `conn.trace_id()` (tQUIC ID)
2. ❌ ID space mismatch - queue won't work!
3. ❌ No wakeup mechanism when stream becomes available (returns error immediately)

**Fix Required**:
```rust
// In on_stream_created() - translate tQUIC ID to our ID
fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
    let tquic_conn_id = conn.trace_id();
    
    // ✅ Translate to our connection ID
    let our_conn_id = self.reverse_connections.get(&tquic_conn_id)
        .copied()
        .unwrap_or(0);  // Handle missing mapping
    
    if let Ok(mut queue) = self.stream_queue.lock() {
        queue.entry(our_conn_id).or_insert_with(Vec::new).push(stream_id);
    }
}
```

---

## tQUIC v1.6.0 API Reference

### Connection Management

```rust
// Create endpoint
let endpoint = Endpoint::new(
    Box::new(config),      // QUIC config
    is_server: bool,       // Server or client mode
    Box::new(handler),     // TransportHandler
    Rc::new(sender),       // PacketSendHandler
);

// Client: Connect to remote
let conn: tquic::Connection = endpoint.connect(
    local_addr: SocketAddr,
    remote_addr: SocketAddr,
    server_name: Option<&str>,
    session: Option<&[u8]>,
    token: Option<&[u8]>,
    config: Option<&Config>,
)?;

// Get connection by tQUIC's internal ID
let conn: Option<&mut Connection> = endpoint.conn_get_mut(conn_id: tquic::ConnectionId);

// Connection ID type
let conn_id: tquic::ConnectionId = conn.trace_id();
```

### Stream Operations

```rust
// Open bidirectional stream
let stream_id: u64 = conn.stream_bidi_new(urgency: u8, incremental: bool)?;

// Write to stream
let written: usize = conn.stream_write(
    stream_id: u64,
    data: bytes::Bytes,
    fin: bool,
)?;

// Read from stream
let (len, fin): (usize, bool) = conn.stream_read(
    stream_id: u64,
    buf: &mut [u8],
)?;

// Shutdown stream
conn.stream_shutdown(
    stream_id: u64,
    direction: tquic::Shutdown,  // ::Read, ::Write, or ::Both
    error_code: u64,
)?;

// Close connection
conn.close(
    app: bool,           // Application close (true) vs transport close
    error_code: u64,
    reason: &[u8],
)?;
```

### Event Loop Integration

```rust
// Receive packet from socket
endpoint.recv(
    buf: &mut [u8],
    pkt_info: &tquic::PacketInfo,
)?;

// Process pending operations
endpoint.process_connections();

// PacketInfo structure
struct PacketInfo {
    src: SocketAddr,   // Source address
    dst: SocketAddr,   // Destination address  
    time: Instant,     // Receipt timestamp
}
```

### Handlers

```rust
// Transport lifecycle events
trait TransportHandler {
    fn on_conn_created(&mut self, conn: &mut Connection);
    fn on_conn_established(&mut self, conn: &mut Connection);
    fn on_conn_closed(&mut self, conn: &mut Connection);
    fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64);
    fn on_stream_readable(&mut self, conn: &mut Connection, stream_id: u64);
    fn on_stream_writable(&mut self, conn: &mut Connection, stream_id: u64);
    fn on_stream_closed(&mut self, conn: &mut Connection, stream_id: u64);
    fn on_new_token(&mut self, conn: &mut Connection, token: Vec<u8>);
}

// Packet transmission
trait PacketSendHandler {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, PacketInfo)]) -> tquic::Result<usize>;
}
```

---

## Implementation Plan

### Phase 1: Fix Connection Management (2-3 hours)

**Objective**: Make connection lifecycle work correctly

**Tasks**:

1. **Update worker data structures** (worker.rs:26-42)
   ```rust
   pub struct TquicWorker {
       // Change from HashMap<ConnId, ConnId>
       connections: HashMap<ConnId, tquic::ConnectionId>,
       reverse_connections: HashMap<tquic::ConnectionId, ConnId>,
       
       // Add pending connection queue for accept
       pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,
       
       // ... rest unchanged
   }
   ```

2. **Fix `handle_connect()`** (worker.rs:623-647)
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
               Ok(tquic_conn_id) => {  // ✅ Returns ConnectionId, not Connection
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

3. **Implement `handle_accept()`** (worker.rs:696-700)
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
       Err(TransportError::Other("No pending connections".into()))
   }
   ```

4. **Update `TransportHandler::on_conn_established()`** (worker.rs:72-86)
   ```rust
   fn on_conn_established(&mut self, conn: &mut Connection) {
       let tquic_conn_id = conn.trace_id();
       
       // Try to get peer address (method depends on tQUIC v1.6.0 API)
       // For now, use placeholder - needs API research
       let remote_addr = "0.0.0.0:0".parse().unwrap();
       
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
   ```

5. **Update `WorkerTransportHandler` to include pending queue** (worker.rs:44-48)
   ```rust
   struct WorkerTransportHandler {
       worker_id: usize,
       event_tx: Option<mpsc::Sender<TquicEvent>>,
       stream_queue: Arc<std::sync::Mutex<HashMap<ConnId, Vec<u64>>>>,
       pending_connections: Arc<Mutex<VecDeque<(tquic::ConnectionId, SocketAddr)>>>,  // ✅ Add
   }
   ```

### Phase 2: Fix Stream Operations (1-2 hours)

**Objective**: Make send/receive actually transmit data

**Tasks**:

1. **Fix `handle_send()`** (worker.rs:649-669)
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
               .ok_or_else(|| TransportError::Other("Connection not found".into()))?;
           
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
               Err(TransportError::Other("Connection not in endpoint".into()))
           }
       } else {
           Err(TransportError::Other("Endpoint not initialized".into()))
       }
   }
   ```

2. **Fix `handle_receive()`** (worker.rs:671-694)
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
               .ok_or_else(|| TransportError::Other("Connection not found".into()))?;
           
           if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
               let mut buf = vec![0u8; max_length];
               match conn.stream_read(stream_id, &mut buf) {
                   Ok((len, _fin)) => {
                       buf.truncate(len);
                       self.stats.bytes_received += len as u64;
                       Ok(buf)
                   }
                   Err(tquic::Error::Done) => {
                       // No data available yet - not an error
                       Ok(Vec::new())
                   }
                   Err(e) => Err(TransportError::Other(format!("Receive failed: {:?}", e))),
               }
           } else {
               Err(TransportError::Other("Connection not in endpoint".into()))
           }
       } else {
           Err(TransportError::Other("Endpoint not initialized".into()))
       }
   }
   ```

3. **Fix `handle_open_stream()`** (worker.rs:702-722)
   ```rust
   fn handle_open_stream(&mut self, connection_id: ConnId) -> Result<u64> {
       if let Some(ref mut endpoint) = self.endpoint {
           // ✅ Translate our connection ID to tQUIC's ID
           let tquic_conn_id = self.connections.get(&connection_id)
               .ok_or_else(|| TransportError::Other("Connection not found".into()))?;
           
           if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
               match conn.stream_bidi_new(0, false) {
                   Ok(stream_id) => {
                       self.stats.total_streams += 1;
                       Ok(stream_id)
                   }
                   Err(e) => Err(TransportError::Other(format!("Stream creation failed: {:?}", e))),
               }
           } else {
               Err(TransportError::Other("Connection not in endpoint".into()))
           }
       } else {
           Err(TransportError::Other("Endpoint not initialized".into()))
       }
   }
   ```

4. **Fix `handle_accept_stream()`** stream queue ID translation (worker.rs:724-748)
   ```rust
   fn handle_accept_stream(&mut self, connection_id: ConnId) -> Result<u64> {
       // ✅ Validate connection exists
       let tquic_conn_id = self.connections.get(&connection_id)
           .ok_or_else(|| TransportError::ConnectionFailed("Connection not found".into()))?;
       
       if let Some(ref mut endpoint) = self.endpoint {
           if endpoint.conn_get_mut(*tquic_conn_id).is_none() {
               return Err(TransportError::ConnectionFailed("Connection not in endpoint".into()));
           }
       }

       // Check stream queue using our connection ID (queue keys already translated)
       if let Ok(mut queue) = self.stream_queue.lock() {
           if let Some(stream_ids) = queue.get_mut(&connection_id) {
               if let Some(stream_id) = stream_ids.pop() {
                   self.stats.total_streams += 1;
                   return Ok(stream_id);
               }
           }
       }

       Err(TransportError::Other("No pending streams available".into()))
   }
   ```

5. **Fix `on_stream_created()` ID translation** (worker.rs:101-114)
   ```rust
   fn on_stream_created(&mut self, conn: &mut Connection, stream_id: u64) {
       let tquic_conn_id = conn.trace_id();
       
       // ✅ We need access to reverse_connections here
       // This requires passing it through WorkerTransportHandler
       // For now, store with tQUIC ID and handle translation in handle_accept_stream
       
       if let Ok(mut queue) = self.stream_queue.lock() {
           // Store with tQUIC connection ID for now
           // TODO: Add reverse lookup to translate to our ID
           queue.entry(tquic_conn_id.as_bytes()[0] as u64)
               .or_insert_with(Vec::new)
               .push(stream_id);
       }
   }
   ```

   **Note**: This requires refactoring `WorkerTransportHandler` to have access to connection ID mappings, or changing the queue to use tQUIC IDs directly.

### Phase 3: Fix Connection Close & Cleanup (30 minutes)

**Objective**: Properly clean up all tracking on connection close

**Tasks**:

1. **Fix `handle_close_connection()`** (worker.rs:759-768)
   ```rust
   fn handle_close_connection(&mut self, connection_id: ConnId, error_code: u64, reason: Vec<u8>) {
       // ✅ Translate to tQUIC ID
       if let Some(tquic_conn_id) = self.connections.get(&connection_id) {
           if let Some(ref mut endpoint) = self.endpoint {
               if let Some(conn) = endpoint.conn_get_mut(*tquic_conn_id) {
                   let _ = conn.close(true, error_code, &reason);
               }
           }
           
           // Remove from both directions of mapping
           self.reverse_connections.remove(tquic_conn_id);
       }
       
       self.connections.remove(&connection_id);
       
       // Clean up stream queue for this connection
       if let Ok(mut queue) = self.stream_queue.lock() {
           queue.remove(&connection_id);
       }
       
       self.stats.active_connections = self.connections.len();
   }
   ```

2. **Fix `on_conn_closed()` handler** (worker.rs:88-99)
   ```rust
   fn on_conn_closed(&mut self, conn: &mut Connection) {
       let tquic_conn_id = conn.trace_id();
       
       // We don't have direct access to worker's connection maps here
       // Just emit event for monitoring
       if let Some(ref tx) = self.event_tx {
           let _ = tx.try_send(TquicEvent::ConnectionClosed {
               connection_id: 0,  // Would need reverse lookup
               error_code: 0,
               reason: Vec::new(),
           });
       }
   }
   ```

### Phase 4: Testing & Validation (2-3 hours)

**Objective**: Verify all operations work end-to-end

**Test Plan**:

1. **Unit Tests** (add to `src/transport/test_worker_pool.rs`)
   - ✅ Worker pool creation
   - ✅ Load balancing
   - ✅ Connection affinity
   - ❌ **Add**: Connection ID translation
   - ❌ **Add**: Stream send/receive with real data
   - ❌ **Add**: Connection lifecycle (connect → use → close)

2. **Integration Tests** (new file: `tests/tquic_integration.rs`)
   ```rust
   #[tokio::test]
   async fn test_simple_connect_send_receive() {
       // Start server
       let server = TquicServer::new(Some(2)).await.unwrap();
       let mut listener = server.listen("127.0.0.1:4433").await.unwrap();
       
       // Start client
       let client = TquicClient::new(Some(2)).await.unwrap();
       let conn = client.connect("127.0.0.1:4433").await.unwrap();
       
       // Open stream
       let mut stream = conn.open_stream().await.unwrap();
       
       // Send data
       let test_data = b"Hello, tQUIC!";
       stream.send(test_data, false).await.unwrap();
       
       // Server accepts and reads
       let server_conn = listener.accept().await.unwrap();
       let mut server_stream = server_conn.accept_stream().await.unwrap();
       let received = server_stream.receive(1024).await.unwrap();
       
       assert_eq!(received, test_data);
   }
   ```

3. **Run bidirectional_streaming example**
   ```bash
   # Terminal 1
   cargo run --example bidirectional_streaming server
   
   # Terminal 2  
   cargo run --example bidirectional_streaming client
   
   # Expected: Real data transmitted, not empty stubs
   ```

4. **Run benchmarks**
   ```bash
   cargo run --release --example benchmark_suite
   # Should see actual throughput numbers, not errors
   ```

---

## Expected Outcomes

### After Phase 1 (Connection Management)
- ✅ Connections can be established (client and server)
- ✅ Connection IDs properly mapped between our layer and tQUIC
- ✅ Server can accept incoming connections
- ❌ Still can't send/receive data (Phase 2 needed)

### After Phase 2 (Stream Operations)  
- ✅ `stream.send()` actually transmits data
- ✅ `stream.receive()` returns real data from network
- ✅ Bidirectional streaming works
- ✅ Examples show real data flow

### After Phase 3 (Cleanup)
- ✅ Connections close cleanly
- ✅ No memory leaks from lingering state
- ✅ Worker statistics accurate

### After Phase 4 (Testing)
- ✅ All tests pass
- ✅ Benchmark suite runs successfully
- ✅ Ready for production use

---

## Risk Assessment

### Low Risk ✅
- Architecture is sound
- APIs are well-designed
- Worker pool infrastructure complete
- Just needs connection ID translation layer

### Medium Risk ⚠️
- tQUIC v1.6.0 API documentation sparse
- May discover additional API incompatibilities
- Stream queue mechanism needs validation

### High Risk ❌
- None identified - issues are all in implementation, not design

---

## Time Estimate

- **Phase 1**: 2-3 hours (connection management fixes)
- **Phase 2**: 1-2 hours (stream operation fixes)  
- **Phase 3**: 30 minutes (cleanup fixes)
- **Phase 4**: 2-3 hours (testing & validation)

**Total**: 6-9 hours of focused development work

---

## Next Steps

1. **Immediate**: Start Phase 1 - fix connection ID translation
2. **Follow-up**: Phase 2 - stream operations
3. **Validate**: Phase 4 - comprehensive testing
4. **Document**: Update TODO.md with completion status

---

## References

- tQUIC Repository: https://github.com/tencent/tquic
- Worker Implementation: `quiccrunch-core/src/transport/worker.rs`
- Worker Pool: `quiccrunch-core/src/transport/worker_pool.rs`
- Adapter Layer: `quiccrunch-core/src/transport/adapter.rs`
- Task Docs: `quiccrunch-core/tasks/tquic_worker.md`
