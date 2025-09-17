use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use dashmap::DashMap;

use crate::error::TransportError;
use crate::transport::messages::{TquicCommand, WorkerConfig, WorkerStats, WorkerHealth, WorkerStatus, ConnectionId};

pub struct TquicWorkerPool {
    workers: Vec<TquicWorkerHandle>,
    next_worker: AtomicUsize,
    connection_map: Arc<DashMap<ConnectionId, usize>>, // connection_id -> worker_index
    worker_count: usize,
}

pub struct TquicWorkerHandle {
    worker_id: usize,
    command_tx: mpsc::Sender<TquicCommand>,
    join_handle: JoinHandle<()>,
    is_healthy: Arc<std::sync::atomic::AtomicBool>,
}

impl TquicWorkerPool {
    pub fn new(worker_count: usize, base_config: WorkerConfig) -> Result<Self, TransportError> {
        if worker_count == 0 {
            return Err(TransportError::ConfigError("Worker count must be greater than 0".into()));
        }

        let mut workers = Vec::with_capacity(worker_count);
        let connection_map = Arc::new(DashMap::new());

        for worker_id in 0..worker_count {
            let mut config = base_config.clone();
            config.worker_id = worker_id;
            
            // Adjust port for each worker (for SO_REUSEPORT)
            if let Ok(mut addr) = config.bind_addr.to_string().parse::<std::net::SocketAddr>() {
                if addr.port() != 0 {
                    addr.set_port(addr.port() + worker_id as u16);
                    config.bind_addr = addr;
                }
            }

            let (command_tx, command_rx) = mpsc::channel::<TquicCommand>();
            let is_healthy = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let is_healthy_clone = is_healthy.clone();

            let join_handle = thread::Builder::new()
                .name(format!("tquic-worker-{}", worker_id))
                .spawn(move || {
                    // Create and run a simple worker loop
                    let result = Self::run_worker_loop(config, command_rx);
                    
                    if let Err(e) = result {
                        tracing::error!("Worker {} failed: {}", worker_id, e);
                        is_healthy_clone.store(false, Ordering::Relaxed);
                    }
                })
                .map_err(|e| TransportError::ConfigError(format!("Failed to spawn worker thread: {}", e)))?;

            workers.push(TquicWorkerHandle {
                worker_id,
                command_tx,
                join_handle,
                is_healthy,
            });
        }

        Ok(TquicWorkerPool {
            workers,
            next_worker: AtomicUsize::new(0),
            connection_map,
            worker_count,
        })
    }

    pub async fn submit_command(&self, cmd: TquicCommand) -> Result<(), TransportError> {
        let worker_id = self.select_worker(&cmd);
        
        if worker_id >= self.workers.len() {
            return Err(TransportError::ConfigError("Invalid worker selection".into()));
        }

        let worker = &self.workers[worker_id];
        
        // Check if worker is healthy
        if !worker.is_healthy.load(Ordering::Relaxed) {
            return Err(TransportError::ConnectionFailed("Worker is unhealthy".into()));
        }

        // Update connection mapping for connection-specific commands
        if let TquicCommand::Connect { response, .. } = &cmd {
            // For new connections, we'll get the connection ID from the response
            // and store the mapping
        }

        worker.command_tx.send(cmd)
            .map_err(|_| TransportError::ConnectionFailed("Worker channel closed".into()))?;

        Ok(())
    }

    pub async fn shutdown(&self, timeout: Duration) -> Result<(), TransportError> {
        // Send shutdown commands to all workers
        for worker in &self.workers {
            let _ = worker.command_tx.send(TquicCommand::Shutdown);
        }

        // Wait for workers to finish or timeout
        let start = std::time::Instant::now();
        for worker in &self.workers {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(TransportError::Other("Shutdown timeout".into()));
            }
            
            // We can't easily timeout thread joins in std, so we'll just check health
            if !worker.is_healthy.load(Ordering::Relaxed) {
                tracing::warn!("Worker {} already unhealthy during shutdown", worker.worker_id);
            }
        }

        Ok(())
    }

    fn select_worker(&self, cmd: &TquicCommand) -> usize {
        match cmd {
            TquicCommand::Connect { .. } => {
                // Round-robin for new connections
                self.next_worker.fetch_add(1, Ordering::Relaxed) % self.worker_count
            }
            TquicCommand::Send { connection_id, .. } |
            TquicCommand::Receive { connection_id, .. } |
            TquicCommand::OpenStream { connection_id, .. } |
            TquicCommand::CloseStream { connection_id, .. } |
            TquicCommand::CloseConnection { connection_id, .. } => {
                // Use connection affinity
                if let Some(entry) = self.connection_map.get(connection_id) {
                    *entry
                } else {
                    // Fallback to round-robin if mapping lost
                    self.next_worker.fetch_add(1, Ordering::Relaxed) % self.worker_count
                }
            }
            TquicCommand::Accept { .. } => {
                // Round-robin for accept requests
                self.next_worker.fetch_add(1, Ordering::Relaxed) % self.worker_count
            }
            TquicCommand::Shutdown => 0, // Doesn't matter for shutdown
        }
    }

    pub fn get_worker_status(&self) -> Vec<WorkerStatus> {
        self.workers.iter().map(|worker| {
            let health = if worker.is_healthy.load(Ordering::Relaxed) {
                WorkerHealth::Healthy
            } else {
                WorkerHealth::Failed { error: "Worker thread failed".into() }
            };

            WorkerStatus {
                worker_id: worker.worker_id,
                health,
                stats: WorkerStats {
                    worker_id: worker.worker_id,
                    active_connections: 0, // TODO: Track actual stats
                    total_connections: 0,
                    total_streams: 0,
                    bytes_sent: 0,
                    bytes_received: 0,
                    errors: 0,
                    uptime: Duration::from_secs(0),
                },
                queue_depth: 0, // TODO: Track queue depth
            }
        }).collect()
    }

    async fn run_worker_loop(
        config: WorkerConfig,
        command_rx: mpsc::Receiver<TquicCommand>,
    ) -> Result<(), TransportError> {
        tracing::info!("Starting tQUIC worker {} on {}", config.worker_id, config.bind_addr);

        // For now, implement a simple message loop that responds to commands
        // This will be replaced with actual tQUIC worker implementation
        let mut running = true;
        
        while running {
            match command_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(cmd) => {
                    match cmd {
                        TquicCommand::Connect { remote_addr, response } => {
                            tracing::debug!("Worker {} handling connect to {}", config.worker_id, remote_addr);
                            // For now, return a dummy connection ID
                            let conn_id = (config.worker_id as u64) << 32 | 1;
                            let _ = response.send(Ok(conn_id));
                        }
                        TquicCommand::Send { connection_id, stream_id, data, fin, response } => {
                            tracing::debug!("Worker {} handling send on connection {} stream {}", 
                                config.worker_id, connection_id, stream_id);
                            // Simulate successful send
                            let _ = response.send(Ok(()));
                        }
                        TquicCommand::Receive { connection_id, stream_id, max_length, response } => {
                            tracing::debug!("Worker {} handling receive on connection {} stream {}", 
                                config.worker_id, connection_id, stream_id);
                            // Return empty data for now
                            let _ = response.send(Ok(Vec::new()));
                        }
                        TquicCommand::Accept { response } => {
                            tracing::debug!("Worker {} handling accept", config.worker_id);
                            // Return dummy connection for now
                            let conn_id = (config.worker_id as u64) << 32 | 2;
                            let remote_addr = "127.0.0.1:0".parse().unwrap();
                            let _ = response.send(Ok((conn_id, remote_addr)));
                        }
                        TquicCommand::OpenStream { connection_id, response } => {
                            tracing::debug!("Worker {} handling open stream on connection {}", 
                                config.worker_id, connection_id);
                            // Return dummy stream ID
                            let _ = response.send(Ok(1));
                        }
                        TquicCommand::CloseStream { connection_id, stream_id, error_code } => {
                            tracing::debug!("Worker {} handling close stream {} on connection {}", 
                                config.worker_id, stream_id, connection_id);
                        }
                        TquicCommand::CloseConnection { connection_id, error_code, reason } => {
                            tracing::debug!("Worker {} handling close connection {}", 
                                config.worker_id, connection_id);
                        }
                        TquicCommand::Shutdown => {
                            tracing::info!("Worker {} shutting down", config.worker_id);
                            running = false;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Normal timeout, continue loop
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::info!("Worker {} command channel disconnected", config.worker_id);
                    running = false;
                }
            }
        }

        tracing::info!("Worker {} stopped", config.worker_id);
        Ok(())
    }
}

impl Drop for TquicWorkerPool {
    fn drop(&mut self) {
        // Send shutdown commands
        for worker in &self.workers {
            let _ = worker.command_tx.send(TquicCommand::Shutdown);
        }
        
        // Note: We can't easily wait for threads to finish in Drop
        // The shutdown() method should be called explicitly for graceful shutdown
    }
}