// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Performance measurement utilities for accurate benchmarking

use crate::benchmark::{
    ErrorMetrics, LatencyMetrics, PerformanceMetrics, ResourceMetrics, ThroughputMetrics,
};
use hdrhistogram::Histogram;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{CpuRefreshKind, Pid, Process, RefreshKind, System};

/// High-resolution timer for accurate latency measurement
pub struct LatencyTimer {
    start: Instant,
}

impl LatencyTimer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_micros(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    pub fn elapsed_millis(&self) -> f64 {
        self.start.elapsed().as_micros() as f64 / 1000.0
    }
}

/// Performance metrics collector with thread-safe operation
pub struct MetricsCollector {
    latency_histogram: Arc<Mutex<Histogram<u64>>>,
    throughput_tracker: Arc<Mutex<ThroughputTracker>>,
    resource_monitor: Arc<Mutex<ResourceMonitor>>,
    error_tracker: Arc<Mutex<ErrorTracker>>,
}

impl MetricsCollector {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            latency_histogram: Arc::new(Mutex::new(Histogram::<u64>::new_with_bounds(
                1, 60_000_000, 3,
            )?)),
            throughput_tracker: Arc::new(Mutex::new(ThroughputTracker::new())),
            resource_monitor: Arc::new(Mutex::new(ResourceMonitor::new())),
            error_tracker: Arc::new(Mutex::new(ErrorTracker::new())),
        })
    }

    /// Record a latency measurement
    pub fn record_latency(
        &self,
        latency_micros: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.latency_histogram.lock().record(latency_micros)?;
        Ok(())
    }

    /// Record throughput measurement
    pub fn record_throughput(&self, bytes: u64, duration: Duration) {
        self.throughput_tracker.lock().record(bytes, duration);
    }

    /// Record error
    pub fn record_error(&self, error_type: ErrorType) {
        self.error_tracker.lock().record_error(error_type);
    }

    /// Update resource usage measurements
    pub fn update_resource_usage(&self) {
        self.resource_monitor.lock().update();
    }

    /// Generate comprehensive performance metrics
    pub fn generate_metrics(&self) -> PerformanceMetrics {
        let latency_hist = self.latency_histogram.lock();
        let throughput = self.throughput_tracker.lock();
        let resources = self.resource_monitor.lock();
        let errors = self.error_tracker.lock();

        let latency_percentiles = LatencyMetrics {
            p50_ms: latency_hist.value_at_quantile(0.5) as f64 / 1000.0,
            p95_ms: latency_hist.value_at_quantile(0.95) as f64 / 1000.0,
            p99_ms: latency_hist.value_at_quantile(0.99) as f64 / 1000.0,
            mean_ms: latency_hist.mean() / 1000.0,
            std_dev_ms: latency_hist.stdev() / 1000.0,
        };

        let throughput_metrics = ThroughputMetrics {
            messages_per_second: throughput.messages_per_second(),
            bytes_per_second: throughput.bytes_per_second(),
            mbps: throughput.mbps(),
        };

        let resource_usage = ResourceMetrics {
            cpu_usage_percent: resources.cpu_usage,
            memory_usage_bytes: resources.memory_usage,
            network_io_bytes: resources.network_io,
        };

        let error_metrics = ErrorMetrics {
            connection_failures: errors.connection_failures,
            message_failures: errors.message_failures,
            timeout_failures: errors.timeout_failures,
            total_errors: errors.total_errors(),
        };

        PerformanceMetrics {
            latency_percentiles,
            throughput: throughput_metrics,
            resource_usage,
            error_metrics,
        }
    }

    /// Reset all metrics for a new test run
    pub fn reset(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.latency_histogram.lock().reset();
        self.throughput_tracker.lock().reset();
        self.resource_monitor.lock().reset();
        self.error_tracker.lock().reset();
        Ok(())
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new().expect("Failed to create MetricsCollector")
    }
}

/// Throughput measurement tracker
struct ThroughputTracker {
    total_bytes: u64,
    total_messages: u64,
    start_time: Instant,
    last_update: Instant,
}

impl ThroughputTracker {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            total_bytes: 0,
            total_messages: 0,
            start_time: now,
            last_update: now,
        }
    }

    fn record(&mut self, bytes: u64, _duration: Duration) {
        self.total_bytes += bytes;
        self.total_messages += 1;
        self.last_update = Instant::now();
    }

    fn messages_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_messages as f64 / elapsed
        } else {
            0.0
        }
    }

    fn bytes_per_second(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_bytes as f64 / elapsed
        } else {
            0.0
        }
    }

    fn mbps(&self) -> f64 {
        self.bytes_per_second() / 1_048_576.0
    }

    fn reset(&mut self) {
        let now = Instant::now();
        self.total_bytes = 0;
        self.total_messages = 0;
        self.start_time = now;
        self.last_update = now;
    }
}

/// Resource usage monitor
struct ResourceMonitor {
    system: System,
    cpu_usage: f64,
    memory_usage: u64,
    network_io: u64,
    process_id: u32,
}

impl ResourceMonitor {
    fn new() -> Self {
        let system = System::new_with_specifics(
            RefreshKind::new()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(sysinfo::MemoryRefreshKind::everything())
                .with_processes(sysinfo::ProcessRefreshKind::everything()),
        );

        let process_id = std::process::id();

        Self {
            system,
            cpu_usage: 0.0,
            memory_usage: 0,
            network_io: 0,
            process_id,
        }
    }

    fn update(&mut self) {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();
        self.system.refresh_processes();

        // Get current process info
        if let Some(process) = self.system.process(Pid::from_u32(self.process_id)) {
            self.cpu_usage = process.cpu_usage() as f64;
            self.memory_usage = process.memory();
        }

        // Calculate total CPU usage
        let total_cpu: f32 = self.system.cpus().iter().map(|cpu| cpu.cpu_usage()).sum();
        self.cpu_usage = total_cpu as f64 / self.system.cpus().len() as f64;

        // Network I/O would require additional system calls or external tools
        // For now, we'll track it indirectly through throughput
    }

    fn reset(&mut self) {
        self.cpu_usage = 0.0;
        self.memory_usage = 0;
        self.network_io = 0;
    }
}

/// Error type enumeration
#[derive(Debug, Clone, Copy)]
pub enum ErrorType {
    ConnectionFailure,
    MessageFailure,
    TimeoutFailure,
}

/// Error tracking structure
struct ErrorTracker {
    connection_failures: u64,
    message_failures: u64,
    timeout_failures: u64,
}

impl ErrorTracker {
    fn new() -> Self {
        Self {
            connection_failures: 0,
            message_failures: 0,
            timeout_failures: 0,
        }
    }

    fn record_error(&mut self, error_type: ErrorType) {
        match error_type {
            ErrorType::ConnectionFailure => self.connection_failures += 1,
            ErrorType::MessageFailure => self.message_failures += 1,
            ErrorType::TimeoutFailure => self.timeout_failures += 1,
        }
    }

    fn total_errors(&self) -> u64 {
        self.connection_failures + self.message_failures + self.timeout_failures
    }

    fn reset(&mut self) {
        self.connection_failures = 0;
        self.message_failures = 0;
        self.timeout_failures = 0;
    }
}

/// Statistical analysis utilities
pub struct StatisticalAnalyzer;

impl StatisticalAnalyzer {
    /// Calculate confidence interval for a given confidence level
    pub fn confidence_interval(
        mean: f64,
        std_dev: f64,
        sample_size: usize,
        confidence_level: f64,
    ) -> (f64, f64) {
        let t_value = Self::t_distribution_value(confidence_level, sample_size - 1);
        let margin_of_error = t_value * (std_dev / (sample_size as f64).sqrt());

        (mean - margin_of_error, mean + margin_of_error)
    }

    /// Calculate statistical significance using Welch's t-test
    pub fn welch_t_test(
        mean1: f64,
        std_dev1: f64,
        n1: usize,
        mean2: f64,
        std_dev2: f64,
        n2: usize,
    ) -> f64 {
        let var1 = std_dev1 * std_dev1;
        let var2 = std_dev2 * std_dev2;

        let pooled_se = ((var1 / n1 as f64) + (var2 / n2 as f64)).sqrt();

        if pooled_se > 0.0 {
            (mean1 - mean2) / pooled_se
        } else {
            0.0
        }
    }

    /// Simple t-distribution approximation for common confidence levels
    fn t_distribution_value(confidence_level: f64, degrees_of_freedom: usize) -> f64 {
        // Simple approximation for common cases
        match (confidence_level * 100.0) as u32 {
            95 => {
                if degrees_of_freedom >= 30 {
                    1.96
                } else {
                    2.045
                }
            }
            99 => {
                if degrees_of_freedom >= 30 {
                    2.576
                } else {
                    2.75
                }
            }
            _ => 1.96, // Default to 95% confidence
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latency_timer() {
        let timer = LatencyTimer::new();
        std::thread::sleep(Duration::from_millis(10));
        let elapsed = timer.elapsed_micros();
        assert!(elapsed >= 10_000); // Should be at least 10ms
    }

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new().unwrap();

        // Record some test data
        collector.record_latency(1000).unwrap(); // 1ms
        collector.record_latency(2000).unwrap(); // 2ms
        collector.record_throughput(1024, Duration::from_millis(1));

        let metrics = collector.generate_metrics();
        assert!(metrics.latency_percentiles.p50_ms > 0.0);
        assert!(metrics.throughput.bytes_per_second > 0.0);
    }

    #[test]
    fn test_statistical_analysis() {
        let (lower, upper) = StatisticalAnalyzer::confidence_interval(100.0, 10.0, 30, 0.95);

        assert!(lower < 100.0);
        assert!(upper > 100.0);
        assert!(upper - lower > 0.0);
    }
}
