use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use std::collections::VecDeque;
use std::collections::HashMap;
use parking_lot::Mutex;

#[derive(Debug, Clone)]
pub enum MetricUnit {
    Bytes,
    Milliseconds,
    Count,
    Percent,
    Custom(String),
}

trait MetricValueHelper {
    fn get_description(&self) -> &str;
    fn variant_name(&self) -> &'static str;
}

impl MetricValueHelper for MetricValue {
    fn get_description(&self) -> &str {
        match self {
            MetricValue::Counter { description, .. } => description,
            MetricValue::Gauge { description, .. } => description,
            MetricValue::Label { description, .. } => description,
            MetricValue::State { description, .. } => description,
        }
    }

    fn variant_name(&self) -> &'static str {
        match self {
            MetricValue::Counter { .. } => "Counter",
            MetricValue::Gauge { .. } => "Gauge", 
            MetricValue::Label { .. } => "Label",
            MetricValue::State { .. } => "State",
        }
    }
}

#[derive(Debug, Clone)]
pub enum MetricValue {
    Counter {
        value: u64,
        description: String,
    },
    Gauge {
        value: f64,
        unit: MetricUnit,
        description: String,
    },
    Label {
        value: String,
        description: String,
    },
    State {
        value: String,
        allowed_values: Vec<String>,
        description: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum MetricError {
    #[error("Invalid state value {value} - must be one of {allowed:?}")]
    InvalidState {
        value: String,
        allowed: Vec<String>,
    },
    #[error("Invalid value type for metric {name}: expected {expected}, got {actual}")]
    TypeMismatch {
        name: String,
        expected: &'static str,
        actual: &'static str,
    },
}

pub type MetricResult<T> = Result<T, MetricError>;

lazy_static::lazy_static! {
    static ref METRIC_REGISTRY: Mutex<HashMap<String, Metric>> = Mutex::new(HashMap::new());
}

/// Represents a single metric measurement
#[derive(Debug, Clone)]
pub struct TimePoint {
    timestamp: u64,
    value: u64,
}

#[derive(Debug, Clone)]
pub struct TimeSeries {
    points: VecDeque<TimePoint>,
    window_size: Duration,
    max_points: usize,
}

impl TimeSeries {
    fn new(window_size: Duration, max_points: usize) -> Self {
        Self {
            points: VecDeque::with_capacity(max_points),
            window_size,
            max_points,
        }
    }

    fn add_point(&mut self, value: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.points.push_back(TimePoint {
            timestamp: now,
            value,
        });

        // Remove old points outside the window
        let window_secs = self.window_size.as_secs();
        while let Some(front) = self.points.front() {
            if now - front.timestamp > window_secs {
                self.points.pop_front();
            } else {
                break;
            }
        }

        // Enforce max points limit
        while self.points.len() > self.max_points {
            self.points.pop_front();
        }
    }

    fn get_statistics(&self) -> (u64, u64, u64) { // returns (mean, stddev, p95) in nanoseconds
        if self.points.is_empty() {
            return (0, 0, 0);
        }

        let values: Vec<u64> = self.points.iter()
            .map(|p| p.value)
            .collect();
        
        let mean = values.iter().sum::<u64>() / values.len() as u64;
        
        let variance = values.iter()
            .map(|&x| {
                let diff = if x > mean { x - mean } else { mean - x };
                diff * diff
            })
            .sum::<u64>() / values.len() as u64;
        
        let stddev = (variance as f64).sqrt() as u64;
        
        let mut sorted_values = values;
        sorted_values.sort_unstable();
        let p95_idx = ((sorted_values.len() as f64 * 0.95) as usize).min(sorted_values.len() - 1);
        let p95 = sorted_values[p95_idx];

        (mean, stddev, p95)
    }
}

#[derive(Debug, Clone)]
pub struct Metric {
    count: Arc<AtomicU64>,
    total_time: Arc<AtomicU64>, // stored in nanoseconds
    min_time: Arc<AtomicU64>,
    max_time: Arc<AtomicU64>,
    time_series: Arc<parking_lot::Mutex<TimeSeries>>,
    value: Arc<parking_lot::Mutex<MetricValue>>,
}

impl Default for Metric {
    fn default() -> Self {
        Self {
            count: Arc::new(AtomicU64::new(0)),
            total_time: Arc::new(AtomicU64::new(0)),
            min_time: Arc::new(AtomicU64::new(u64::MAX)),
            max_time: Arc::new(AtomicU64::new(0)),
            time_series: Arc::new(parking_lot::Mutex::new(TimeSeries::new(
                Duration::from_secs(3600), // 1 hour window
                1000, // max 1000 points
            ))),
            value: Arc::new(parking_lot::Mutex::new(MetricValue::Counter {
                value: 0,
                description: "Default counter".into(),
            })),
        }
    }
}

impl Metric {
    /// Record a new time measurement
    pub fn record(&self, duration: Duration) {
        let nanos = duration.as_nanos() as u64;
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_time.fetch_add(nanos, Ordering::Relaxed);
        
        // Update min/max times
        self.min_time.fetch_min(nanos, Ordering::Relaxed);
        self.max_time.fetch_max(nanos, Ordering::Relaxed);
        
        // Add to time series
        self.time_series.lock().add_point(nanos);
    }

    /// Record a metric value
    pub fn record_value(name: &str, new_value: MetricValue) -> MetricResult<()> {
        let mut registry = METRIC_REGISTRY.lock();
        let metric = registry.entry(name.to_string()).or_insert_with(|| {
            Metric {
                count: Arc::new(AtomicU64::new(0)),
                total_time: Arc::new(AtomicU64::new(0)),
                min_time: Arc::new(AtomicU64::new(u64::MAX)),
                max_time: Arc::new(AtomicU64::new(0)),
                time_series: Arc::new(parking_lot::Mutex::new(TimeSeries::new(
                    Duration::from_secs(3600),
                    1000,
                ))),
                value: Arc::new(parking_lot::Mutex::new(new_value.clone())),
            }
        });

        let mut value_guard = metric.value.lock();
        
        // Validate type consistency
        let description = value_guard.get_description().to_string();
        match (&*value_guard, &new_value) {
            (MetricValue::Counter { .. }, MetricValue::Counter { value: new_value, .. }) => {
                *value_guard = MetricValue::Counter {
                    value: *new_value,
                    description,
                }
            },
            (MetricValue::Gauge { unit, .. }, MetricValue::Gauge { value, .. }) => {
                *value_guard = MetricValue::Gauge {
                    value: *value,
                    unit: unit.clone(),
                    description,
                }
            },
            (MetricValue::Label { .. }, MetricValue::Label { value, .. }) => {
                *value_guard = MetricValue::Label {
                    value: value.clone(),
                    description,
                }
            },
            (MetricValue::State { allowed_values, .. }, MetricValue::State { value, .. }) => {
                if !allowed_values.contains(value) {
                    return Err(MetricError::InvalidState {
                        value: value.clone(),
                        allowed: allowed_values.clone(),
                    });
                }
                *value_guard = MetricValue::State {
                    value: value.clone(),
                    allowed_values: allowed_values.clone(),
                    description,
                }
            },
            (current, new_val) => {
                return Err(MetricError::TypeMismatch {
                    name: name.to_string(),
                    expected: current.variant_name(),
                    actual: new_val.variant_name(),
                });
            }
        }

        metric.count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Get the average latency
    pub fn average(&self) -> Duration {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return Duration::from_secs(0);
        }
        let total = self.total_time.load(Ordering::Relaxed);
        Duration::from_nanos(total / count)
    }

    /// Get the minimum latency
    pub fn min(&self) -> Duration {
        Duration::from_nanos(self.min_time.load(Ordering::Relaxed))
    }

    /// Get the maximum latency
    pub fn max(&self) -> Duration {
        Duration::from_nanos(self.max_time.load(Ordering::Relaxed))
    }

    /// Get the total number of measurements
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

/// Performance metrics collector
#[derive(Debug, Default, Clone)]
pub struct Metrics {
    transport_latency: Metric,
    stream_latency: Metric,
    operation_count: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self::default()
    }

    /// Record transport-level latency
    pub fn record_transport_latency(&self, duration: Duration) {
        self.transport_latency.record(duration);
    }

    /// Record stream-level latency
    pub fn record_stream_latency(&self, duration: Duration) {
        self.stream_latency.record(duration);
    }

    /// Record bytes sent
    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes received
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increment operation counter
    pub fn increment_operations(&self) {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current throughput in bytes per second
    pub fn throughput(&self) -> (f64, f64) {
        let bytes_sent = self.bytes_sent.load(Ordering::Relaxed) as f64;
        let bytes_received = self.bytes_received.load(Ordering::Relaxed) as f64;
        let elapsed = self.transport_latency.total_time.load(Ordering::Relaxed) as f64;
        
        if elapsed == 0.0 {
            return (0.0, 0.0);
        }

        let seconds = elapsed / 1_000_000_000.0;
        (bytes_sent / seconds, bytes_received / seconds)
    }

    /// Generate a performance report
    pub fn generate_report(&self) -> String {
        let (send_throughput, recv_throughput) = self.throughput();
        
        // Get detailed time series statistics
        let transport_stats = self.transport_latency.time_series.lock();
        let stream_stats = self.stream_latency.time_series.lock();
        
        let (transport_mean, transport_stddev, transport_p95) = transport_stats.get_statistics();
        let (stream_mean, stream_stddev, stream_p95) = stream_stats.get_statistics();
        
        // Calculate additional metrics
        let transport_ops = self.transport_latency.count();
        let stream_ops = self.stream_latency.count();
        format!(
            "Performance Metrics Report:\n\
             Transport Latency:\n\
             - Average: {:?}\n\
             - Min: {:?}\n\
             - Max: {:?}\n\
             - Last Hour Statistics:\n\
               * Mean: {:.2} ms\n\
               * StdDev: {:.2} ms\n\
               * P95: {:.2} ms\n\
             Stream Latency:\n\
             - Average: {:?}\n\
             - Min: {:?}\n\
             - Max: {:?}\n\
             - Last Hour Statistics:\n\
               * Mean: {:.2} ms\n\
               * StdDev: {:.2} ms\n\
               * P95: {:.2} ms\n\
             Throughput:\n\
             - Send: {:.2} MB/s\n\
             - Receive: {:.2} MB/s\n\
             Operations:\n\
             - Total: {}\n\
             - Transport: {}\n\
             - Stream: {}\n\
             Total Bytes:\n\
             - Sent: {} bytes\n\
             - Received: {} bytes",
            self.transport_latency.average(),
            self.transport_latency.min(),
            self.transport_latency.max(),
            transport_mean as f64 / 1_000_000.0,
            transport_stddev as f64 / 1_000_000.0,
            transport_p95 as f64 / 1_000_000.0,
            self.stream_latency.average(),
            self.stream_latency.min(),
            self.stream_latency.max(),
            stream_mean as f64 / 1_000_000.0,
            stream_stddev as f64 / 1_000_000.0,
            stream_p95 as f64 / 1_000_000.0,
            send_throughput / (1024.0 * 1024.0),
            recv_throughput / (1024.0 * 1024.0),
            self.operation_count.load(Ordering::Relaxed),
            transport_ops,
            stream_ops,
            self.bytes_sent.load(Ordering::Relaxed),
            self.bytes_received.load(Ordering::Relaxed)
        )
    }
}

/// Utility struct for measuring operation duration
pub struct Operation<'a> {
    start: Instant,
    metrics: &'a Metrics,
    transport: bool,
}

impl<'a> Operation<'a> {
    /// Start timing a transport operation
    pub fn start_transport(metrics: &'a Metrics) -> Self {
        metrics.increment_operations();
        Self {
            start: Instant::now(),
            metrics,
            transport: true,
        }
    }

    /// Start timing a stream operation
    pub fn start_stream(metrics: &'a Metrics) -> Self {
        metrics.increment_operations();
        Self {
            start: Instant::now(),
            metrics,
            transport: false,
        }
    }
}

impl<'a> Drop for Operation<'a> {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        if self.transport {
            self.metrics.record_transport_latency(duration);
        } else {
            self.metrics.record_stream_latency(duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_metrics_recording() {
        let metrics = Metrics::new();
        
        // Record some test measurements
        metrics.record_transport_latency(Duration::from_millis(100));
        metrics.record_transport_latency(Duration::from_millis(200));
        metrics.add_bytes_sent(1024);
        metrics.add_bytes_received(2048);
        metrics.increment_operations();
        
        assert_eq!(metrics.transport_latency.count(), 2);
        assert_eq!(metrics.bytes_sent.load(Ordering::Relaxed), 1024);
        assert_eq!(metrics.bytes_received.load(Ordering::Relaxed), 2048);
        assert_eq!(metrics.operation_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_operation_timing() {
        let metrics = Metrics::new();
        
        {
            let _op = Operation::start_transport(&metrics);
            thread::sleep(Duration::from_millis(10));
        }
        
        assert_eq!(metrics.transport_latency.count(), 1);
        assert!(metrics.transport_latency.average() >= Duration::from_millis(10));
    }

    #[test]
    fn test_metric_value_recording() -> MetricResult<()> {
        // Test Counter
        Metric::record_value("requests", MetricValue::Counter {
            value: 42,
            description: "Total number of requests".into(),
        })?;

        // Test Gauge
        Metric::record_value("memory_usage", MetricValue::Gauge {
            value: 1234.56,
            unit: MetricUnit::Bytes,
            description: "Current memory usage".into(),
        })?;

        // Test Label
        Metric::record_value("version", MetricValue::Label {
            value: "1.0.0".into(),
            description: "Software version".into(),
        })?;

        // Test State
        let allowed_states = vec!["running".into(), "paused".into(), "stopped".into()];
        Metric::record_value("status", MetricValue::State {
            value: "running".into(),
            allowed_values: allowed_states.clone(),
            description: "Current system status".into(),
        })?;

        // Test invalid state value
        let result = Metric::record_value("status", MetricValue::State {
            value: "invalid".into(),
            allowed_values: allowed_states,
            description: "Current system status".into(),
        });
        assert!(result.is_err());
        
        // Test type mismatch
        let result = Metric::record_value("requests", MetricValue::Gauge {
            value: 42.0,
            unit: MetricUnit::Count,
            description: "Should fail".into(),
        });
        assert!(result.is_err());

        Ok(())
    }
}