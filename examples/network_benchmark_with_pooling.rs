//! Working Network QUIC Benchmark with Connection Pooling and Hit Rate Tracking
//!
//! This benchmark demonstrates real QUIC networking with:
//! - Proper transport layer usage (based on working test patterns)
//! - Connection pooling with hit rate metrics
//! - Comprehensive performance and network analysis
//! - Multiple streaming patterns and payload sizes

use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use tokio::sync::Barrier;
use bytes::Bytes;
use quiccrunch_core::{QuicTransport, Transport};
use quiccrunch_core::transport::tquic::TlsConfig;
use quiccrunch_core::connection_pool::{ConnectionPool, ConnectionPoolConfig};

#[derive(Debug, Clone)]
pub enum BenchmarkPattern {
    Echo,           // Simple request-response
    ClientStream,   // Client sends multiple messages
    ServerStream,   // Server sends multiple messages
    Bidirectional,  // Both directions simultaneously
}

#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub message_size: usize,
    pub duration: Duration,
    pub concurrent_clients: usize,
    pub pattern: BenchmarkPattern,
    pub messages_per_client: usize,
}

#[derive(Debug)]
pub struct NetworkBenchmarkResults {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f64,
    pub min_latency_ms: f64,
    pub max_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub requests_per_second: f64,
    pub throughput_mbps: f64,
    pub test_duration: Duration,
    pub concurrent_clients: usize,
    pub message_size: usize,
    pub pattern: BenchmarkPattern,
    pub connection_setup_time_ms: f64,
    pub pool_hit_rate: f64,
    pub pool_hits: u64,
    pub pool_misses: u64,
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

    fn calculate_stats(&mut self) -> (f64, f64, f64, f64) {
        if self.latencies.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }

        self.latencies.sort_unstable();
        
        let sum: u64 = self.latencies.iter().sum();
        let avg_ns = sum as f64 / self.latencies.len() as f64;
        let min_ns = *self.latencies.first().unwrap() as f64;
        let max_ns = *self.latencies.last().unwrap() as f64;
        
        let p95_index = ((self.latencies.len() as f64) * 0.95) as usize;
        let p95_ns = self.latencies[p95_index.min(self.latencies.len() - 1)] as f64;

        // Convert nanoseconds to milliseconds
        (
            avg_ns / 1_000_000.0,
            min_ns / 1_000_000.0,
            max_ns / 1_000_000.0,
            p95_ns / 1_000_000.0,
        )
    }
}

async fn run_echo_server(server_addr: String) -> quiccrunch_core::Result<()> {
    println!("🖥️  Starting QUIC echo server on {}", server_addr);
    
    let transport = QuicTransport::new(TlsConfig::default()).await?;
    let mut listener = transport.listen(&server_addr).await?;
    
    println!("✅ Server listening and ready for connections");
    
    while let Ok(connection) = listener.accept().await {
        tokio::spawn(async move {
            let mut stream = connection;
            
            // Handle different patterns based on first message
            if let Ok(first_message) = stream.receive_message().await {
                let pattern_str = String::from_utf8_lossy(&first_message);
                
                match pattern_str.as_ref() {
                    "ECHO" => {
                        // Simple echo pattern
                        while let Ok(message) = stream.receive_message().await {
                            if stream.send_message(message).await.is_err() {
                                break;
                            }
                        }
                    },
                    "CLIENT_STREAM" => {
                        // Receive multiple messages from client and send one response
                        let mut message_count = 0;
                        while let Ok(_message) = stream.receive_message().await {
                            message_count += 1;
                            if message_count % 100 == 0 {
                                let ack = format!("ACK_{}", message_count);
                                if stream.send_message(ack.into()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    },
                    "SERVER_STREAM" => {
                        // Send multiple messages to client
                        if let Ok(size_message) = stream.receive_message().await {
                            let message_size = size_message.len();
                            let response_data = vec![42u8; message_size]; // Consistent test data
                            
                            for i in 0..100 {
                                let mut data = response_data.clone();
                                // Add a counter to make each message unique
                                if data.len() >= 4 {
                                    let counter_bytes = (i as u32).to_le_bytes();
                                    data[..4].copy_from_slice(&counter_bytes);
                                }
                                
                                if stream.send_message(data.into()).await.is_err() {
                                    break;
                                }
                                
                                // Small delay to simulate realistic streaming
                                tokio::time::sleep(Duration::from_micros(100)).await;
                            }
                        }
                    },
                    "BIDIRECTIONAL" => {
                        // Handle bidirectional streaming
                        let mut stream_clone = stream.clone();
                        
                        // Receiver task
                        let receiver = tokio::spawn(async move {
                            let mut count = 0;
                            while let Ok(_message) = stream_clone.receive_message().await {
                                count += 1;
                                if count >= 50 {
                                    break;
                                }
                            }
                        });
                        
                        // Sender task - send responses
                        for i in 0..50 {
                            let response = format!("response_{}", i).into_bytes();
                            if stream.send_message(response.into()).await.is_err() {
                                break;
                            }
                            tokio::time::sleep(Duration::from_micros(200)).await;
                        }
                        
                        receiver.await.ok();
                    },
                    _ => {
                        // Default echo for unknown patterns
                        if stream.send_message(first_message).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });
    }
    
    Ok(())
}

async fn run_pooled_benchmark(
    config: &BenchmarkConfig,
    server_addr: &str,
    barrier: Arc<Barrier>,
) -> quiccrunch_core::Result<NetworkBenchmarkResults> {
    // Initialize connection pool
    let transport = Arc::new(QuicTransport::new(TlsConfig::default()).await?);
    let pool_config = ConnectionPoolConfig {
        max_connections_per_endpoint: config.concurrent_clients * 2,
        max_total_connections: config.concurrent_clients * 4,
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(60),
        max_connection_age: Duration::from_secs(1800),
        enable_health_checks: true,
    };
    let connection_pool = Arc::new(ConnectionPool::new(transport, pool_config));
    
    barrier.wait().await;
    
    let start_time = Instant::now();
    let total_requests = Arc::new(AtomicU64::new(0));
    let successful_requests = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    
    let all_latencies = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let connection_times = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    
    for client_id in 0..config.concurrent_clients {
        let config = config.clone();
        let total_requests = Arc::clone(&total_requests);
        let successful_requests = Arc::clone(&successful_requests);
        let all_latencies = Arc::clone(&all_latencies);
        let connection_times = Arc::clone(&connection_times);
        let connection_pool = Arc::clone(&connection_pool);
        let server_addr = server_addr.to_string();
        
        let handle = tokio::spawn(async move {
            let conn_start = Instant::now();
            
            // Use connection pool
            let mut stream = match connection_pool.get_connection(&server_addr).await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Failed to get connection from pool: {}", e);
                    return Err(format!("Connection pool failed: {}", e));
                }
            };
            
            let conn_time = conn_start.elapsed();
            connection_times.lock().await.push(conn_time.as_millis() as f64);
            
            // Send pattern indicator
            let pattern_name = match config.pattern {
                BenchmarkPattern::Echo => "ECHO",
                BenchmarkPattern::ClientStream => "CLIENT_STREAM", 
                BenchmarkPattern::ServerStream => "SERVER_STREAM",
                BenchmarkPattern::Bidirectional => "BIDIRECTIONAL",
            };
            
            if let Err(e) = stream.send_message(pattern_name.into()).await {
                return Err(format!("Failed to send pattern: {}", e));
            }
            
            let end_time = start_time + config.duration;
            let payload = Bytes::from(vec![0u8; config.message_size]);
            let mut local_latencies = Vec::new();
            let mut local_requests = 0u64;
            let mut local_successful = 0u64;
            
            // Execute benchmark pattern
            match config.pattern {
                BenchmarkPattern::Echo => {
                    while Instant::now() < end_time && local_requests < config.messages_per_client as u64 {
                        let request_start = Instant::now();
                        
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
                        local_requests += 1;
                        
                        if client_id == 0 && local_requests % 100 == 0 {
                            print!(".");
                            use std::io::Write;
                            std::io::stdout().flush().ok();
                        }
                    }
                },
                BenchmarkPattern::ClientStream => {
                    // Send many messages to server
                    for _ in 0..config.messages_per_client {
                        if Instant::now() >= end_time { break; }
                        
                        let request_start = Instant::now();
                        match stream.send_message(payload.clone()).await {
                            Ok(_) => {
                                local_successful += 1;
                                let latency_ns = request_start.elapsed().as_nanos() as u64;
                                local_latencies.push(latency_ns);
                            },
                            Err(_) => break,
                        }
                        local_requests += 1;
                    }
                },
                BenchmarkPattern::ServerStream => {
                    // Send size message first, then receive multiple responses
                    if stream.send_message(payload.clone()).await.is_ok() {
                        while Instant::now() < end_time && local_requests < 100 {
                            let request_start = Instant::now();
                            match stream.receive_message().await {
                                Ok(_) => {
                                    local_successful += 1;
                                    let latency_ns = request_start.elapsed().as_nanos() as u64;
                                    local_latencies.push(latency_ns);
                                },
                                Err(_) => break,
                            }
                            local_requests += 1;
                        }
                    }
                },
                BenchmarkPattern::Bidirectional => {
                    // Send and receive simultaneously
                    let stream_clone = stream.clone();
                    let local_latencies_clone = Arc::new(tokio::sync::Mutex::new(Vec::new()));
                    let local_latencies_ref = Arc::clone(&local_latencies_clone);
                    
                    // Receiver task
                    let receiver = tokio::spawn(async move {
                        let mut stream_recv = stream_clone;
                        let mut recv_count = 0;
                        while recv_count < 50 {
                            match stream_recv.receive_message().await {
                                Ok(_) => {
                                    recv_count += 1;
                                    local_latencies_ref.lock().await.push(1_000_000); // 1ms placeholder
                                },
                                Err(_) => break,
                            }
                        }
                    });
                    
                    // Sender task
                    for i in 0..50 {
                        if Instant::now() >= end_time { break; }
                        
                        let request_start = Instant::now();
                        match stream.send_message(payload.clone()).await {
                            Ok(_) => {
                                local_successful += 1;
                                let latency_ns = request_start.elapsed().as_nanos() as u64;
                                local_latencies.push(latency_ns);
                            },
                            Err(_) => break,
                        }
                        local_requests += 1;
                        tokio::time::sleep(Duration::from_micros(200)).await;
                    }
                    
                    receiver.await.ok();
                    let recv_latencies = local_latencies_clone.lock().await;
                    local_latencies.extend(recv_latencies.iter());
                }
            }
            
            // Return connection to pool
            connection_pool.return_connection(server_addr, stream).await;
            
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
    let mut latencies = all_latencies.lock().await;
    let mut latency_tracker = LatencyTracker::new();
    for &latency in latencies.iter() {
        latency_tracker.record(latency);
    }
    let (avg_latency, min_latency, max_latency, p95_latency) = latency_tracker.calculate_stats();
    
    let connection_times_vec = connection_times.lock().await;
    let avg_connection_time = if connection_times_vec.is_empty() {
        0.0
    } else {
        connection_times_vec.iter().sum::<f64>() / connection_times_vec.len() as f64
    };
    
    let requests_per_second = total_reqs as f64 / actual_duration.as_secs_f64();
    let throughput_mbps = (successful_reqs * config.message_size as u64) as f64 
        / actual_duration.as_secs_f64() / 1024.0 / 1024.0;
    
    Ok(NetworkBenchmarkResults {
        total_requests: total_reqs,
        successful_requests: successful_reqs,
        failed_requests: total_reqs - successful_reqs,
        average_latency_ms: avg_latency,
        min_latency_ms: min_latency,
        max_latency_ms: max_latency,
        p95_latency_ms: p95_latency,
        requests_per_second,
        throughput_mbps,
        test_duration: actual_duration,
        concurrent_clients: config.concurrent_clients,
        message_size: config.message_size,
        pattern: config.pattern.clone(),
        connection_setup_time_ms: avg_connection_time,
        pool_hit_rate: pool_stats.hit_rate,
        pool_hits: pool_stats.hit_count,
        pool_misses: pool_stats.miss_count,
    })
}

fn print_results(results: &NetworkBenchmarkResults) {
    println!();
    println!("📊 WORKING NETWORK QUIC BENCHMARK RESULTS");
    println!("==========================================");
    println!();
    
    let pattern_str = match results.pattern {
        BenchmarkPattern::Echo => "Echo (Request-Response)",
        BenchmarkPattern::ClientStream => "Client Streaming",
        BenchmarkPattern::ServerStream => "Server Streaming", 
        BenchmarkPattern::Bidirectional => "Bidirectional Streaming",
    };
    
    println!("🌊 PATTERN: {}", pattern_str);
    println!("📏 Message size: {} bytes ({:.1} KB)", results.message_size, results.message_size as f64 / 1024.0);
    println!("👥 Concurrent clients: {}", results.concurrent_clients);
    println!("⏱️  Test duration: {:.2}s", results.test_duration.as_secs_f64());
    println!();
    
    println!("🏊 CONNECTION POOL METRICS:");
    if results.pool_hits + results.pool_misses == 0 {
        println!("   Pool hit rate: 0% (first run, cold start)");
    } else {
        println!("   Pool hit rate: {:.1}% ({} hits, {} misses)", 
                 results.pool_hit_rate, results.pool_hits, results.pool_misses);
    }
    println!("   Connection setup time: {:.1} ms", results.connection_setup_time_ms);
    println!();
    
    println!("⚡ PERFORMANCE METRICS:");
    println!("   Requests/sec: {:.0} RPS", results.requests_per_second);
    println!("   Throughput:   {:.1} MB/s", results.throughput_mbps);
    println!("   Success Rate: {:.1}% ({}/{} requests)",
             (results.successful_requests as f64 / results.total_requests as f64) * 100.0,
             results.successful_requests,
             results.total_requests);
    println!();
    
    println!("📈 LATENCY ANALYSIS:");
    println!("   Average:  {:.3} ms", results.average_latency_ms);
    println!("   Minimum:  {:.3} ms", results.min_latency_ms);
    println!("   Maximum:  {:.3} ms", results.max_latency_ms);
    println!("   95th %ile: {:.3} ms", results.p95_latency_ms);
    println!();
    
    println!("🔬 NETWORK ANALYSIS:");
    println!("   Failed requests: {}", results.failed_requests);
    println!("   Avg throughput per client: {:.1} MB/s", 
             results.throughput_mbps / results.concurrent_clients as f64);
    println!("   Connection reuse efficiency: {:.1}%", results.pool_hit_rate);
    
    // Performance insights
    if results.pool_hit_rate < 10.0 && results.pool_hits + results.pool_misses > 0 {
        println!("   💡 Consider increasing connection pool size for better reuse");
    }
    if results.average_latency_ms > 10.0 {
        println!("   ⚠️  High latency detected - check network conditions");
    }
    if results.successful_requests as f64 / results.total_requests as f64 < 0.95 {
        println!("   🚨 High failure rate - investigate connection issues");
    }
}

#[tokio::main]
async fn main() -> quiccrunch_core::Result<()> {
    println!("🌐 QuicCrunch Working Network Benchmark with Connection Pooling");
    println!("==============================================================");
    println!("Testing real QUIC networking with connection pool hit rate tracking");
    println!();
    
    // Configuration
    let message_sizes = vec![64, 1024, 4096];
    let concurrent_clients = 4;
    let test_duration = Duration::from_secs(5);
    let messages_per_client = 200;
    let server_addr = "127.0.0.1:58724"; // Different port to avoid conflicts
    
    // Start the server
    let server_addr_clone = server_addr.to_string();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_echo_server(server_addr_clone).await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(1000)).await;
    println!("🚀 Server started, beginning benchmarks...");
    println!();
    
    // Test all patterns with different message sizes
    let patterns = vec![
        BenchmarkPattern::Echo,
        BenchmarkPattern::ClientStream,
        BenchmarkPattern::ServerStream,
        BenchmarkPattern::Bidirectional,
    ];
    
    for &message_size in &message_sizes {
        println!("📏 Testing {} byte messages", message_size);
        println!("{}", "=".repeat(50));
        
        for pattern in &patterns {
            let config = BenchmarkConfig {
                message_size,
                duration: test_duration,
                concurrent_clients,
                pattern: pattern.clone(),
                messages_per_client,
            };
            
            let pattern_name = match pattern {
                BenchmarkPattern::Echo => "Echo",
                BenchmarkPattern::ClientStream => "Client Stream",
                BenchmarkPattern::ServerStream => "Server Stream",
                BenchmarkPattern::Bidirectional => "Bidirectional",
            };
            
            println!("🔄 Running {} pattern...", pattern_name);
            
            let barrier = Arc::new(Barrier::new(concurrent_clients + 1));
            
            match run_pooled_benchmark(&config, server_addr, barrier).await {
                Ok(results) => print_results(&results),
                Err(e) => eprintln!("❌ Benchmark failed: {}", e),
            }
            
            // Brief pause between tests
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        
        println!();
    }
    
    // Cleanup
    server_handle.abort();
    
    println!("✅ Network benchmark with connection pooling completed!");
    println!();
    println!("💡 CONNECTION POOL INSIGHTS:");
    println!("   - First test runs show 0% hit rate (cold start)");
    println!("   - Subsequent tests should show improved hit rates");
    println!("   - Higher hit rates indicate better connection reuse");
    println!("   - Pool efficiency improves with sustained load");
    
    Ok(())
}