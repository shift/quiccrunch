/// Comprehensive Metrics Collection Framework for RPC Benchmarking
///
/// This module provides high-precision performance measurement utilities for
/// benchmarking QuicCrunch and gRPC implementations across all RPC patterns.

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// High-precision latency measurement using HDR histogram
pub mod latency {
    use hdrhistogram::Histogram;
    use std::time::{Duration, Instant};

    /// Latency measurement with HDR histogram for accurate percentiles
    pub struct LatencyMeter {
        histogram: Histogram<u64>,
        start_times: Vec<Instant>,
    }

    impl LatencyMeter {
        /// Create a new latency meter with microsecond precision
        pub fn new() -> Self {
            Self {
                histogram: Histogram::<u64>::new_with_bounds(1, 10_000_000, 3).unwrap(), // 1ns to 10s
                start_times: Vec::new(),
            }
        }

        /// Start measuring latency for an operation
        pub fn start_measurement(&mut self) -> usize {
            let index = self.start_times.len();
            self.start_times.push(Instant::now());
            index
        }

        /// End measurement and record latency
        pub fn end_measurement(&mut self, index: usize) {
            if let Some(start_time) = self.start_times.get(index) {
                let latency = start_time.elapsed();
                let latency_ns = latency.as_nanos() as u64;
                let _ = self.histogram.record(latency_ns);
            }
        }

        /// Record a latency measurement directly
        pub fn record_latency(&mut self, latency: Duration) {
            let latency_ns = latency.as_nanos() as u64;
            let _ = self.histogram.record(latency_ns);
        }

        /// Get latency statistics
        pub fn statistics(&self) -> LatencyStats {
            LatencyStats {
                count: self.histogram.len(),
                min_ns: self.histogram.min(),
                max_ns: self.histogram.max(),
                mean_ns: self.histogram.mean() as u64,
                p50_ns: self.histogram.value_at_percentile(50.0),
                p90_ns: self.histogram.value_at_percentile(90.0),
                p95_ns: self.histogram.value_at_percentile(95.0),
                p99_ns: self.histogram.value_at_percentile(99.0),
                p999_ns: self.histogram.value_at_percentile(99.9),
            }
        }

        /// Reset all measurements
        pub fn reset(&mut self) {
            self.histogram.clear();
            self.start_times.clear();
        }
    }

    /// Latency statistics from HDR histogram
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LatencyStats {
        pub count: u64,
        pub min_ns: u64,
        pub max_ns: u64,
        pub mean_ns: u64,
        pub p50_ns: u64,
        pub p90_ns: u64,
        pub p95_ns: u64,
        pub p99_ns: u64,
        pub p999_ns: u64,
    }

    impl LatencyStats {
        /// Convert nanoseconds to milliseconds for display
        pub fn to_ms(&self) -> LatencyStatsMs {
            LatencyStatsMs {
                count: self.count,
                min_ms: self.min_ns as f64 / 1_000_000.0,
                max_ms: self.max_ns as f64 / 1_000_000.0,
                mean_ms: self.mean_ns as f64 / 1_000_000.0,
                p50_ms: self.p50_ns as f64 / 1_000_000.0,
                p90_ms: self.p90_ns as f64 / 1_000_000.0,
                p95_ms: self.p95_ns as f64 / 1_000_000.0,
                p99_ms: self.p99_ns as f64 / 1_000_000.0,
                p999_ms: self.p999_ns as f64 / 1_000_000.0,
            }
        }
    }

    /// Latency statistics in milliseconds for display
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LatencyStatsMs {
        pub count: u64,
        pub min_ms: f64,
        pub max_ms: f64,
        pub mean_ms: f64,
        pub p50_ms: f64,
        pub p90_ms: f64,
        pub p95_ms: f64,
        pub p99_ms: f64,
        pub p999_ms: f64,
    }
}

/// Throughput and bandwidth measurement
pub mod throughput {
    use std::time::{Duration, Instant};
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Throughput measurement with atomic counters
    pub struct ThroughputMeter {
        messages_sent: AtomicU64,
        messages_received: AtomicU64,
        bytes_sent: AtomicU64,
        bytes_received: AtomicU64,
        start_time: Instant,
    }

    impl ThroughputMeter {
        pub fn new() -> Self {
            Self {
                messages_sent: AtomicU64::new(0),
                messages_received: AtomicU64::new(0),
                bytes_sent: AtomicU64::new(0),
                bytes_received: AtomicU64::new(0),
                start_time: Instant::now(),
            }
        }

        /// Record a message sent
        pub fn record_message_sent(&self, size_bytes: usize) {
            self.messages_sent.fetch_add(1, Ordering::Relaxed);
            self.bytes_sent.fetch_add(size_bytes as u64, Ordering::Relaxed);
        }

        /// Record a message received
        pub fn record_message_received(&self, size_bytes: usize) {
            self.messages_received.fetch_add(1, Ordering::Relaxed);
            self.bytes_received.fetch_add(size_bytes as u64, Ordering::Relaxed);
        }

        /// Get throughput statistics
        pub fn statistics(&self) -> ThroughputStats {
            let elapsed = self.start_time.elapsed();
            let elapsed_secs = elapsed.as_secs_f64();

            let messages_sent = self.messages_sent.load(Ordering::Relaxed);
            let messages_received = self.messages_received.load(Ordering::Relaxed);
            let bytes_sent = self.bytes_sent.load(Ordering::Relaxed);
            let bytes_received = self.bytes_received.load(Ordering::Relaxed);

            ThroughputStats {
                elapsed_seconds: elapsed_secs,
                messages_sent_per_second: messages_sent as f64 / elapsed_secs,
                messages_received_per_second: messages_received as f64 / elapsed_secs,
                total_messages_sent: messages_sent,
                total_messages_received: messages_received,
                bytes_sent_per_second: bytes_sent as f64 / elapsed_secs,
                bytes_received_per_second: bytes_received as f64 / elapsed_secs,
                total_bytes_sent: bytes_sent,
                total_bytes_received: bytes_received,
                bandwidth_mbps_sent: (bytes_sent as f64 * 8.0) / (elapsed_secs * 1_000_000.0),
                bandwidth_mbps_received: (bytes_received as f64 * 8.0) / (elapsed_secs * 1_000_000.0),
            }
        }

        /// Reset all counters
        pub fn reset(&self) {
            self.messages_sent.store(0, Ordering::Relaxed);
            self.messages_received.store(0, Ordering::Relaxed);
            self.bytes_sent.store(0, Ordering::Relaxed);
            self.bytes_received.store(0, Ordering::Relaxed);
        }
    }

    /// Throughput statistics
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ThroughputStats {
        pub elapsed_seconds: f64,
        pub messages_sent_per_second: f64,
        pub messages_received_per_second: f64,
        pub total_messages_sent: u64,
        pub total_messages_received: u64,
        pub bytes_sent_per_second: f64,
        pub bytes_received_per_second: f64,
        pub total_bytes_sent: u64,
        pub total_bytes_received: u64,
        pub bandwidth_mbps_sent: f64,
        pub bandwidth_mbps_received: f64,
    }
}

/// System resource monitoring (CPU, memory, network)
pub mod resources {
    use std::sync::atomic::{AtomicU64, Ordering};
    use sysinfo::{System, SystemExt, ProcessExt, NetworkExt};

    /// System resource monitor
    pub struct ResourceMonitor {
        system: System,
        initial_network_rx: u64,
        initial_network_tx: u64,
        start_time: std::time::Instant,
        cpu_samples: Vec<f32>,
        memory_samples: Vec<u64>,
    }

    impl ResourceMonitor {
        pub fn new() -> Self {
            let mut system = System::new_all();
            system.refresh_all();

            let network_data = system.networks();
            let mut initial_network_rx = 0;
            let mut initial_network_tx = 0;

            for (_name, network) in network_data {
                initial_network_rx += network.received();
                initial_network_tx += network.transmitted();
            }

            Self {
                system,
                initial_network_rx,
                initial_network_tx,
                start_time: std::time::Instant::now(),
                cpu_samples: Vec::new(),
                memory_samples: Vec::new(),
            }
        }

        /// Take a resource measurement snapshot
        pub fn take_snapshot(&mut self) {
            self.system.refresh_all();

            // CPU usage
            let cpu_usage = self.system.global_cpu_info().cpu_usage();
            self.cpu_samples.push(cpu_usage);

            // Memory usage
            let memory_used = self.system.used_memory();
            self.memory_samples.push(memory_used);
        }

        /// Get resource statistics
        pub fn statistics(&self) -> ResourceStats {
            let elapsed = self.start_time.elapsed().as_secs_f64();

            // Calculate averages
            let avg_cpu = if self.cpu_samples.is_empty() {
                0.0
            } else {
                self.cpu_samples.iter().sum::<f32>() / self.cpu_samples.len() as f32
            };

            let avg_memory_mb = if self.memory_samples.is_empty() {
                0.0
            } else {
                self.memory_samples.iter().sum::<u64>() as f64 / self.memory_samples.len() as f64 / 1_048_576.0
            };

            // Network statistics
            let mut current_network_rx = 0;
            let mut current_network_tx = 0;

            for (_name, network) in self.system.networks() {
                current_network_rx += network.received();
                current_network_tx += network.transmitted();
            }

            let network_rx_bytes = current_network_rx.saturating_sub(self.initial_network_rx);
            let network_tx_bytes = current_network_tx.saturating_sub(self.initial_network_tx);

            ResourceStats {
                elapsed_seconds: elapsed,
                avg_cpu_percent: avg_cpu,
                avg_memory_mb: avg_memory_mb,
                peak_memory_mb: self.memory_samples.iter().max().copied().unwrap_or(0) as f64 / 1_048_576.0,
                network_rx_mbps: (network_rx_bytes as f64 * 8.0) / (elapsed * 1_000_000.0),
                network_tx_mbps: (network_tx_bytes as f64 * 8.0) / (elapsed * 1_000_000.0),
                total_network_rx_mb: network_rx_bytes as f64 / 1_048_576.0,
                total_network_tx_mb: network_tx_bytes as f64 / 1_048_576.0,
            }
        }
    }

    /// Resource usage statistics
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ResourceStats {
        pub elapsed_seconds: f64,
        pub avg_cpu_percent: f32,
        pub avg_memory_mb: f64,
        pub peak_memory_mb: f64,
        pub network_rx_mbps: f64,
        pub network_tx_mbps: f64,
        pub total_network_rx_mb: f64,
        pub total_network_tx_mb: f64,
    }
}

/// Error rate and success tracking
pub mod errors {
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Error rate measurement
    pub struct ErrorMeter {
        total_operations: AtomicU64,
        successful_operations: AtomicU64,
        connection_errors: AtomicU64,
        timeout_errors: AtomicU64,
        protocol_errors: AtomicU64,
    }

    impl ErrorMeter {
        pub fn new() -> Self {
            Self {
                total_operations: AtomicU64::new(0),
                successful_operations: AtomicU64::new(0),
                connection_errors: AtomicU64::new(0),
                timeout_errors: AtomicU64::new(0),
                protocol_errors: AtomicU64::new(0),
            }
        }

        /// Record a successful operation
        pub fn record_success(&self) {
            self.total_operations.fetch_add(1, Ordering::Relaxed);
            self.successful_operations.fetch_add(1, Ordering::Relaxed);
        }

        /// Record a connection error
        pub fn record_connection_error(&self) {
            self.total_operations.fetch_add(1, Ordering::Relaxed);
            self.connection_errors.fetch_add(1, Ordering::Relaxed);
        }

        /// Record a timeout error
        pub fn record_timeout_error(&self) {
            self.total_operations.fetch_add(1, Ordering::Relaxed);
            self.timeout_errors.fetch_add(1, Ordering::Relaxed);
        }

        /// Record a protocol error
        pub fn record_protocol_error(&self) {
            self.total_operations.fetch_add(1, Ordering::Relaxed);
            self.protocol_errors.fetch_add(1, Ordering::Relaxed);
        }

        /// Get error statistics
        pub fn statistics(&self) -> ErrorStats {
            let total = self.total_operations.load(Ordering::Relaxed);
            let successful = self.successful_operations.load(Ordering::Relaxed);
            let failed = total - successful;

            ErrorStats {
                total_operations: total,
                successful_operations: successful,
                failed_operations: failed,
                success_rate_percent: if total > 0 { (successful as f64 / total as f64) * 100.0 } else { 0.0 },
                connection_errors: self.connection_errors.load(Ordering::Relaxed),
                timeout_errors: self.timeout_errors.load(Ordering::Relaxed),
                protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
            }
        }
    }

    /// Error statistics
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ErrorStats {
        pub total_operations: u64,
        pub successful_operations: u64,
        pub failed_operations: u64,
        pub success_rate_percent: f64,
        pub connection_errors: u64,
        pub timeout_errors: u64,
        pub protocol_errors: u64,
    }
}

/// Comprehensive benchmark metrics aggregator
pub struct BenchmarkMetrics {
    pub latency_meter: latency::LatencyMeter,
    pub throughput_meter: throughput::ThroughputMeter,
    pub resource_monitor: resources::ResourceMonitor,
    pub error_meter: errors::ErrorMeter,
    pub test_start: Instant,
    pub test_name: String,
    pub protocol: String,
}

impl BenchmarkMetrics {
    pub fn new(test_name: String, protocol: String) -> Self {
        Self {
            latency_meter: latency::LatencyMeter::new(),
            throughput_meter: throughput::ThroughputMeter::new(),
            resource_monitor: resources::ResourceMonitor::new(),
            error_meter: errors::ErrorMeter::new(),
            test_start: Instant::now(),
            test_name,
            protocol,
        }
    }

    /// Take a resource measurement snapshot
    pub fn take_resource_snapshot(&mut self) {
        self.resource_monitor.take_snapshot();
    }

    /// Get comprehensive benchmark results
    pub fn results(&self) -> BenchmarkResults {
        BenchmarkResults {
            test_name: self.test_name.clone(),
            protocol: self.protocol.clone(),
            duration_seconds: self.test_start.elapsed().as_secs_f64(),
            latency_stats: self.latency_meter.statistics(),
            throughput_stats: self.throughput_meter.statistics(),
            resource_stats: self.resource_monitor.statistics(),
            error_stats: self.error_meter.statistics(),
        }
    }
}

/// Complete benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub test_name: String,
    pub protocol: String,
    pub duration_seconds: f64,
    pub latency_stats: latency::LatencyStats,
    pub throughput_stats: throughput::ThroughputStats,
    pub resource_stats: resources::ResourceStats,
    pub error_stats: errors::ErrorStats,
}

/// Statistical analysis utilities
pub mod statistics {
    use std::collections::HashMap;

    /// Calculate confidence interval for a sample
    pub fn confidence_interval(values: &[f64], confidence_level: f64) -> (f64, f64) {
        if values.is_empty() {
            return (0.0, 0.0);
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / (values.len() - 1) as f64;
        let std_dev = variance.sqrt();

        // For 95% confidence level, z-score ≈ 1.96
        let z_score = match confidence_level {
            0.95 => 1.96,
            0.99 => 2.576,
            0.90 => 1.645,
            _ => 1.96,
        };

        let margin = z_score * std_dev / (values.len() as f64).sqrt();
        (mean - margin, mean + margin)
    }

    /// Perform t-test to compare two samples
    pub fn t_test(sample1: &[f64], sample2: &[f64]) -> f64 {
        if sample1.is_empty() || sample2.is_empty() {
            return 0.0;
        }

        let mean1 = sample1.iter().sum::<f64>() / sample1.len() as f64;
        let mean2 = sample2.iter().sum::<f64>() / sample2.len() as f64;

        let var1 = sample1.iter()
            .map(|x| (x - mean1).powi(2))
            .sum::<f64>() / (sample1.len() - 1) as f64;
        let var2 = sample2.iter()
            .map(|x| (x - mean2).powi(2))
            .sum::<f64>() / (sample2.len() - 1) as f64;

        let pooled_std_dev = ((var1 / sample1.len() as f64) + (var2 / sample2.len() as f64)).sqrt();

        if pooled_std_dev == 0.0 {
            return 0.0;
        }

        (mean1 - mean2) / pooled_std_dev
    }

    /// Calculate statistical significance (p-value approximation)
    pub fn statistical_significance(t_statistic: f64, degrees_of_freedom: usize) -> f64 {
        // Simplified p-value calculation using t-distribution approximation
        // For production use, consider using a proper statistical library
        let abs_t = t_statistic.abs();

        // Approximation for large degrees of freedom
        if degrees_of_freedom > 30 {
            // Use normal distribution approximation
            let p_value = 2.0 * (1.0 - normal_cdf(abs_t));
            p_value.min(1.0)
        } else {
            // Conservative estimate
            if abs_t > 2.0 { 0.05 } else { 0.5 }
        }
    }

    /// Normal cumulative distribution function approximation
    fn normal_cdf(x: f64) -> f64 {
        // Abramowitz & Stegun approximation
        let a1 =  0.254829592;
        let a2 = -0.284496736;
        let a3 =  1.421413741;
        let a4 = -1.453152027;
        let a5 =  1.061405429;
        let p  =  0.3275911;

        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let x = x.abs();

        let t = 1.0 / (1.0 + p * x);
        let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x * x).exp();

        0.5 * (1.0 + sign * y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_latency_meter() {
        let mut meter = latency::LatencyMeter::new();

        // Record some latencies
        meter.record_latency(Duration::from_millis(10));
        meter.record_latency(Duration::from_millis(20));
        meter.record_latency(Duration::from_millis(30));

        let stats = meter.statistics();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.min_ns, 10_000_000); // 10ms in nanoseconds
        assert_eq!(stats.max_ns, 30_000_000); // 30ms in nanoseconds
    }

    #[test]
    fn test_throughput_meter() {
        let meter = throughput::ThroughputMeter::new();

        // Record some messages
        meter.record_message_sent(1024);
        meter.record_message_received(1024);
        meter.record_message_sent(2048);
        meter.record_message_received(2048);

        let stats = meter.statistics();
        assert_eq!(stats.total_messages_sent, 2);
        assert_eq!(stats.total_messages_received, 2);
        assert_eq!(stats.total_bytes_sent, 3072);
        assert_eq!(stats.total_bytes_received, 3072);
    }

    #[test]
    fn test_error_meter() {
        let meter = errors::ErrorMeter::new();

        // Record operations
        meter.record_success();
        meter.record_success();
        meter.record_connection_error();
        meter.record_timeout_error();

        let stats = meter.statistics();
        assert_eq!(stats.total_operations, 4);
        assert_eq!(stats.successful_operations, 2);
        assert_eq!(stats.failed_operations, 2);
        assert_eq!(stats.success_rate_percent, 50.0);
    }

    #[test]
    fn test_confidence_interval() {
        let values = vec![10.0, 12.0, 14.0, 16.0, 18.0];
        let (lower, upper) = statistics::confidence_interval(&values, 0.95);

        assert!(lower < 14.0); // Mean should be within confidence interval
        assert!(upper > 14.0);
        assert!(lower < upper);
    }
}</content>
</xai:function_call">Create comprehensive metrics collection framework for benchmarking

- HDR histogram-based latency measurement with microsecond precision
- Atomic counter-based throughput and bandwidth tracking
- System resource monitoring (CPU, memory, network)
- Error rate and success tracking
- Statistical analysis utilities (confidence intervals, t-tests)
- Comprehensive benchmark results aggregation
- JSON serialization for automated reporting