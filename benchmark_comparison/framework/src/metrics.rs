use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub test_id: String,
    pub implementation_name: String,
    pub scenario_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub metrics: TestMetrics,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetrics {
    pub total_messages: u64,
    pub total_bytes: u64,
    pub duration: Duration,
    pub latency_min: Duration,
    pub latency_max: Duration,
    pub latency_mean: Duration,
    pub latency_p50: Duration,
    pub latency_p95: Duration,
    pub latency_p99: Duration,
    pub latency_p99_9: Duration,
    pub throughput_mps: f64,  // messages per second
    pub throughput_bps: f64,  // bytes per second
    pub connection_time: Duration,
    pub system_metrics: SystemMetrics,
    pub errors: ErrorMetrics,
}

impl Default for TestMetrics {
    fn default() -> Self {
        Self {
            total_messages: 0,
            total_bytes: 0,
            duration: Duration::ZERO,
            latency_min: Duration::ZERO,
            latency_max: Duration::ZERO,
            latency_mean: Duration::ZERO,
            latency_p50: Duration::ZERO,
            latency_p95: Duration::ZERO,
            latency_p99: Duration::ZERO,
            latency_p99_9: Duration::ZERO,
            throughput_mps: 0.0,
            throughput_bps: 0.0,
            connection_time: Duration::ZERO,
            system_metrics: SystemMetrics::default(),
            errors: ErrorMetrics::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub disk_usage_percent: f64,
    pub load_average: f64,
    pub process_count: usize,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self {
            cpu_usage_percent: 0.0,
            memory_used_mb: 0,
            memory_total_mb: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            disk_usage_percent: 0.0,
            load_average: 0.0,
            process_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMetrics {
    pub total_errors: u64,
    pub connection_errors: u64,
    pub timeout_errors: u64,
    pub protocol_errors: u64,
    pub network_errors: u64,
    pub other_errors: u64,
    pub error_rate: f64,
}

impl Default for ErrorMetrics {
    fn default() -> Self {
        Self {
            total_errors: 0,
            connection_errors: 0,
            timeout_errors: 0,
            protocol_errors: 0,
            network_errors: 0,
            other_errors: 0,
            error_rate: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyDistribution {
    pub buckets: Vec<LatencyBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBucket {
    pub upper_bound: Duration,
    pub count: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
    pub megabits_per_second: f64,
    pub peak_mps: f64,
    pub average_mps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub total_connections: u64,
    pub successful_connections: u64,
    pub failed_connections: u64,
    pub connection_time_mean: Duration,
    pub connection_time_p95: Duration,
    pub connection_time_p99: Duration,
}

impl TestMetrics {
    pub fn calculate_throughput_mbps(&self) -> f64 {
        (self.throughput_bps * 8.0) / (1024.0 * 1024.0)
    }

    pub fn get_latency_summary(&self) -> LatencySummary {
        LatencySummary {
            min: self.latency_min,
            max: self.latency_max,
            mean: self.latency_mean,
            p50: self.latency_p50,
            p95: self.latency_p95,
            p99: self.latency_p99,
            p99_9: self.latency_p99_9,
        }
    }

    pub fn get_error_rate(&self) -> f64 {
        if self.total_messages == 0 {
            0.0
        } else {
            self.errors.total_errors as f64 / self.total_messages as f64 * 100.0
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySummary {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub p99_9: Duration,
}

impl LatencySummary {
    pub fn to_millis(&self) -> LatencySummaryMillis {
        LatencySummaryMillis {
            min: self.min.as_millis() as f64,
            max: self.max.as_millis() as f64,
            mean: self.mean.as_millis() as f64,
            p50: self.p50.as_millis() as f64,
            p95: self.p95.as_millis() as f64,
            p99: self.p99.as_millis() as f64,
            p99_9: self.p99_9.as_millis() as f64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencySummaryMillis {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p99_9: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceComparison {
    pub implementation_a: String,
    pub implementation_b: String,
    pub latency_improvement_percent: f64,
    pub throughput_improvement_percent: f64,
    pub cpu_efficiency_improvement_percent: f64,
    pub memory_efficiency_improvement_percent: f64,
    pub error_rate_improvement_percent: f64,
    pub overall_score: f64,
}

impl PerformanceComparison {
    pub fn calculate(
        metrics_a: &TestMetrics,
        metrics_b: &TestMetrics,
        impl_a: &str,
        impl_b: &str,
    ) -> Self {
        let latency_improvement = if metrics_a.latency_mean > metrics_b.latency_mean {
            ((metrics_a.latency_mean.as_nanos() as f64 - metrics_b.latency_mean.as_nanos() as f64) 
                / metrics_a.latency_mean.as_nanos() as f64) * 100.0
        } else {
            -((metrics_b.latency_mean.as_nanos() as f64 - metrics_a.latency_mean.as_nanos() as f64) 
                / metrics_b.latency_mean.as_nanos() as f64) * 100.0
        };

        let throughput_improvement = if metrics_a.throughput_mps < metrics_b.throughput_mps {
            ((metrics_b.throughput_mps - metrics_a.throughput_mps) / metrics_a.throughput_mps) * 100.0
        } else {
            -((metrics_a.throughput_mps - metrics_b.throughput_mps) / metrics_b.throughput_mps) * 100.0
        };

        let cpu_efficiency = if metrics_a.system_metrics.cpu_usage_percent > metrics_b.system_metrics.cpu_usage_percent {
            ((metrics_a.system_metrics.cpu_usage_percent - metrics_b.system_metrics.cpu_usage_percent) 
                / metrics_a.system_metrics.cpu_usage_percent) * 100.0
        } else {
            -((metrics_b.system_metrics.cpu_usage_percent - metrics_a.system_metrics.cpu_usage_percent) 
                / metrics_b.system_metrics.cpu_usage_percent) * 100.0
        };

        let memory_efficiency = if metrics_a.system_metrics.memory_used_mb > metrics_b.system_metrics.memory_used_mb {
            ((metrics_a.system_metrics.memory_used_mb as f64 - metrics_b.system_metrics.memory_used_mb as f64) 
                / metrics_a.system_metrics.memory_used_mb as f64) * 100.0
        } else {
            -((metrics_b.system_metrics.memory_used_mb as f64 - metrics_a.system_metrics.memory_used_mb as f64) 
                / metrics_b.system_metrics.memory_used_mb as f64) * 100.0
        };

        let error_rate_improvement = if metrics_a.get_error_rate() > metrics_b.get_error_rate() {
            (metrics_a.get_error_rate() - metrics_b.get_error_rate())
        } else {
            -(metrics_b.get_error_rate() - metrics_a.get_error_rate())
        };

        // Calculate overall score (weighted average)
        let overall_score = (
            latency_improvement * 0.3 +
            throughput_improvement * 0.3 +
            cpu_efficiency * 0.2 +
            memory_efficiency * 0.15 +
            error_rate_improvement * 0.05
        );

        Self {
            implementation_a: impl_a.to_string(),
            implementation_b: impl_b.to_string(),
            latency_improvement_percent: latency_improvement,
            throughput_improvement_percent: throughput_improvement,
            cpu_efficiency_improvement_percent: cpu_efficiency,
            memory_efficiency_improvement_percent: memory_efficiency,
            error_rate_improvement_percent: error_rate_improvement,
            overall_score,
        }
    }
}