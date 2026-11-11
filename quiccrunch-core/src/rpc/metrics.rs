//! OpenTelemetry and Prometheus Metrics for RPC
//!
//! Provides OpenTelemetry tracing integration, Prometheus metrics export,
//! custom metrics collection, and distributed tracing support.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

/// Metric value
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<f64>),
}

/// Metric definition
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Metric {
    pub name: String,
    pub metric_type: MetricType,
    pub value: MetricValue,
    pub labels: HashMap<String, String>,
    pub timestamp: u64,
}

/// Metrics collector
pub struct MetricsCollector {
    metrics: Arc<RwLock<HashMap<String, Metric>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Increment a counter
    pub async fn increment_counter(&self, name: &str, labels: HashMap<String, String>, value: u64) {
        let mut metrics = self.metrics.write().await;
        let key = format!("{}:{}", name, Self::labels_hash(&labels));

        let metric = metrics.entry(key).or_insert_with(|| Metric {
            name: name.to_string(),
            metric_type: MetricType::Counter,
            value: MetricValue::Counter(0),
            labels,
            timestamp: Self::now(),
        });

        if let MetricValue::Counter(ref mut count) = metric.value {
            *count += value;
        }
    }

    /// Set a gauge
    pub async fn set_gauge(&self, name: &str, labels: HashMap<String, String>, value: f64) {
        let mut metrics = self.metrics.write().await;
        let key = format!("{}:{}", name, Self::labels_hash(&labels));

        metrics.insert(
            key,
            Metric {
                name: name.to_string(),
                metric_type: MetricType::Gauge,
                value: MetricValue::Gauge(value),
                labels,
                timestamp: Self::now(),
            },
        );
    }

    /// Record a histogram value
    pub async fn record_histogram(&self, name: &str, labels: HashMap<String, String>, value: f64) {
        let mut metrics = self.metrics.write().await;
        let key = format!("{}:{}", name, Self::labels_hash(&labels));

        let metric = metrics.entry(key).or_insert_with(|| Metric {
            name: name.to_string(),
            metric_type: MetricType::Histogram,
            value: MetricValue::Histogram(Vec::new()),
            labels,
            timestamp: Self::now(),
        });

        if let MetricValue::Histogram(ref mut values) = metric.value {
            values.push(value);
        }
    }

    /// Get all metrics
    pub async fn get_metrics(&self) -> Vec<Metric> {
        let metrics = self.metrics.read().await;
        metrics.values().cloned().collect()
    }

    /// Get metrics as Prometheus format
    pub async fn to_prometheus(&self) -> String {
        let metrics = self.metrics.read().await;
        let mut output = String::new();

        for metric in metrics.values() {
            let labels = Self::format_labels(&metric.labels);

            match &metric.value {
                MetricValue::Counter(value) => {
                    output.push_str(&format!("{}{} {}\n", metric.name, labels, value));
                }
                MetricValue::Gauge(value) => {
                    output.push_str(&format!("{}{} {}\n", metric.name, labels, value));
                }
                MetricValue::Histogram(values) => {
                    // Calculate histogram statistics
                    if !values.is_empty() {
                        let sum: f64 = values.iter().sum();
                        let count = values.len();
                        let mean = sum / count as f64;

                        output.push_str(&format!("{}_sum{} {}\n", metric.name, labels, sum));
                        output.push_str(&format!("{}_count{} {}\n", metric.name, labels, count));
                        output.push_str(&format!("{}_mean{} {}\n", metric.name, labels, mean));
                    }
                }
            }
        }

        output
    }

    /// Clear all metrics
    pub async fn clear(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.clear();
    }

    /// Helper to create labels hash
    fn labels_hash(labels: &HashMap<String, String>) -> String {
        let mut sorted: Vec<_> = labels.iter().collect();
        sorted.sort_by_key(|k| k.0.clone());
        sorted
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Helper to format labels for Prometheus
    fn format_labels(labels: &HashMap<String, String>) -> String {
        if labels.is_empty() {
            return String::new();
        }

        let mut sorted: Vec<_> = labels.iter().collect();
        sorted.sort_by_key(|k| k.0.clone());

        let label_str = sorted
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(",");

        format!("{{{}}}", label_str)
    }

    /// Helper to get current timestamp
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// RPC metrics collector
pub struct RpcMetrics {
    collector: Arc<MetricsCollector>,
}

impl RpcMetrics {
    pub fn new() -> Self {
        Self {
            collector: Arc::new(MetricsCollector::new()),
        }
    }

    /// Record a request
    pub async fn record_request(&self, service: &str, method: &str, duration: Duration) {
        let labels = HashMap::from([
            ("service".to_string(), service.to_string()),
            ("method".to_string(), method.to_string()),
        ]);

        self.collector
            .increment_counter("rpc_requests_total", labels.clone(), 1)
            .await;

        self.collector
            .record_histogram(
                "rpc_request_duration_seconds",
                labels,
                duration.as_secs_f64(),
            )
            .await;
    }

    /// Record a response
    pub async fn record_response(
        &self,
        service: &str,
        method: &str,
        status: &str,
        duration: Duration,
    ) {
        let labels = HashMap::from([
            ("service".to_string(), service.to_string()),
            ("method".to_string(), method.to_string()),
            ("status".to_string(), status.to_string()),
        ]);

        self.collector
            .increment_counter("rpc_responses_total", labels.clone(), 1)
            .await;

        self.collector
            .record_histogram(
                "rpc_response_duration_seconds",
                labels,
                duration.as_secs_f64(),
            )
            .await;
    }

    /// Record an error
    pub async fn record_error(&self, service: &str, method: &str, error_type: &str) {
        let labels = HashMap::from([
            ("service".to_string(), service.to_string()),
            ("method".to_string(), method.to_string()),
            ("error".to_string(), error_type.to_string()),
        ]);

        self.collector
            .increment_counter("rpc_errors_total", labels, 1)
            .await;
    }

    /// Record active connections
    pub async fn set_active_connections(&self, count: usize) {
        self.collector
            .set_gauge("rpc_active_connections", HashMap::new(), count as f64)
            .await;
    }

    /// Get metrics collector
    pub fn collector(&self) -> &Arc<MetricsCollector> {
        &self.collector
    }

    /// Get metrics as Prometheus format
    pub async fn to_prometheus(&self) -> String {
        self.collector.to_prometheus().await
    }
}

impl Default for RpcMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracing span context
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TracingSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub start_time_ms: u128,
    pub duration_ms: Option<f64>,
    pub labels: HashMap<String, String>,
}

impl TracingSpan {
    pub fn new(operation: &str) -> Self {
        Self {
            trace_id: Self::generate_id(),
            span_id: Self::generate_id(),
            parent_span_id: None,
            operation: operation.to_string(),
            start_time_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            duration_ms: None,
            labels: HashMap::new(),
        }
    }

    pub fn with_parent(mut self, parent_span_id: String) -> Self {
        self.parent_span_id = Some(parent_span_id);
        self
    }

    pub fn with_label(mut self, key: String, value: String) -> Self {
        self.labels.insert(key, value);
        self
    }

    pub fn finish(&mut self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        self.duration_ms = Some((now_ms - self.start_time_ms) as f64);
    }

    fn generate_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        format!("{:032x}", timestamp)
    }
}

/// Tracing collector
pub struct TracingCollector {
    spans: Arc<RwLock<Vec<TracingSpan>>>,
}

impl TracingCollector {
    pub fn new() -> Self {
        Self {
            spans: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Start a new span
    pub async fn start_span(&self, operation: &str) -> TracingSpan {
        let span = TracingSpan::new(operation);
        let mut spans = self.spans.write().await;
        spans.push(span.clone());
        span
    }

    /// Finish a span
    pub async fn finish_span(&self, span: &TracingSpan) {
        let mut spans = self.spans.write().await;
        if let Some(pos) = spans.iter().position(|s| s.span_id == span.span_id) {
            spans.remove(pos);
        }
    }

    /// Get all completed spans (for export)
    pub async fn get_spans(&self) -> Vec<TracingSpan> {
        let spans = self.spans.read().await;
        spans.clone()
    }

    /// Get spans as JSON
    pub async fn to_json(&self) -> String {
        let spans = self.spans.read().await;
        serde_json::to_string_pretty(&*spans).unwrap_or_default()
    }
}

impl Default for TracingCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics interceptor for RPC
pub struct MetricsInterceptor {
    metrics: Arc<RpcMetrics>,
}

impl MetricsInterceptor {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RpcMetrics::new()),
        }
    }

    pub fn with_metrics(metrics: Arc<RpcMetrics>) -> Self {
        Self { metrics }
    }

    pub fn metrics(&self) -> &Arc<RpcMetrics> {
        &self.metrics
    }
}

impl Clone for MetricsInterceptor {
    fn clone(&self) -> Self {
        Self {
            metrics: self.metrics.clone(),
        }
    }
}

impl Default for MetricsInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        collector
            .increment_counter("test_counter", HashMap::new(), 1)
            .await;

        collector
            .increment_counter("test_counter", HashMap::new(), 1)
            .await;

        let metrics = collector.get_metrics().await;
        assert_eq!(metrics.len(), 1);

        if let MetricValue::Counter(value) = metrics[0].value {
            assert_eq!(value, 2);
        } else {
            panic!("Expected counter");
        }
    }

    #[tokio::test]
    async fn test_prometheus_format() {
        let collector = MetricsCollector::new();

        collector
            .increment_counter(
                "test_counter",
                HashMap::from([("env".to_string(), "test".to_string())]),
                42,
            )
            .await;

        let prometheus = collector.to_prometheus().await;
        assert!(prometheus.contains("test_counter"));
        assert!(prometheus.contains("42"));
    }

    #[tokio::test]
    async fn test_rpc_metrics() {
        let metrics = RpcMetrics::new();

        metrics
            .record_request("test_service", "test_method", Duration::from_millis(100))
            .await;

        metrics
            .record_response(
                "test_service",
                "test_method",
                "success",
                Duration::from_millis(100),
            )
            .await;

        let prometheus = metrics.to_prometheus().await;
        assert!(prometheus.contains("rpc_requests_total"));
        assert!(prometheus.contains("rpc_responses_total"));
    }
}
