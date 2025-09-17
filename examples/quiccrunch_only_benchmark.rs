//! QuicCrunch Performance Benchmark
//! 
//! This benchmark tests QuicCrunch performance with various message sizes.

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use bytes::Bytes;
use quiccrunch_core::{QuicTransport, Transport};
use quiccrunch_core::transport::{tquic::TlsConfig, Listener};
use quiccrunch_core::connection_pool::{ConnectionPool, ConnectionPoolConfig};

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub message_sizes: Vec<usize>,
    pub requests_per_size: usize,
    pub concurrent_clients: usize,
    pub test_duration: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            message_sizes: vec![64, 1024, 4096, 16384, 65536], // Range from small to large
            requests_per_size: 100,
            concurrent_clients: 4,
            test_duration: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub struct BenchmarkResults {
    pub protocol: String,
    pub message_size: usize,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub requests_per_second: f64,
    pub throughput_mbps: f64,
    pub test_duration: Duration,
    pub connection_pool_hit_rate: f64,
    pub connection_pool_hits: u64,
    pub connection_pool_misses: u64,
}

struct LatencyTracker {
    latencies: Vec<u64>,
}

impl LatencyTracker {
    fn new() -> Self {
        Self {
            latencies: Vec::new(),
        }
    }

    fn record(&mut self, latency_ns: u64) {
        self.latencies.push(latency_ns);
    }

    fn calculate_stats(&mut self) -> (f64, f64, f64) {
        if self.latencies.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        self.latencies.sort_unstable();
        
        let sum: u64 = self.latencies.iter().sum();
        let avg_ns = sum as f64 / self.latencies.len() as f64;
        let min_ns = *self.latencies.first().unwrap() as f64;
        let max_ns = *self.latencies.last().unwrap() as f64;

        // Convert nanoseconds to milliseconds
        (
            avg_ns / 1_000_000.0,
            min_ns / 1_000_000.0,
            max_ns / 1_000_000.0,
        )
    }
}

async fn run_quiccrunch_echo_server(server_addr: String) -> quiccrunch_core::Result<()> {
    println!("🖥️  Starting QuicCrunch echo server on {}", server_addr);
    
    let transport = QuicTransport::new(TlsConfig::default().into()).await?;
    let mut listener = transport.listen(&server_addr).await?;
    
    println!("✅ QuicCrunch server listening and ready");
    
    while let Ok(mut connection) = listener.accept().await {
        tokio::spawn(async move {
            // Accept streams from this connection
            while let Ok(mut stream) = connection.accept_bi().await {
                tokio::spawn(async move {
                    // Simple echo server - echo each message back
                    while let Ok(message) = stream.receive_message().await {
                        if stream.send_message(message).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}

async fn run_quiccrunch_benchmark(
    config: &BenchmarkConfig,
    message_size: usize,
    server_addr: &str,
) -> quiccrunch_core::Result<BenchmarkResults> {
    // Initialize connection pool
    let transport = Arc::new(QuicTransport::new(TlsConfig::default().into()).await?);
    let pool_config = ConnectionPoolConfig {
        max_connections_per_endpoint: config.concurrent_clients * 2,
        max_total_connections: config.concurrent_clients * 4,
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(60),
        max_connection_age: Duration::from_secs(1800),
        enable_health_checks: true,
    };
    let connection_pool = Arc::new(ConnectionPool::new(transport, pool_config));
    
    let start_time = Instant::now();
    let total_requests = Arc::new(AtomicU64::new(0));
    let successful_requests = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    
    let all_latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    for client_id in 0..config.concurrent_clients {
        let config = config.clone();
        let total_requests = Arc::clone(&total_requests);
        let successful_requests = Arc::clone(&successful_requests);
        let all_latencies = Arc::clone(&all_latencies);
        let connection_pool = Arc::clone(&connection_pool);
        let server_addr = server_addr.to_string();
        
        let handle = tokio::spawn(async move {
            // Use connection pool to get connections
            let mut connection = match connection_pool.get_connection(&server_addr).await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to get connection from pool: {}", e);
                    return Err(format!("Connection pool failed: {}", e));
                }
            };
            
            let end_time = start_time + config.test_duration;
            let payload = Bytes::from(vec![42u8; message_size]); // Simple test data
            let mut local_latencies = Vec::new();
            let mut local_requests = 0u64;
            let mut local_successful = 0u64;
            
            // Run benchmark until time expires or max requests reached
            while Instant::now() < end_time && local_requests < config.requests_per_size as u64 {
                let request_start = Instant::now();
                
                // Open a new stream for each request
                match connection.open_bi().await {
                    Ok(mut stream) => {
                        // Send message and wait for echo
                        match stream.send_message(payload.clone()).await {
                            Ok(_) => {
                                match stream.receive_message().await {
                                    Ok(_) => {
                                        local_successful += 1;
                                        let latency_ns = request_start.elapsed().as_nanos() as u64;
                                        local_latencies.push(latency_ns);
                                    },
                                    Err(_) => break,
                                }
                            },
                            Err(_) => break,
                        }
                    },
                    Err(_) => break,
                }
                local_requests += 1;
                
                if client_id == 0 && local_requests % 20 == 0 {
                    print!(".");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            }
            
            // Return connection to pool
            connection_pool.return_connection(server_addr, connection).await;
            
            // Aggregate results
            total_requests.fetch_add(local_requests, Ordering::Relaxed);
            successful_requests.fetch_add(local_successful, Ordering::Relaxed);
            all_latencies.lock().await.extend(local_latencies);
            
            Ok::<(), String>(())
        });
        
        handles.push(handle);
    }
    
    // Wait for all clients to complete
    for handle in handles {
        if let Err(e) = handle.await {
            eprintln!("Client task failed: {:?}", e);
        }
    }
    
    let actual_duration = start_time.elapsed();
    let total_reqs = total_requests.load(Ordering::Relaxed);
    let successful_reqs = successful_requests.load(Ordering::Relaxed);
    
    // Get connection pool statistics
    let pool_stats = connection_pool.stats().await;
    
    // Calculate latency statistics
    let latencies = all_latencies.lock().await;
    let mut latency_tracker = LatencyTracker::new();
    for &latency in latencies.iter() {
        latency_tracker.record(latency);
    }
    let (avg_latency, min_latency, max_latency) = latency_tracker.calculate_stats();
    
    let requests_per_second = total_reqs as f64 / actual_duration.as_secs_f64();
    let throughput_mbps = (successful_reqs * message_size as u64) as f64 
        / actual_duration.as_secs_f64() / 1024.0 / 1024.0;
    
    Ok(BenchmarkResults {
        protocol: "QuicCrunch".to_string(),
        message_size,
        total_requests: total_reqs,
        successful_requests: successful_reqs,
        failed_requests: total_reqs - successful_reqs,
        average_latency_ms: avg_latency,
        min_latency_ms: min_latency,
        max_latency_ms: max_latency,
        requests_per_second,
        throughput_mbps,
        test_duration: actual_duration,
        connection_pool_hit_rate: pool_stats.hit_rate,
        connection_pool_hits: pool_stats.hit_count,
        connection_pool_misses: pool_stats.miss_count,
    })
}

fn print_results(results: &[BenchmarkResults]) {
    println!();
    println!("📊 QUICCRUNCH PERFORMANCE RESULTS");
    println!("==================================");
    println!();
    
    println!("{:<12} {:<8} {:<10} {:<10} {:<12} {:<12}", 
        "Msg Size", "RPS", "Avg Lat", "Min Lat", "Max Lat", "Throughput");
    println!("{}", "-".repeat(70));
    
    for result in results {
        println!("{:<12} {:<8.0} {:<10.2}ms {:<10.2}ms {:<12.2}ms {:<12.1}MB/s",
            format!("{}B", result.message_size),
            result.requests_per_second,
            result.average_latency_ms,
            result.min_latency_ms,
            result.max_latency_ms,
            result.throughput_mbps);
            
        // Show connection pool benefits
        if result.connection_pool_hits + result.connection_pool_misses > 0 {
            println!("  -> Connection pool hit rate: {:.1}% ({} hits, {} misses)",
                result.connection_pool_hit_rate, result.connection_pool_hits, result.connection_pool_misses);
        }
        
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🚀 QuicCrunch Performance Benchmark");
    println!("===================================");
    println!("Testing QuicCrunch with connection pooling");
    println!();
    
    let config = BenchmarkConfig::default();
    let server_addr = "127.0.0.1:58725";
    
    // Start QuicCrunch server
    let server_addr_clone = server_addr.to_string();
    let quic_server_handle = tokio::spawn(async move {
        if let Err(e) = run_quiccrunch_echo_server(server_addr_clone).await {
            eprintln!("QuicCrunch server error: {}", e);
        }
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(2000)).await;
    println!("✅ QuicCrunch server started");
    
    let mut results = Vec::new();
    
    for &message_size in &config.message_sizes {
        println!("📏 Testing {} byte messages", message_size);
        
        print!("  Running QuicCrunch benchmark... ");
        match run_quiccrunch_benchmark(&config, message_size, server_addr).await {
            Ok(result) => {
                println!("✅ Done");
                results.push(result);
            },
            Err(e) => {
                println!("❌ Failed: {}", e);
                return Err(format!("QuicCrunch benchmark failed: {}", e).into());
            }
        }
        
        // Brief pause between tests
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
    // Print results
    print_results(&results);
    
    // Cleanup
    quic_server_handle.abort();
    
    // Summary
    println!("💡 KEY INSIGHTS:");
    println!("   - QuicCrunch uses connection pooling for better resource efficiency");
    println!("   - Connection pool hit rates improve with repeated connections");
    println!("   - QUIC protocol provides built-in encryption and reliability");
    println!("   - Performance varies by message size and network conditions");
    
    Ok(())
}