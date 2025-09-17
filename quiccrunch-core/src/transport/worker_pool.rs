use dashmap::DashMap;
use mio::Waker;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::error::TransportError;
use crate::transport::messages::{
    ConnectionId, TquicCommand, WorkerConfig, WorkerHealth, WorkerStats, WorkerStatus,
};

#[derive(Debug)]
pub struct TquicWorkerPool {
    workers: Vec<TquicWorkerHandle>,
    next_worker: AtomicUsize,
    connection_map: Arc<DashMap<ConnectionId, usize>>, // connection_id -> worker_index
    worker_count: usize,
    load_balancer: LoadBalancer,
    performance_monitor: PerformanceMonitor,
}

pub struct TquicWorkerHandle {
    worker_id: usize,
    command_tx: mpsc::Sender<TquicCommand>,
    waker: Option<Arc<Waker>>,
    join_handle: JoinHandle<()>,
    is_healthy: Arc<std::sync::atomic::AtomicBool>,
    stats: Arc<std::sync::RwLock<WorkerStats>>,
}

/// Intelligent load balancer that considers worker health and load
struct LoadBalancer {
    worker_loads: Vec<Arc<std::sync::atomic::AtomicUsize>>, // Active connections per worker
    last_selected: AtomicUsize,
}

/// Performance monitor for tracking worker pool efficiency 
struct PerformanceMonitor {
    start_time: Instant,
    total_connections: AtomicUsize,
    total_commands: AtomicUsize,
    socket_distribution_stats: Arc<DashMap<usize, AtomicUsize>>, // worker_id -> packet_count
}

impl LoadBalancer {
    fn new(worker_count: usize) -> Self {
        let mut worker_loads = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            worker_loads.push(Arc::new(AtomicUsize::new(0)));
        }
        
        Self {
            worker_loads,
            last_selected: AtomicUsize::new(0),
        }
    }
    
    /// Select the best worker based on current load and health
    fn select_worker(&self, worker_handles: &[TquicWorkerHandle]) -> usize {
        let worker_count = self.worker_loads.len();
        
        // Find worker with minimum load that is healthy
        let mut min_load = usize::MAX;
        let mut best_worker = 0;
        
        for i in 0..worker_count {
            if !worker_handles[i].is_healthy.load(Ordering::Relaxed) {
                continue; // Skip unhealthy workers
            }
            
            let current_load = self.worker_loads[i].load(Ordering::Relaxed);
            if current_load < min_load {
                min_load = current_load;
                best_worker = i;
            }
        }
        
        // If all workers have the same load, use round-robin
        if min_load == usize::MAX {
            // All workers are unhealthy, fallback to round-robin
            best_worker = self.last_selected.fetch_add(1, Ordering::Relaxed) % worker_count;
        }
        
        // Increment selected worker's load
        self.worker_loads[best_worker].fetch_add(1, Ordering::Relaxed);
        
        best_worker
    }
    
    /// Decrement worker load when a connection closes
    fn release_worker_load(&self, worker_id: usize) {
        if worker_id < self.worker_loads.len() {
            self.worker_loads[worker_id].fetch_sub(1, Ordering::Relaxed);
        }
    }
    
    /// Get current worker load distribution
    fn get_load_distribution(&self) -> Vec<usize> {
        self.worker_loads
            .iter()
            .map(|load| load.load(Ordering::Relaxed))
            .collect()
    }
}

impl PerformanceMonitor {
    fn new(worker_count: usize) -> Self {
        let socket_distribution_stats = Arc::new(DashMap::new());
        for i in 0..worker_count {
            socket_distribution_stats.insert(i, AtomicUsize::new(0));
        }
        
        Self {
            start_time: Instant::now(),
            total_connections: AtomicUsize::new(0),
            total_commands: AtomicUsize::new(0),
            socket_distribution_stats,
        }
    }
    
    fn record_command(&self) {
        self.total_commands.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_connection(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    fn record_packet_for_worker(&self, worker_id: usize) {
        if let Some(counter) = self.socket_distribution_stats.get(&worker_id) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    fn get_performance_metrics(&self) -> PerformanceMetrics {
        let uptime = self.start_time.elapsed();
        let total_connections = self.total_connections.load(Ordering::Relaxed);
        let total_commands = self.total_commands.load(Ordering::Relaxed);
        
        let distribution = self.socket_distribution_stats
            .iter()
            .map(|entry| (*entry.key(), entry.value().load(Ordering::Relaxed)))
            .collect();
        
        PerformanceMetrics {
            uptime,
            total_connections,
            total_commands,
            commands_per_second: if uptime.as_secs() > 0 {
                total_commands as f64 / uptime.as_secs_f64()
            } else {
                0.0
            },
            socket_distribution: distribution,
        }
    }
}

#[derive(Debug)]
pub struct PerformanceMetrics {
    pub uptime: Duration,
    pub total_connections: usize,
    pub total_commands: usize,
    pub commands_per_second: f64,
    pub socket_distribution: Vec<(usize, usize)>, // (worker_id, packet_count)
}

#[derive(Debug)]
pub struct SocketDistributionAnalysis {
    pub total_packets: usize,
    pub worker_count: usize,
    pub expected_per_worker: usize,
    pub max_deviation_percent: f64,
    pub fairness_score: f64, // 1.0 = perfectly fair, 0.0 = completely unfair
    pub per_worker_packets: Vec<(usize, usize)>, // (worker_id, packet_count)
    pub load_distribution: Vec<usize>, // Current load per worker
    pub is_balanced: bool, // True if distribution is considered fair
}
impl TquicWorkerPool {
    pub fn new(worker_count: usize, base_config: WorkerConfig) -> Result<Self, TransportError> {
        if worker_count == 0 {
            return Err(TransportError::ConfigError(
                "Worker count must be greater than 0".into(),
            ));
        }

        let mut workers = Vec::with_capacity(worker_count);
        let connection_map = Arc::new(DashMap::new());
        
        // Initialize performance monitoring and load balancing
        let load_balancer = LoadBalancer::new(worker_count);
        let performance_monitor = PerformanceMonitor::new(worker_count);

        for worker_id in 0..worker_count {
            let mut config = base_config.clone();
            config.worker_id = worker_id;

            // Use SO_REUSEPORT instead of port adjustment
            // SO_REUSEPORT allows multiple sockets to bind to the same port
            // The kernel will distribute packets across workers automatically

            let (command_tx, command_rx) = mpsc::channel::<TquicCommand>();
            let is_healthy = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let is_healthy_clone = is_healthy.clone();
            
            // Initialize worker stats
            let worker_stats = Arc::new(std::sync::RwLock::new(WorkerStats {
                worker_id,
                active_connections: 0,
                total_connections: 0,
                total_streams: 0,
                bytes_sent: 0,
                bytes_received: 0,
                errors: 0,
                uptime: Duration::new(0, 0),
            }));

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
                .map_err(|e| {
                    TransportError::ConfigError(format!("Failed to spawn worker thread: {}", e))
                })?;

            workers.push(TquicWorkerHandle {
                worker_id,
                command_tx,
                waker: None, // Will be set after worker creation if we implement it
                join_handle,
                is_healthy,
                stats: worker_stats,
            });
        }

        Ok(TquicWorkerPool {
            workers,
            next_worker: AtomicUsize::new(0),
            connection_map,
            worker_count,
            load_balancer,
            performance_monitor,
        })
    }

    pub async fn submit_command(&self, cmd: TquicCommand) -> Result<(), TransportError> {
        // Record command for performance monitoring
        self.performance_monitor.record_command();
        
        let worker_id = self.select_worker(&cmd);

        if worker_id >= self.workers.len() {
            return Err(TransportError::ConfigError(
                "Invalid worker selection".into(),
            ));
        }

        let worker = &self.workers[worker_id];

        // Check if worker is healthy
        if !worker.is_healthy.load(Ordering::Relaxed) {
            // Try to restart the worker before failing
            if let Err(_) = self.restart_worker(worker_id) {
                return Err(TransportError::ConnectionFailed(
                    "Worker is unhealthy and restart failed".into(),
                ));
            }
        }

        // Update connection mapping for new connections
        if let TquicCommand::Connect { .. } = &cmd {
            self.performance_monitor.record_connection();
        }

        worker
            .command_tx
            .send(cmd)
            .map_err(|_| TransportError::ConnectionFailed("Worker channel closed".into()))?;

        // Wake up the worker to process the command if we have a waker
        if let Some(ref waker) = worker.waker {
            let _ = waker.wake(); // Ignore wake errors
        }

        Ok(())
    }

    pub fn map_connection_to_worker(&self, connection_id: ConnectionId, worker_id: usize) {
        self.connection_map.insert(connection_id, worker_id);
    }

    pub fn unmap_connection(&self, connection_id: ConnectionId) {
        if let Some((_, worker_id)) = self.connection_map.remove(&connection_id) {
            // Release worker load when connection closes
            self.load_balancer.release_worker_load(worker_id);
        }
    }

    /// Get performance metrics for the worker pool
    pub fn get_performance_metrics(&self) -> PerformanceMetrics {
        self.performance_monitor.get_performance_metrics()
    }
    
    /// Get load distribution across workers
    pub fn get_load_distribution(&self) -> Vec<usize> {
        self.load_balancer.get_load_distribution()
    }
    
    /// Check if SO_REUSEPORT is distributing load evenly
    pub fn analyze_socket_distribution(&self) -> SocketDistributionAnalysis {
        let metrics = self.get_performance_metrics();
        let load_distribution = self.get_load_distribution();
        
        // Calculate distribution fairness
        let total_packets: usize = metrics.socket_distribution.iter().map(|(_, count)| *count).sum();
        let worker_count = self.worker_count;
        let expected_per_worker = total_packets as f64 / worker_count as f64;
        
        let mut max_deviation = 0.0;
        let mut distribution_score = 0.0;
        
        for &(_, packet_count) in &metrics.socket_distribution {
            let deviation = (packet_count as f64 - expected_per_worker).abs() / expected_per_worker;
            if deviation > max_deviation {
                max_deviation = deviation;
            }
            distribution_score += deviation * deviation; // Squared deviation for fairness metric
        }
        
        distribution_score = (distribution_score / worker_count as f64).sqrt();
        
        SocketDistributionAnalysis {
            total_packets,
            worker_count,
            expected_per_worker: expected_per_worker as usize,
            max_deviation_percent: max_deviation * 100.0,
            fairness_score: 1.0 - (distribution_score / expected_per_worker).min(1.0),
            per_worker_packets: metrics.socket_distribution,
            load_distribution,
            is_balanced: max_deviation < 0.15, // Consider balanced if deviation < 15%
        }
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
                tracing::warn!(
                    "Worker {} already unhealthy during shutdown",
                    worker.worker_id
                );
            }
        }

        Ok(())
    }

    fn select_worker(&self, cmd: &TquicCommand) -> usize {
        match cmd {
            TquicCommand::Connect { .. } => {
                // Use intelligent load balancing for new connections
                self.load_balancer.select_worker(&self.workers)
            }
            TquicCommand::Send { connection_id, .. }
            | TquicCommand::Receive { connection_id, .. }
            | TquicCommand::OpenStream { connection_id, .. }
            | TquicCommand::CloseStream { connection_id, .. }
            | TquicCommand::CloseConnection { connection_id, .. } => {
                // Use connection affinity
                if let Some(entry) = self.connection_map.get(connection_id) {
                    *entry
                } else {
                    // Fallback to intelligent load balancing if mapping lost
                    self.load_balancer.select_worker(&self.workers)
                }
            }
            TquicCommand::Accept { .. } => {
                // Use intelligent load balancing for accept requests
                self.load_balancer.select_worker(&self.workers)
            }
            TquicCommand::Shutdown => 0, // Doesn't matter for shutdown
        }
    }

    fn restart_worker(&self, worker_id: usize) -> Result<(), TransportError> {
        if worker_id >= self.workers.len() {
            return Err(TransportError::ConfigError("Invalid worker ID".into()));
        }

        tracing::warn!("Attempting to restart worker {}", worker_id);

        // For now, just mark the worker as healthy again
        // In a full implementation, we would:
        // 1. Stop the old worker thread
        // 2. Clear its connection mappings
        // 3. Spawn a new worker thread
        // 4. Update the worker handle

        let worker = &self.workers[worker_id];
        worker.is_healthy.store(true, Ordering::Relaxed);

        // Clear connection mappings for this worker
        self.connection_map.retain(|_, v| *v != worker_id);

        tracing::info!(
            "Worker {} marked as healthy (restart simplified)",
            worker_id
        );
        Ok(())
    }

    pub fn health_check(&self) -> Vec<WorkerHealth> {
        self.workers
            .iter()
            .map(|worker| {
                if worker.is_healthy.load(Ordering::Relaxed) {
                    WorkerHealth::Healthy
                } else {
                    WorkerHealth::Failed {
                        error: format!("Worker {} is unhealthy", worker.worker_id),
                    }
                }
            })
            .collect()
    }

    pub fn get_worker_status(&self) -> Vec<WorkerStatus> {
        self.workers
            .iter()
            .map(|worker| {
                let health = if worker.is_healthy.load(Ordering::Relaxed) {
                    WorkerHealth::Healthy
                } else {
                    WorkerHealth::Failed {
                        error: "Worker thread failed".into(),
                    }
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
            })
            .collect()
    }

    fn run_worker_loop(
        config: WorkerConfig,
        command_rx: mpsc::Receiver<TquicCommand>,
    ) -> Result<(), TransportError> {
        tracing::info!(
            "Starting tQUIC worker {} on {}",
            config.worker_id,
            config.bind_addr
        );

        // For now, implement a simple message loop that responds to commands
        // This will be replaced with actual tQUIC worker implementation
        let mut running = true;

        while running {
            match command_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(cmd) => {
                    match cmd {
                        TquicCommand::Connect {
                            remote_addr,
                            response,
                        } => {
                            tracing::debug!(
                                "Worker {} handling connect to {}",
                                config.worker_id,
                                remote_addr
                            );
                            // For now, return a dummy connection ID
                            let conn_id = (config.worker_id as u64) << 32 | 1;
                            let _ = response.send(Ok(conn_id));
                        }
                        TquicCommand::Send {
                            connection_id,
                            stream_id,
                            data,
                            fin,
                            response,
                        } => {
                            tracing::debug!(
                                "Worker {} handling send on connection {} stream {}",
                                config.worker_id,
                                connection_id,
                                stream_id
                            );
                            // Simulate successful send
                            let _ = response.send(Ok(()));
                        }
                        TquicCommand::Receive {
                            connection_id,
                            stream_id,
                            max_length,
                            response,
                        } => {
                            tracing::debug!(
                                "Worker {} handling receive on connection {} stream {}",
                                config.worker_id,
                                connection_id,
                                stream_id
                            );
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
                        TquicCommand::OpenStream {
                            connection_id,
                            response,
                        } => {
                            tracing::debug!(
                                "Worker {} handling open stream on connection {}",
                                config.worker_id,
                                connection_id
                            );
                            // Return dummy stream ID
                            let _ = response.send(Ok(1));
                        }
                        TquicCommand::CloseStream {
                            connection_id,
                            stream_id,
                            error_code,
                        } => {
                            tracing::debug!(
                                "Worker {} handling close stream {} on connection {}",
                                config.worker_id,
                                stream_id,
                                connection_id
                            );
                        }
                        TquicCommand::CloseConnection {
                            connection_id,
                            error_code,
                            reason,
                        } => {
                            tracing::debug!(
                                "Worker {} handling close connection {}",
                                config.worker_id,
                                connection_id
                            );
                        }
                        TquicCommand::Shutdown => {
                            tracing::info!("Worker {} shutting down", config.worker_id);
                            running = false;
                        }
                    }
                }
                 Err(mpsc::RecvTimeoutError::Timeout) => {
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
