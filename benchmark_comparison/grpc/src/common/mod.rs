use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub server_address: String,
    pub message_size: usize,
    pub num_messages: usize,
    pub concurrent_connections: usize,
    pub test_duration_secs: u64,
    pub warmup_duration_secs: u64,
    pub payload_pattern: PayloadPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadPattern {
    Random,
    Sequential,
    Zeroes,
    Repeating(String),
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            server_address: "http://127.0.0.1:50051".to_string(),
            message_size: 1024,
            num_messages: 1000,
            concurrent_connections: 10,
            test_duration_secs: 30,
            warmup_duration_secs: 5,
            payload_pattern: PayloadPattern::Random,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkMetrics {
    pub test_id: String,
    pub test_type: String,
    pub start_time: u64,
    pub end_time: u64,
    pub total_messages: u64,
    pub successful_messages: u64,
    pub failed_messages: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub latency_stats: LatencyStats,
    pub throughput_stats: ThroughputStats,
    pub error_stats: ErrorStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    pub min_us: u64,
    pub max_us: u64,
    pub mean_us: f64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub p999_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputStats {
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
    pub megabits_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorStats {
    pub connection_errors: u64,
    pub timeout_errors: u64,
    pub protocol_errors: u64,
    pub other_errors: u64,
}

pub struct MetricsCollector {
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    errors: Arc<ErrorCounters>,
    latencies: Arc<std::sync::Mutex<Vec<Duration>>>,
    start_time: Instant,
}

#[derive(Debug, Default)]
pub struct ErrorCounters {
    connection_errors: AtomicU64,
    timeout_errors: AtomicU64,
    protocol_errors: AtomicU64,
    other_errors: AtomicU64,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            errors: Arc::new(ErrorCounters::default()),
            latencies: Arc::new(std::sync::Mutex::new(Vec::new())),
            start_time: Instant::now(),
        }
    }

    pub fn record_sent(&self, bytes: u64) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_received(&self, bytes: u64, latency: Duration) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
        if let Ok(mut latencies) = self.latencies.lock() {
            latencies.push(latency);
        }
    }

    pub fn record_error(&self, error_type: ErrorType) {
        match error_type {
            ErrorType::Connection => self.errors.connection_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Timeout => self.errors.timeout_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Protocol => self.errors.protocol_errors.fetch_add(1, Ordering::Relaxed),
            ErrorType::Other => self.errors.other_errors.fetch_add(1, Ordering::Relaxed),
        };
    }

    pub fn calculate_metrics(&self, test_type: String) -> BenchmarkMetrics {
        let total_messages = self.messages_sent.load(Ordering::Relaxed);
        let successful_messages = self.messages_received.load(Ordering::Relaxed);
        let failed_messages = total_messages - successful_messages;
        
        let bytes_sent = self.bytes_sent.load(Ordering::Relaxed);
        let bytes_received = self.bytes_received.load(Ordering::Relaxed);
        
        let elapsed = self.start_time.elapsed();
        let throughput_stats = ThroughputStats {
            messages_per_second: successful_messages as f64 / elapsed.as_secs_f64(),
            bytes_per_second: bytes_received as f64 / elapsed.as_secs_f64(),
            megabits_per_second: (bytes_received as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0),
        };

        let latency_stats = if let Ok(latencies) = self.latencies.lock() {
            if !latencies.is_empty() {
                let mut sorted_latencies: Vec<Duration> = latencies.clone();
                sorted_latencies.sort();
                
                let len = sorted_latencies.len();
                LatencyStats {
                    min_us: sorted_latencies[0].as_micros() as u64,
                    max_us: sorted_latencies[len - 1].as_micros() as u64,
                    mean_us: sorted_latencies.iter().sum::<Duration>().as_micros() as f64 / len as f64,
                    p50_us: sorted_latencies[len / 2].as_micros() as u64,
                    p95_us: sorted_latencies[(len * 95) / 100].as_micros() as u64,
                    p99_us: sorted_latencies[(len * 99) / 100].as_micros() as u64,
                    p999_us: sorted_latencies[(len * 999) / 1000].as_micros() as u64,
                }
            } else {
                LatencyStats {
                    min_us: 0,
                    max_us: 0,
                    mean_us: 0.0,
                    p50_us: 0,
                    p95_us: 0,
                    p99_us: 0,
                    p999_us: 0,
                }
            }
        } else {
            LatencyStats {
                min_us: 0,
                max_us: 0,
                mean_us: 0.0,
                p50_us: 0,
                p95_us: 0,
                p99_us: 0,
                p999_us: 0,
            }
        };

        BenchmarkMetrics {
            test_id: Uuid::new_v4().to_string(),
            test_type,
            start_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            end_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_messages,
            successful_messages,
            failed_messages,
            bytes_sent,
            bytes_received,
            latency_stats,
            throughput_stats,
            error_stats: ErrorStats {
                connection_errors: self.errors.connection_errors.load(Ordering::Relaxed),
                timeout_errors: self.errors.timeout_errors.load(Ordering::Relaxed),
                protocol_errors: self.errors.protocol_errors.load(Ordering::Relaxed),
                other_errors: self.errors.other_errors.load(Ordering::Relaxed),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ErrorType {
    Connection,
    Timeout,
    Protocol,
    Other,
}

pub fn generate_payload(size: usize, pattern: &PayloadPattern) -> Vec<u8> {
    match pattern {
        PayloadPattern::Zeroes => vec![0u8; size],
        PayloadPattern::Random => {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            (0..size).map(|_| rng.gen()).collect()
        }
        PayloadPattern::Sequential => (0..size).map(|i| (i % 256) as u8).collect(),
        PayloadPattern::Repeating(pattern_str) => {
            let pattern_bytes = pattern_str.as_bytes();
            let mut payload = Vec::with_capacity(size);
            for i in 0..size {
                payload.push(pattern_bytes[i % pattern_bytes.len()]);
            }
            payload
        }
    }
}

pub fn get_current_timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}