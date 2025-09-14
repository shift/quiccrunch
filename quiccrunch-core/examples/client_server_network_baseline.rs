/// Client-Server Network Baseline Benchmark
/// 
/// This benchmark establishes baseline performance metrics for pure network communication
/// between QuicCrunch client and server, excluding CPU-intensive operations.
/// 
/// Focus areas:
/// - Network latency and throughput
/// - Connection establishment overhead
/// - Message serialization/deserialization
/// - QUIC protocol performance
/// - Memory usage patterns
/// 
/// Excludes:
/// - Heavy computational workloads
/// - Complex business logic simulation
/// - CPU-bound operations

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use quiccrunch_core::client::QuicCrunchClient;
use quiccrunch_core::server::QuicCrunchServer;
use quiccrunch_core::transport::QuicTransport;
use quiccrunch_core::types::{Result, QuicCrunchError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkBenchmarkConfig {
    pub test_duration_secs: u64,
    pub concurrent_clients: usize,
    pub message_sizes: Vec<usize>,
    pub connection_warmup_time_ms: u64,
    pub measurement_interval_ms: u64,
}

impl Default for NetworkBenchmarkConfig {
    fn default() -> Self {
        Self {
            test_duration_secs: 30,
            concurrent_clients: 4,
            message_sizes: vec![64, 256, 1024, 4096, 16384, 65536],
            connection_warmup_time_ms: 1000,
            measurement_interval_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub test_name: String,
    pub message_size_bytes: usize,
    pub concurrent_clients: usize,
    pub duration_seconds: f64,
    
    // Connection metrics
    pub total_connections: u64,
    pub successful_connections: u64,
    pub connection_failures: u64,
    pub avg_connection_time_ms: f64,
    
    // Message metrics
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub message_loss_rate: f64,
    
    // Latency metrics (in milliseconds)
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    
    // Throughput metrics
    pub messages_per_second: f64,
    pub bytes_per_second: f64,
    pub megabits_per_second: f64,
    
    // Resource metrics
    pub peak_memory_mb: f64,
    pub avg_cpu_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResults {
    pub timestamp: String,
    pub config: NetworkBenchmarkConfig,
    pub metrics_by_message_size: HashMap<usize, NetworkMetrics>,
    pub summary: BaselineSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSummary {
    pub total_test_duration_seconds: f64,
    pub peak_messages_per_second: f64,
    pub peak_megabits_per_second: f64,
    pub optimal_message_size_bytes: usize,
    pub avg_latency_across_sizes_ms: f64,
    pub connection_success_rate: f64,
}

pub struct ClientServerNetworkBenchmark {
    config: NetworkBenchmarkConfig,
    server_addr: String,
}

impl ClientServerNetworkBenchmark {
    pub fn new(config: NetworkBenchmarkConfig) -> Self {
        Self {
            config,
            server_addr: "127.0.0.1:0".to_string(),
        }
    }

    pub async fn run_baseline_benchmark(&mut self) -> Result<BaselineResults> {
        println!("🚀 Starting Client-Server Network Baseline Benchmark");
        println!("═══════════════════════════════════════════════════");
        println!("Configuration:");
        println!("  Test duration: {}s", self.config.test_duration_secs);
        println!("  Concurrent clients: {}", self.config.concurrent_clients);
        println!("  Message sizes: {:?} bytes", self.config.message_sizes);
        println!("  Warmup time: {}ms", self.config.connection_warmup_time_ms);
        println!();

        let start_time = Instant::now();
        let mut metrics_by_size = HashMap::new();

        // Start server
        let server_handle = self.start_server().await?;
        
        // Wait for server to be ready
        sleep(Duration::from_millis(500)).await;

        // Run benchmarks for each message size
        for &message_size in &self.config.message_sizes {
            println!("📊 Testing message size: {} bytes ({:.1} KB)", 
                message_size, message_size as f64 / 1024.0);
            
            let metrics = self.run_message_size_benchmark(message_size).await?;
            metrics_by_size.insert(message_size, metrics);
            
            // Brief pause between tests
            sleep(Duration::from_millis(1000)).await;
        }

        // Stop server
        server_handle.abort();

        let total_duration = start_time.elapsed().as_secs_f64();

        // Generate summary
        let summary = self.generate_summary(&metrics_by_size, total_duration);

        let results = BaselineResults {
            timestamp: chrono::Utc::now().to_rfc3339(),
            config: self.config.clone(),
            metrics_by_message_size: metrics_by_size,
            summary,
        };

        self.print_results(&results);
        Ok(results)
    }

    async fn start_server(&mut self) -> Result<tokio::task::JoinHandle<()>> {
        let transport = QuicTransport::new_server("127.0.0.1:0").await?;
        let actual_addr = transport.local_addr()?;
        self.server_addr = actual_addr.to_string();
        
        println!("🖥️  Server started on: {}", self.server_addr);

        let server = QuicCrunchServer::new(transport);
        
        let handle = tokio::spawn(async move {
            if let Err(e) = server.run_simple_echo_server().await {
                eprintln!("Server error: {}", e);
            }
        });

        Ok(handle)
    }

    async fn run_message_size_benchmark(&self, message_size: usize) -> Result<NetworkMetrics> {
        let mut latencies = Vec::new();
        let mut connection_times = Vec::new();
        let mut total_messages_sent = 0u64;
        let mut total_messages_received = 0u64;
        let mut successful_connections = 0u64;
        let mut connection_failures = 0u64;

        let test_start = Instant::now();
        let end_time = test_start + Duration::from_secs(self.config.test_duration_secs);

        // Create message payload
        let payload = vec![42u8; message_size];

        // Spawn concurrent clients
        let mut client_handles = Vec::new();
        let (tx, mut rx) = mpsc::channel(1000);

        for client_id in 0..self.config.concurrent_clients {
            let server_addr = self.server_addr.clone();
            let payload = payload.clone();
            let tx = tx.clone();
            let warmup_time = self.config.connection_warmup_time_ms;

            let handle = tokio::spawn(async move {
                let mut client_metrics = ClientMetrics::default();
                
                // Connection phase
                let connection_start = Instant::now();
                let transport = QuicTransport::new_client().await?;
                
                match transport.connect(&server_addr).await {
                    Ok(connection) => {
                        let connection_time = connection_start.elapsed();
                        client_metrics.connection_time_ms = connection_time.as_secs_f64() * 1000.0;
                        client_metrics.connection_successful = true;

                        let client = QuicCrunchClient::new(connection);

                        // Warmup period
                        sleep(Duration::from_millis(warmup_time)).await;

                        // Message sending loop
                        while Instant::now() < end_time {
                            let msg_start = Instant::now();
                            
                            match client.simple_unary_call("echo", &payload).await {
                                Ok(_response) => {
                                    let latency = msg_start.elapsed();
                                    client_metrics.latencies_ms.push(latency.as_secs_f64() * 1000.0);
                                    client_metrics.messages_sent += 1;
                                    client_metrics.messages_received += 1;
                                }
                                Err(_) => {
                                    client_metrics.message_failures += 1;
                                }
                            }

                            // Minimal delay to prevent overwhelming
                            sleep(Duration::from_micros(100)).await;
                        }
                    }
                    Err(_) => {
                        client_metrics.connection_successful = false;
                    }
                }

                let _ = tx.send(client_metrics).await;
                Ok::<_, QuicCrunchError>(())
            });

            client_handles.push(handle);
        }

        // Drop the original sender
        drop(tx);

        // Collect metrics from all clients
        while let Some(client_metrics) = rx.recv().await {
            if client_metrics.connection_successful {
                successful_connections += 1;
                connection_times.push(client_metrics.connection_time_ms);
            } else {
                connection_failures += 1;
            }

            total_messages_sent += client_metrics.messages_sent;
            total_messages_received += client_metrics.messages_received;
            latencies.extend(client_metrics.latencies_ms);
        }

        // Wait for all clients to complete
        for handle in client_handles {
            let _ = handle.await;
        }

        let actual_duration = test_start.elapsed().as_secs_f64();

        // Calculate metrics
        let metrics = self.calculate_metrics(
            message_size,
            actual_duration,
            &latencies,
            &connection_times,
            total_messages_sent,
            total_messages_received,
            successful_connections,
            connection_failures,
        );

        Ok(metrics)
    }

    fn calculate_metrics(
        &self,
        message_size: usize,
        duration: f64,
        latencies: &[f64],
        connection_times: &[f64],
        total_sent: u64,
        total_received: u64,
        successful_connections: u64,
        connection_failures: u64,
    ) -> NetworkMetrics {
        // Latency calculations
        let mut sorted_latencies = latencies.to_vec();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let (min_lat, max_lat, avg_lat, p50_lat, p95_lat, p99_lat) = if !sorted_latencies.is_empty() {
            let min = sorted_latencies[0];
            let max = sorted_latencies[sorted_latencies.len() - 1];
            let avg = sorted_latencies.iter().sum::<f64>() / sorted_latencies.len() as f64;
            let p50 = sorted_latencies[sorted_latencies.len() * 50 / 100];
            let p95 = sorted_latencies[sorted_latencies.len() * 95 / 100];
            let p99 = sorted_latencies[sorted_latencies.len() * 99 / 100];
            (min, max, avg, p50, p95, p99)
        } else {
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
        };

        // Connection time calculation
        let avg_connection_time = if !connection_times.is_empty() {
            connection_times.iter().sum::<f64>() / connection_times.len() as f64
        } else {
            0.0
        };

        // Throughput calculations
        let messages_per_second = total_received as f64 / duration;
        let bytes_per_second = (total_received * message_size as u64) as f64 / duration;
        let megabits_per_second = bytes_per_second * 8.0 / 1_000_000.0;

        // Loss rate
        let message_loss_rate = if total_sent > 0 {
            ((total_sent - total_received) as f64 / total_sent as f64) * 100.0
        } else {
            0.0
        };

        NetworkMetrics {
            test_name: format!("network_baseline_{}bytes", message_size),
            message_size_bytes: message_size,
            concurrent_clients: self.config.concurrent_clients,
            duration_seconds: duration,
            
            total_connections: successful_connections + connection_failures,
            successful_connections,
            connection_failures,
            avg_connection_time_ms: avg_connection_time,
            
            total_messages_sent: total_sent,
            total_messages_received: total_received,
            message_loss_rate,
            
            min_latency_ms: min_lat,
            max_latency_ms: max_lat,
            avg_latency_ms: avg_lat,
            p50_latency_ms: p50_lat,
            p95_latency_ms: p95_lat,
            p99_latency_ms: p99_lat,
            
            messages_per_second,
            bytes_per_second,
            megabits_per_second,
            
            peak_memory_mb: 0.0, // TODO: Add memory monitoring
            avg_cpu_percent: 0.0, // TODO: Add CPU monitoring
        }
    }

    fn generate_summary(&self, metrics: &HashMap<usize, NetworkMetrics>, total_duration: f64) -> BaselineSummary {
        let mut peak_msgs_per_sec = 0.0;
        let mut peak_mbps = 0.0;
        let mut optimal_size = 1024;
        let mut total_latency = 0.0;
        let mut total_connections = 0u64;
        let mut successful_connections = 0u64;

        for (size, metric) in metrics {
            if metric.messages_per_second > peak_msgs_per_sec {
                peak_msgs_per_sec = metric.messages_per_second;
            }
            if metric.megabits_per_second > peak_mbps {
                peak_mbps = metric.megabits_per_second;
                optimal_size = *size;
            }
            total_latency += metric.avg_latency_ms;
            total_connections += metric.total_connections;
            successful_connections += metric.successful_connections;
        }

        let avg_latency = if !metrics.is_empty() {
            total_latency / metrics.len() as f64
        } else {
            0.0
        };

        let connection_success_rate = if total_connections > 0 {
            (successful_connections as f64 / total_connections as f64) * 100.0
        } else {
            0.0
        };

        BaselineSummary {
            total_test_duration_seconds: total_duration,
            peak_messages_per_second: peak_msgs_per_sec,
            peak_megabits_per_second: peak_mbps,
            optimal_message_size_bytes: optimal_size,
            avg_latency_across_sizes_ms: avg_latency,
            connection_success_rate,
        }
    }

    fn print_results(&self, results: &BaselineResults) {
        println!("\n🎯 CLIENT-SERVER NETWORK BASELINE RESULTS");
        println!("═════════════════════════════════════════");
        println!("Test completed: {}", results.timestamp);
        println!("Total duration: {:.1}s", results.summary.total_test_duration_seconds);
        println!();

        println!("📊 SUMMARY METRICS:");
        println!("  Peak performance: {:.0} msg/s, {:.1} Mbps", 
            results.summary.peak_messages_per_second,
            results.summary.peak_megabits_per_second);
        println!("  Optimal message size: {} bytes", results.summary.optimal_message_size_bytes);
        println!("  Average latency: {:.2} ms", results.summary.avg_latency_across_sizes_ms);
        println!("  Connection success rate: {:.1}%", results.summary.connection_success_rate);
        println!();

        println!("📈 DETAILED RESULTS BY MESSAGE SIZE:");
        println!("Size(B) | Msg/s  | Mbps | Lat(ms) | P95(ms) | P99(ms) | Loss%");
        println!("--------|--------|------|---------|---------|---------|------");
        
        let mut sizes: Vec<_> = results.metrics_by_message_size.keys().collect();
        sizes.sort();
        
        for &size in sizes {
            if let Some(metrics) = results.metrics_by_message_size.get(&size) {
                println!(
                    "{:7} | {:6.0} | {:4.1} | {:7.2} | {:7.2} | {:7.2} | {:5.1}",
                    size,
                    metrics.messages_per_second,
                    metrics.megabits_per_second,
                    metrics.avg_latency_ms,
                    metrics.p95_latency_ms,
                    metrics.p99_latency_ms,
                    metrics.message_loss_rate
                );
            }
        }
        println!();

        println!("💾 BASELINE DATA SAVED");
        println!("This baseline can be used for performance regression testing");
        println!("and comparing against future optimizations.");
    }
}

#[derive(Debug, Default)]
struct ClientMetrics {
    connection_successful: bool,
    connection_time_ms: f64,
    messages_sent: u64,
    messages_received: u64,
    message_failures: u64,
    latencies_ms: Vec<f64>,
}

// Simple echo server implementation for QuicCrunchServer
impl QuicCrunchServer {
    async fn run_simple_echo_server(&self) -> Result<()> {
        loop {
            match self.accept_connection().await {
                Ok(mut connection) => {
                    tokio::spawn(async move {
                        loop {
                            match connection.receive_message().await {
                                Ok(message) => {
                                    // Simple echo - just send back the same data
                                    if let Err(e) = connection.send_message(&message).await {
                                        eprintln!("Failed to echo message: {}", e);
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {}", e);
                    break;
                }
            }
        }
    }
}

// Simple unary call implementation for QuicCrunchClient
impl QuicCrunchClient {
    async fn simple_unary_call(&self, _method: &str, data: &[u8]) -> Result<Vec<u8>> {
        // Send the data and wait for response
        self.send_message(data).await?;
        let response = self.receive_message().await?;
        Ok(response)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔬 QuicCrunch Client-Server Network Baseline Benchmark");
    println!("======================================================");
    
    let config = NetworkBenchmarkConfig::default();
    let mut benchmark = ClientServerNetworkBenchmark::new(config);
    
    match benchmark.run_baseline_benchmark().await {
        Ok(results) => {
            // Save results to file
            let json_results = serde_json::to_string_pretty(&results)?;
            let filename = format!("network_baseline_{}.json", 
                chrono::Utc::now().format("%Y%m%d_%H%M%S"));
            
            std::fs::write(&filename, json_results)?;
            println!("📁 Results saved to: {}", filename);
            
            println!("\n✅ Network baseline benchmark completed successfully!");
        }
        Err(e) => {
            eprintln!("❌ Benchmark failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_benchmark_config() {
        let config = NetworkBenchmarkConfig::default();
        assert_eq!(config.test_duration_secs, 30);
        assert_eq!(config.concurrent_clients, 4);
        assert!(!config.message_sizes.is_empty());
    }

    #[tokio::test]
    async fn test_benchmark_creation() {
        let config = NetworkBenchmarkConfig::default();
        let benchmark = ClientServerNetworkBenchmark::new(config);
        assert_eq!(benchmark.server_addr, "127.0.0.1:0");
    }
}