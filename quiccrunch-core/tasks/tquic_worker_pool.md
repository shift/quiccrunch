# Task: TquicWorkerPool Implementation

## Task Overview

Implement a thread pool manager for tQUIC worker threads that handles load balancing, worker health monitoring, and graceful shutdown.

## Current Status

**Status**: pending

**Dependencies**: None (can be implemented independently)

**Blocks**: TquicAdapter, socket sharing, integration tasks

## Context

Part of the multi-threaded tQUIC adapter architecture. The WorkerPool manages multiple TquicWorker threads, distributing workload and ensuring system reliability.

## Technical Requirements

### Core Functionality

**WorkerPool Management**:
- Spawn configurable number of worker threads
- Health monitoring and worker restart on failure  
- Load balancing across workers
- Graceful shutdown coordination

**Load Balancing Strategies**:
- Round-robin for new connections
- Connection affinity (existing connections stick to same worker)
- Consider least-loaded balancing for future optimization

**Worker Health Monitoring**:
- Detect worker thread panics/failures
- Automatic worker restart with exponential backoff
- Worker performance metrics collection

**Shutdown Handling**:
- Signal all workers to stop processing new requests
- Wait for in-flight requests to complete (with timeout)
- Force shutdown after grace period

### Interface Design

```rust
pub struct TquicWorkerPool {
    workers: Vec<TquicWorkerHandle>,
    next_worker: AtomicUsize, // for round-robin
    connection_map: Arc<DashMap<u64, usize>>, // connection_id -> worker_index
    shutdown_signal: watch::Sender<bool>,
}

pub struct TquicWorkerHandle {
    worker_id: usize,
    command_tx: mpsc::Sender<TquicCommand>,
    join_handle: JoinHandle<()>,
    is_healthy: Arc<AtomicBool>,
}

impl TquicWorkerPool {
    pub fn new(worker_count: usize) -> Result<Self, Error>;
    pub async fn submit_command(&self, cmd: TquicCommand) -> Result<Response, Error>;
    pub async fn shutdown(&self, timeout: Duration) -> Result<(), Error>;
    
    // Internal methods
    fn select_worker(&self, cmd: &TquicCommand) -> usize;
    fn restart_worker(&self, worker_id: usize) -> Result<(), Error>;
    fn health_check(&self) -> HealthStatus;
}
```

### Message Routing Logic

```rust
fn select_worker(&self, cmd: &TquicCommand) -> usize {
    match cmd {
        TquicCommand::Connect { .. } => {
            // Round-robin for new connections
            self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
        }
        TquicCommand::Send { connection_id, .. } |
        TquicCommand::Close { connection_id } => {
            // Use connection affinity
            if let Some(worker_id) = self.connection_map.get(connection_id) {
                *worker_id
            } else {
                // Fallback to round-robin if mapping lost
                self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
            }
        }
        TquicCommand::Accept { .. } => {
            // Round-robin for accept requests
            self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
        }
    }
}
```

## Dependencies

**Required First**: TquicWorker and TquicCommand message types

**Blocks**: TquicAdapter, integration with transport layer

## Implementation Plan

### Phase 1: Basic Pool Structure
1. Create TquicWorkerPool struct with worker management
2. Implement worker spawning and basic lifecycle management
3. Add round-robin load balancing

### Phase 2: Connection Affinity  
1. Implement connection ID to worker mapping
2. Add affinity-based routing for existing connections
3. Handle connection cleanup on close

### Phase 3: Health Monitoring
1. Add worker health status tracking
2. Implement automatic worker restart on failure
3. Add exponential backoff for restart attempts

### Phase 4: Graceful Shutdown
1. Implement shutdown signaling to all workers
2. Add grace period for in-flight request completion
3. Force shutdown after timeout

## Files to Modify

### New Files
- `src/transport/worker_pool.rs` - Main implementation

### Dependencies
- `src/transport/worker.rs` - TquicWorker implementation  
- `src/transport/messages.rs` - TquicCommand and response types

## Testing Strategy

### Unit Tests
- Worker spawning and management
- Load balancing algorithm correctness
- Connection affinity maintenance
- Health monitoring and restart logic

### Integration Tests  
- Worker pool with real TquicWorker instances
- Graceful shutdown under load
- Worker failure and recovery scenarios

### Stress Tests
- High connection count distribution
- Worker restart under heavy load
- Memory leak detection during long runs

## Notes

### Key Design Decisions

**Connection Affinity**: Essential because tQUIC connections are tied to specific endpoints. Once a connection is established on a worker, all subsequent operations must route to the same worker.

**Health Monitoring**: Worker threads can panic due to tQUIC errors or memory issues. The pool must detect and restart failed workers automatically.

**Shutdown Strategy**: Two-phase shutdown:
1. Stop accepting new work, drain existing work
2. Force shutdown after timeout to prevent hanging

### Error Handling Strategies

**Worker Restart**: When a worker fails, restart it and clear its connection mappings. Clients will need to reconnect, but the system remains available.

**Command Routing Failures**: If a command can't be routed (e.g., worker down), try fallback worker or return error to client.

**Resource Exhaustion**: Monitor worker queue depths and reject new work if queues are full.

### Performance Considerations

**Lock-Free Structures**: Use `AtomicUsize` for round-robin counter and `DashMap` for lock-free connection mapping.

**Channel Sizing**: Worker command channels should be bounded to prevent memory exhaustion under load.

**CPU Affinity**: Consider pinning workers to specific CPU cores for better cache locality (future optimization).

### Platform Considerations

**Thread Spawning**: Use standard library thread spawning - no special requirements.

**Resource Limits**: Check system thread limits and fail gracefully if exceeded.

**Signal Handling**: Ensure clean shutdown on SIGTERM/SIGINT signals.