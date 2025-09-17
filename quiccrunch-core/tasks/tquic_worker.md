# Task: TquicWorker Implementation

## Task Overview

Implement individual worker threads that run mio event loops with tQUIC endpoints and handle requests via message passing.

## Current Status

**Status**: pending  
**Dependencies**: Message types definition
**Blocks**: TquicWorkerPool, TquicAdapter

## Context

Core component of the multi-threaded tQUIC adapter. Each worker runs in its own thread with a dedicated mio::Poll and tQUIC::Endpoint, processing commands from the tokio layer.

## Technical Requirements

### Worker Structure
```rust
pub struct TquicWorker {
    worker_id: usize,
    poll: mio::Poll,
    endpoint: tquic::Endpoint,
    socket: mio::net::UdpSocket,
    command_rx: mpsc::Receiver<TquicCommand>,
    connections: HashMap<u64, tquic::Connection>,
    running: Arc<AtomicBool>,
}

impl TquicWorker {
    pub fn new(worker_id: usize, bind_addr: SocketAddr, tls_config: TlsConfig) -> Result<Self, Error>;
    pub fn run(&mut self) -> Result<(), Error>;
    fn handle_command(&mut self, cmd: TquicCommand) -> Result<(), Error>;
    fn handle_endpoint_events(&mut self) -> Result<(), Error>;
    fn handle_socket_events(&mut self) -> Result<(), Error>;
}
```

### Event Loop Design
- Single thread per worker (no Send constraints needed)
- mio::Poll with socket + waker registration
- Process tQUIC endpoint events
- Handle incoming commands from channels
- Manage connection lifecycle

### Key Operations
- **Connect**: Establish new QUIC connections
- **Send**: Send data on existing connections  
- **Accept**: Accept incoming connections
- **Close**: Clean connection termination

## Dependencies

**Required First**: TquicCommand and response message types

## Implementation Plan

### Phase 1: Basic Worker Structure
1. Create TquicWorker with mio::Poll + tQUIC::Endpoint
2. Implement command message loop
3. Basic socket event handling

### Phase 2: tQUIC Integration  
1. Proper tQUIC v1.6.0 API usage
2. Connection lifecycle management
3. Error handling and cleanup

### Phase 3: SO_REUSEPORT Support
1. Socket configuration for port reuse
2. Platform-specific socket options
3. Fallback for unsupported platforms

## Files to Modify

### New Files
- `src/transport/worker.rs` - Main implementation
- `src/transport/messages.rs` - Command/response types

## Notes

### Key Constraints
- Single-threaded design (no Send/Sync issues)
- Uses tQUIC v1.6.0 APIs directly
- mio event-driven architecture
- Channel-based communication with tokio layer