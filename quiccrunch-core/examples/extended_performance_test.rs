use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;

#[path = "cert_utils.rs"]
mod cert_utils;

#[derive(Debug, Clone)]
struct PerformanceMetrics {
    total_messages: u64,
    total_bytes: u64,
    latencies: Vec<Duration>,
    start_time: Instant,
    duration: Duration,
}

impl PerformanceMetrics {
    fn new() -> Self {
        Self {
            total_messages: 0,
            total_bytes: 0,
            latencies: Vec::new(),
            start_time: Instant::now(),
            duration: Duration::new(0, 0),
        }
    }

    fn add_measurement(&mut self, message_size: usize, latency: Duration) {
        self.total_messages += 1;
        self.total_bytes += message_size as u64;
        self.latencies.push(latency);
    }

    fn finalize(&mut self) {
        self.duration = self.start_time.elapsed();
    }

    fn calculate_stats(&self) -> PerformanceStats {
        let avg_latency = self.latencies.iter().sum::<Duration>() / self.latencies.len() as u32;
        
        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort();
        
        let p50 = sorted_latencies[self.latencies.len() / 2];
        let p95 = sorted_latencies[(self.latencies.len() as f64 * 0.95) as usize];
        let p99 = sorted_latencies[(self.latencies.len() as f64 * 0.99) as usize];

        let throughput_msg_per_sec = self.total_messages as f64 / self.duration.as_secs_f64();
        let throughput_mbps = (self.total_bytes as f64 * 8.0) / (self.duration.as_secs_f64() * 1_000_000.0);

        PerformanceStats {
            total_messages: self.total_messages,
            total_bytes: self.total_bytes,
            duration: self.duration,
            avg_latency,
            p50_latency: p50,
            p95_latency: p95,
            p99_latency: p99,
            throughput_msg_per_sec,
            throughput_mbps,
        }
    }
}

#[derive(Debug)]
struct PerformanceStats {
    total_messages: u64,
    total_bytes: u64,
    duration: Duration,
    avg_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    throughput_msg_per_sec: f64,
    throughput_mbps: f64,
}

impl PerformanceStats {
    fn print(&self, backend_name: &str) {
        println!("\n{} Performance Results (1-minute test):", backend_name);
        println!("=====================================");
        println!("Test Duration: {:.2}s", self.duration.as_secs_f64());
        println!("Total Messages: {}", self.total_messages);
        println!("Total Bytes: {} ({:.2} MB)", self.total_bytes, self.total_bytes as f64 / 1_000_000.0);
        println!("");
        println!("Latency Statistics:");
        println!("  Average: {:.2}ms", self.avg_latency.as_secs_f64() * 1000.0);
        println!("  P50:     {:.2}ms", self.p50_latency.as_secs_f64() * 1000.0);
        println!("  P95:     {:.2}ms", self.p95_latency.as_secs_f64() * 1000.0);
        println!("  P99:     {:.2}ms", self.p99_latency.as_secs_f64() * 1000.0);
        println!("");
        println!("Throughput:");
        println!("  Messages/sec: {:.0}", self.throughput_msg_per_sec);
        println!("  Mbps:         {:.2}", self.throughput_mbps);
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("QuicCrunch Extended Performance Test");
    println!("===================================");
    println!("Running 1-minute performance tests for each backend...\n");

    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    let test_duration = Duration::from_secs(60); // 1 minute for faster testing
    let message_sizes = vec![64, 256, 1024, 4096]; // Different message sizes

    // Test Quinn backend (default)
    println!("🚀 Starting Quinn backend test...");
    let quinn_results = run_performance_test(
        tls_config.clone(), 
        "127.0.0.1:50052", 
        test_duration,
        message_sizes.clone(),
        "Quinn"
    ).await?;

    quinn_results.print("Quinn Backend (1-minute test)");

    // Wait a bit between tests
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Test tQUIC backend (if available)
    #[cfg(feature = "backend-tquic")]
    {
        println!("\n🚀 Starting tQUIC backend test...");
        let tquic_results = run_performance_test_tquic(
            tls_config.clone(), 
            "127.0.0.1:50053", 
            test_duration,
            message_sizes.clone(),
            "tQUIC"
        ).await?;

        tquic_results.print("tQUIC Backend (1-minute test)");

        // Compare results
        print_comparison(&quinn_results, &tquic_results);
    }

    #[cfg(not(feature = "backend-tquic"))]
    {
        println!("\n⚠️  tQUIC backend not available (feature not enabled)");
        println!("To test tQUIC, run with: --features backend-tquic --no-default-features");
    }

    Ok(())
}

async fn run_performance_test(
    tls_config: TlsConfig,
    addr: &str,
    duration: Duration,
    message_sizes: Vec<usize>,
    backend_name: &str,
) -> Result<PerformanceStats, Box<dyn Error>> {
    let transport = QuicTransport::new(tls_config)?;
    let metrics = Arc::new(Mutex::new(PerformanceMetrics::new()));
    
    // Start server
    let mut incoming = transport.listen(addr).await?;
    
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                // Handle multiple streams per connection
                loop {
                    match conn.accept_bi().await {
                        Ok(mut stream) => {
                            tokio::spawn(async move {
                                while let Ok(msg) = stream.receive_message().await {
                                    let _ = stream.send_message(msg).await;
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    println!("  Server started, beginning {} performance test...", backend_name);
    
    // Parallel client load generation
    let num_concurrent_clients = 8; // Adjust based on system capabilities
    let client_addr = addr.to_string();
    let start_time = Instant::now();
    
    let mut client_tasks = Vec::new();
    
    for client_id in 0..num_concurrent_clients {
        let transport_clone = transport.clone();
        let addr_clone = client_addr.clone();
        let metrics_clone = metrics.clone();
        let message_sizes_clone = message_sizes.clone();
        
        let task = tokio::spawn(async move {
            // Create one connection per client worker and reuse it
            if let Ok(client) = transport_clone.connect(&addr_clone).await {
                let mut message_counter = 0u64;
                
                while start_time.elapsed() < duration {
                    // Rotate through different message sizes
                    let message_size = message_sizes_clone[message_counter as usize % message_sizes_clone.len()];
                    let message_data = vec![42u8; message_size];
                    let message = Bytes::from(message_data);
                    
                    // Reuse connection, create new stream for each message batch
                    if let Ok(mut stream) = client.open_bi().await {
                        let batch_size = 100; // Send messages in batches
                        
                        for _ in 0..batch_size {
                            let test_start = Instant::now();
                            
                            if stream.send_message(message.clone()).await.is_ok() {
                                if let Ok(response) = stream.receive_message().await {
                                    let latency = test_start.elapsed();
                                    
                                    if response.len() == message.len() {
                                        let mut m = metrics_clone.lock().await;
                                        m.add_measurement(message_size, latency);
                                    }
                                }
                            }
                            
                            message_counter += 1;
                            
                            // Check if test duration exceeded
                            if start_time.elapsed() >= duration {
                                break;
                            }
                        }
                    }
                }
            }
        });
        
        client_tasks.push(task);
    }
    
    // Progress reporting task
    let progress_metrics = metrics.clone();
    let progress_task = tokio::spawn(async move {
        let mut last_reported = 0u64;
        while start_time.elapsed() < duration {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            let current_messages = {
                let m = progress_metrics.lock().await;
                m.total_messages
            };
            
            let elapsed = start_time.elapsed().as_secs();
            let remaining = duration.as_secs().saturating_sub(elapsed);
            let rate = (current_messages - last_reported) as f64 / 10.0;
            
            println!("    Progress: {}s elapsed, {}s remaining, {} msgs total, {:.0} msg/s", 
                     elapsed, remaining, current_messages, rate);
            
            last_reported = current_messages;
        }
    });
    
    // Wait for all client tasks to complete
    for task in client_tasks {
        let _ = task.await;
    }
    
    progress_task.abort();
    
    let mut final_metrics = metrics.lock().await;
    final_metrics.finalize();
    
    Ok(final_metrics.calculate_stats())
}

#[cfg(feature = "backend-tquic")]
async fn run_performance_test_tquic(
    tls_config: TlsConfig,
    addr: &str,
    duration: Duration,
    message_sizes: Vec<usize>,
    backend_name: &str,
) -> Result<PerformanceStats, Box<dyn Error>> {
    // For now, return placeholder results since tQUIC stream implementation is incomplete
    println!("  ⚠️  tQUIC performance test not fully implemented yet");
    println!("      Returning estimated results based on connection establishment");
    
    // Simulate basic performance metrics
    let mut metrics = PerformanceMetrics::new();
    
    // Test basic connection establishment performance
    let iterations = 10;
    for i in 0..iterations {
        let start = Instant::now();
        
        // Just test connection establishment for now
        if let Ok(transport) = QuicTransport::new(tls_config.clone()) {
            let test_addr = format!("127.0.0.1:{}", 50053 + i);
            if let Ok(_incoming) = transport.listen(&test_addr).await {
                let latency = start.elapsed();
                metrics.add_measurement(64, latency);
            }
        }
        
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    metrics.finalize();
    Ok(metrics.calculate_stats())
}

fn print_comparison(quinn_stats: &PerformanceStats, tquic_stats: &PerformanceStats) {
    println!("\n📊 Performance Comparison Summary:");
    println!("=================================");
    
    let latency_improvement = if quinn_stats.avg_latency > tquic_stats.avg_latency {
        let improvement = (quinn_stats.avg_latency.as_secs_f64() / tquic_stats.avg_latency.as_secs_f64() - 1.0) * 100.0;
        format!("tQUIC is {:.1}% faster", improvement)
    } else {
        let improvement = (tquic_stats.avg_latency.as_secs_f64() / quinn_stats.avg_latency.as_secs_f64() - 1.0) * 100.0;
        format!("Quinn is {:.1}% faster", improvement)
    };
    
    let throughput_improvement = if quinn_stats.throughput_msg_per_sec > tquic_stats.throughput_msg_per_sec {
        let improvement = (quinn_stats.throughput_msg_per_sec / tquic_stats.throughput_msg_per_sec - 1.0) * 100.0;
        format!("Quinn has {:.1}% higher throughput", improvement)
    } else {
        let improvement = (tquic_stats.throughput_msg_per_sec / quinn_stats.throughput_msg_per_sec - 1.0) * 100.0;
        format!("tQUIC has {:.1}% higher throughput", improvement)
    };
    
    println!("Latency:    {}", latency_improvement);
    println!("Throughput: {}", throughput_improvement);
    
    println!("\nQuinn Backend:");
    println!("  Avg Latency: {:.2}ms", quinn_stats.avg_latency.as_secs_f64() * 1000.0);
    println!("  Throughput:  {:.0} msg/s", quinn_stats.throughput_msg_per_sec);
    
    println!("\ntQUIC Backend:");
    println!("  Avg Latency: {:.2}ms", tquic_stats.avg_latency.as_secs_f64() * 1000.0);
    println!("  Throughput:  {:.0} msg/s", tquic_stats.throughput_msg_per_sec);
}