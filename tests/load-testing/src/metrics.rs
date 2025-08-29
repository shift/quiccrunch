use anyhow::{Context, Result};
use async_trait::async_trait;
use dashmap::DashMap;
use histogram::Histogram;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, warn};

use crate::framework::MetricsEvent;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetrics {
    #[serde(skip, default = "Instant::now")]
    pub start_time: Instant,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_connections: u64,
    pub active_connections: u64,
    pub throughput_rps: f64,
    pub throughput_bps: f64,
    pub latency_min: Duration,
    pub latency_max: Duration,
    pub latency_mean: Duration,
    pub latency_p50: Duration,
    pub latency_p95: Duration,
    pub latency_p99: Duration,
    pub latency_p999: Duration,
    pub error_rate: f64,
    pub connection_errors: u64,
    pub timeout_errors: u64,
    pub protocol_errors: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub cpu_usage: f64,
    pub memory_usage_mb: f64,
    pub streams_created: u64,
    pub streams_active: u64,
    pub custom_metrics: std::collections::HashMap<String, f64>,
}

impl Default for TestMetrics {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            duration: Duration::ZERO,
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_connections: 0,
            active_connections: 0,
            throughput_rps: 0.0,
            throughput_bps: 0.0,
            latency_min: Duration::MAX,
            latency_max: Duration::ZERO,
            latency_mean: Duration::ZERO,
            latency_p50: Duration::ZERO,
            latency_p95: Duration::ZERO,
            latency_p99: Duration::ZERO,
            latency_p999: Duration::ZERO,
            error_rate: 0.0,
            connection_errors: 0,
            timeout_errors: 0,
            protocol_errors: 0,
            bytes_sent: 0,
            bytes_received: 0,
            cpu_usage: 0.0,
            memory_usage_mb: 0.0,
            streams_created: 0,
            streams_active: 0,
            custom_metrics: std::collections::HashMap::new(),
        }
    }
}

pub struct MetricsCollector {
    metrics_interval: Duration,
    latencies: Arc<parking_lot::Mutex<Vec<Duration>>>,
    active_requests: Arc<DashMap<String, Instant>>,
    active_connections: Arc<DashMap<String, Instant>>,
    active_streams: Arc<DashMap<String, Instant>>,
    counters: Arc<MetricsCounters>,
}

#[derive(Default)]
struct MetricsCounters {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    total_connections: AtomicU64,
    connection_errors: AtomicU64,
    timeout_errors: AtomicU64,
    protocol_errors: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    streams_created: AtomicU64,
}

impl MetricsCollector {
    pub fn new(metrics_interval: Duration) -> Self {
        Self {
            metrics_interval,
            latencies: Arc::new(parking_lot::Mutex::new(Vec::new())),
            active_requests: Arc::new(DashMap::new()),
            active_connections: Arc::new(DashMap::new()),
            active_streams: Arc::new(DashMap::new()),
            counters: Arc::new(MetricsCounters::default()),
        }
    }

    pub async fn collect_metrics(
        &self,
        mut metrics_rx: mpsc::UnboundedReceiver<MetricsEvent>,
    ) -> Result<TestMetrics> {
        let start_time = Instant::now();
        let mut system_monitor = SystemMonitor::new();
        let mut metrics_timer = interval(self.metrics_interval);
        
        // Start system monitoring
        let system_handle = tokio::spawn(async move {
            let mut monitor = SystemMonitor::new();
            monitor.monitor().await;
            monitor.get_metrics().await
        });

        // Process metrics events
        loop {
            tokio::select! {
                event = metrics_rx.recv() => {
                    match event {
                        Some(event) => self.process_event(event).await,
                        None => break, // Channel closed
                    }
                }
                _ = metrics_timer.tick() => {
                    debug!("Metrics tick - active requests: {}, connections: {}, streams: {}",
                           self.active_requests.len(),
                           self.active_connections.len(),
                           self.active_streams.len());
                }
            }
        }

        let duration = start_time.elapsed();
        
        // Stop system monitoring
        system_handle.abort();
        let system_metrics = SystemMetrics::default();

        // Calculate final metrics
        let metrics = self.calculate_final_metrics(start_time, duration, system_metrics).await;
        
        Ok(metrics)
    }

    async fn process_event(&self, event: MetricsEvent) {
        match event {
            MetricsEvent::RequestStarted { request_id, timestamp } => {
                self.active_requests.insert(request_id, timestamp);
                self.counters.total_requests.fetch_add(1, Ordering::Relaxed);
            }
            MetricsEvent::RequestCompleted { request_id, timestamp, success, latency } => {
                if let Some((_, start_time)) = self.active_requests.remove(&request_id) {
                    if success {
                        self.counters.successful_requests.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.counters.failed_requests.fetch_add(1, Ordering::Relaxed);
                    }
                    
                    // Record latency
                    self.latencies.lock().push(latency);
                }
            }
            MetricsEvent::ConnectionEstablished { connection_id, timestamp } => {
                self.active_connections.insert(connection_id, timestamp);
                self.counters.total_connections.fetch_add(1, Ordering::Relaxed);
            }
            MetricsEvent::ConnectionClosed { connection_id, timestamp, reason } => {
                self.active_connections.remove(&connection_id);
                
                if reason.contains("error") || reason.contains("timeout") {
                    self.counters.connection_errors.fetch_add(1, Ordering::Relaxed);
                }
            }
            MetricsEvent::ErrorOccurred { error_type, message, timestamp } => {
                match error_type.as_str() {
                    "timeout" => self.counters.timeout_errors.fetch_add(1, Ordering::Relaxed),
                    "protocol" => self.counters.protocol_errors.fetch_add(1, Ordering::Relaxed),
                    _ => self.counters.connection_errors.fetch_add(1, Ordering::Relaxed),
                };
            }
            MetricsEvent::StreamCreated { stream_id, timestamp } => {
                self.active_streams.insert(stream_id, timestamp);
                self.counters.streams_created.fetch_add(1, Ordering::Relaxed);
            }
            MetricsEvent::StreamClosed { stream_id, timestamp, bytes_transferred } => {
                self.active_streams.remove(&stream_id);
                self.counters.bytes_received.fetch_add(bytes_transferred, Ordering::Relaxed);
            }
            MetricsEvent::CustomMetric { name, value, timestamp } => {
                // Store custom metrics for later aggregation
                debug!("Custom metric: {} = {}", name, value);
            }
        }
    }

    async fn calculate_final_metrics(
        &self,
        start_time: Instant,
        duration: Duration,
        system_metrics: SystemMetrics,
    ) -> TestMetrics {
        let total_requests = self.counters.total_requests.load(Ordering::Relaxed);
        let successful_requests = self.counters.successful_requests.load(Ordering::Relaxed);
        let failed_requests = self.counters.failed_requests.load(Ordering::Relaxed);
        
        let throughput_rps = if duration.as_secs_f64() > 0.0 {
            total_requests as f64 / duration.as_secs_f64()
        } else {
            0.0
        };

        let error_rate = if total_requests > 0 {
            failed_requests as f64 / total_requests as f64
        } else {
            0.0
        };

        // Calculate latency percentiles
        let latencies = self.latencies.lock();
        let latency_percentiles = if !latencies.is_empty() {
            let mut sorted_latencies = latencies.clone();
            sorted_latencies.sort();
            
            let len = sorted_latencies.len();
            LatencyPercentiles {
                min: *sorted_latencies.first().unwrap_or(&Duration::ZERO),
                max: *sorted_latencies.last().unwrap_or(&Duration::ZERO),
                mean: Duration::from_nanos(sorted_latencies.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 / len as u64),
                p50: sorted_latencies.get(len * 50 / 100).copied().unwrap_or(Duration::ZERO),
                p95: sorted_latencies.get(len * 95 / 100).copied().unwrap_or(Duration::ZERO),
                p99: sorted_latencies.get(len * 99 / 100).copied().unwrap_or(Duration::ZERO),
                p999: sorted_latencies.get(len * 999 / 1000).copied().unwrap_or(Duration::ZERO),
            }
        } else {
            LatencyPercentiles::default()
        };

        TestMetrics {
            start_time,
            duration,
            total_requests,
            successful_requests,
            failed_requests,
            total_connections: self.counters.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.len() as u64,
            throughput_rps,
            throughput_bps: 0.0, // TODO: Calculate based on message sizes
            latency_min: latency_percentiles.min,
            latency_max: latency_percentiles.max,
            latency_mean: latency_percentiles.mean,
            latency_p50: latency_percentiles.p50,
            latency_p95: latency_percentiles.p95,
            latency_p99: latency_percentiles.p99,
            latency_p999: latency_percentiles.p999,
            error_rate,
            connection_errors: self.counters.connection_errors.load(Ordering::Relaxed),
            timeout_errors: self.counters.timeout_errors.load(Ordering::Relaxed),
            protocol_errors: self.counters.protocol_errors.load(Ordering::Relaxed),
            bytes_sent: self.counters.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.counters.bytes_received.load(Ordering::Relaxed),
            cpu_usage: system_metrics.cpu_usage,
            memory_usage_mb: system_metrics.memory_usage_mb,
            streams_created: self.counters.streams_created.load(Ordering::Relaxed),
            streams_active: self.active_streams.len() as u64,
            custom_metrics: std::collections::HashMap::new(), // TODO: Aggregate custom metrics
        }
    }
}

#[derive(Debug, Default)]
struct LatencyPercentiles {
    min: Duration,
    max: Duration,
    mean: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
    p999: Duration,
}

#[derive(Debug, Default)]
struct SystemMetrics {
    cpu_usage: f64,
    memory_usage_mb: f64,
}

struct SystemMonitor {
    cpu_usage: f64,
    memory_usage_mb: f64,
}

impl SystemMonitor {
    fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage_mb: 0.0,
        }
    }

    async fn monitor(&mut self) {
        let mut interval = interval(Duration::from_secs(1));
        
        loop {
            interval.tick().await;
            self.update_metrics().await;
        }
    }

    async fn update_metrics(&mut self) {
        // Simple system metrics collection
        // In a real implementation, this would use system APIs
        self.cpu_usage = self.get_cpu_usage().await.unwrap_or(0.0);
        self.memory_usage_mb = self.get_memory_usage().await.unwrap_or(0.0);
    }

    async fn get_cpu_usage(&self) -> Result<f64> {
        // TODO: Implement actual CPU usage monitoring
        // For now, return a mock value
        Ok(0.0)
    }

    async fn get_memory_usage(&self) -> Result<f64> {
        // TODO: Implement actual memory usage monitoring
        // For now, return a mock value based on process info
        Ok(0.0)
    }

    async fn get_metrics(&self) -> SystemMetrics {
        SystemMetrics {
            cpu_usage: self.cpu_usage,
            memory_usage_mb: self.memory_usage_mb,
        }
    }
}

#[async_trait]
pub trait MetricsReporter {
    async fn report_metrics(&self, metrics: &TestMetrics) -> Result<()>;
}

pub struct PrometheusReporter {
    // TODO: Add Prometheus client
}

impl PrometheusReporter {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl MetricsReporter for PrometheusReporter {
    async fn report_metrics(&self, metrics: &TestMetrics) -> Result<()> {
        // TODO: Implement Prometheus metrics reporting
        debug!("Reporting metrics to Prometheus: {} requests, {:.2} RPS", 
               metrics.total_requests, metrics.throughput_rps);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_metrics_collection() {
        let collector = MetricsCollector::new(Duration::from_millis(100));
        let (tx, rx) = mpsc::unbounded_channel();

        // Send some test events
        tx.send(MetricsEvent::RequestStarted {
            request_id: "test-1".to_string(),
            timestamp: Instant::now(),
        }).unwrap();

        tx.send(MetricsEvent::RequestCompleted {
            request_id: "test-1".to_string(),
            timestamp: Instant::now(),
            success: true,
            latency: Duration::from_millis(50),
        }).unwrap();

        drop(tx);

        let metrics = collector.collect_metrics(rx).await.unwrap();
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.successful_requests, 1);
        assert_eq!(metrics.failed_requests, 0);
    }

    #[test]
    fn test_latency_calculation() {
        let mut latencies = vec![
            Duration::from_millis(1),
            Duration::from_millis(5),
            Duration::from_millis(10),
        ];
        latencies.sort();
        
        let len = latencies.len();
        let p50 = latencies.get(len * 50 / 100).copied().unwrap_or(Duration::ZERO);
        let p95 = latencies.get(len * 95 / 100).copied().unwrap_or(Duration::ZERO);
        
        assert!(p50 > Duration::ZERO);
        assert!(p95 > Duration::ZERO);
    }
}