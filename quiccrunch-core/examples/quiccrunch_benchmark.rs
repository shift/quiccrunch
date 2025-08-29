// Simplified QuicCrunch Performance Test
// Tests QuicCrunch performance and saves results for comparison

use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use std::error::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use hdrhistogram::Histogram;

#[path = "cert_utils.rs"]
mod cert_utils;

// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkConfig {
    message_sizes: Vec<usize>,
    num_requests: usize,
    concurrent_clients: usize,
    warmup_requests: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            message_sizes: vec![64, 1024, 8192, 32768], // 64B, 1KB, 8KB, 32KB
            num_requests: 1000, // Reduced for faster testing
            concurrent_clients: 4, // Reduced for stability
            warmup_requests: 100, // Reduced
        }
    }
}

// Benchmark results
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkResults {
    protocol: String,
    message_size: usize,
    total_requests: usize,
    duration: Duration,
    requests_per_second: f64,
    latency_p50: f64,
    latency_p95: f64,
    latency_p99: f64,
    latency_max: f64,
    throughput_mbps: f64,
    errors: usize,
}

// Shared benchmark message
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkMessage {
    id: String,
    timestamp: u64,
    payload: Vec<u8>,
    sequence: u32,
}

impl BenchmarkMessage {
    fn new(size: usize, sequence: u32) -> Self {
        Self {
            id: format!("msg-{}", sequence),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: vec![0u8; size],
            sequence,
        }
    }
}

// QuicCrunch benchmark implementation
struct QuicCrunchBenchmark;

impl QuicCrunchBenchmark {
    async fn run_latency_test(
        config: &BenchmarkConfig,
        message_size: usize,
    ) -> Result<BenchmarkResults, Box<dyn Error + Send + Sync>> {
        // Generate TLS certificates
        let (ca_cert, _, _) = cert_utils::generate_ca_cert()?;
        let (cert, key) = cert_utils::generate_node_cert(&ca_cert, "localhost")?;
        
        let tls_config = TlsConfig { cert, key, ca_cert };
        let transport = QuicTransport::new(tls_config)?;
        
        // Start server
        let server_transport = transport.clone();
        let server_handle = tokio::spawn(async move {
            let mut incoming = server_transport.listen("127.0.0.1:50051").await?;
            
            while let Ok(conn) = incoming.accept().await {
                tokio::spawn(async move {
                    loop {
                        match conn.accept_bi().await {
                            Ok(mut stream) => {
                                loop {
                                    match stream.receive_message().await {
                                        Ok(msg) => {
                                            // Echo back immediately
                                            if stream.send_message(msg).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok::<_, Box<dyn Error + Send + Sync>>(())
                });
            }
            Ok::<_, Box<dyn Error + Send + Sync>>(())
        });

        // Wait for server to start
        sleep(Duration::from_millis(100)).await;

        // Warmup
        Self::run_warmup(&transport, config.warmup_requests, message_size).await?;

        // Run benchmark
        let start_time = Instant::now();
        let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?; // 1us to 60s
        let mut total_requests = 0;
        let mut errors = 0;

        // Create concurrent clients
        let mut handles = Vec::new();
        let requests_per_client = config.num_requests / config.concurrent_clients;

        for _ in 0..config.concurrent_clients {
            let transport = transport.clone();
            let handle = tokio::spawn(async move {
                let conn = transport.connect("127.0.0.1:50051").await?;
                let mut stream = conn.open_bi().await?;
                let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
                let mut local_errors = 0;

                for i in 0..requests_per_client {
                    let msg = BenchmarkMessage::new(message_size, i as u32);
                    let msg_bytes = serde_json::to_vec(&msg)?;
                    
                    let request_start = Instant::now();
                    
                    match stream.send_message(Bytes::from(msg_bytes)).await {
                        Ok(_) => {
                            match stream.receive_message().await {
                                Ok(_) => {
                                    let latency = request_start.elapsed().as_micros() as u64;
                                    let _ = local_hist.record(latency);
                                }
                                Err(_) => local_errors += 1,
                            }
                        }
                        Err(_) => local_errors += 1,
                    }
                }

                Ok::<_, Box<dyn Error + Send + Sync>>((local_hist, requests_per_client, local_errors))
            });
            handles.push(handle);
        }

        // Collect results
        for handle in handles {
            let (local_hist, local_requests, local_errors) = handle.await.map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)??;
            histogram.add(&local_hist)?;
            total_requests += local_requests;
            errors += local_errors;
        }

        let duration = start_time.elapsed();
        
        // Stop server
        server_handle.abort();

        Ok(BenchmarkResults {
            protocol: "QuicCrunch".to_string(),
            message_size,
            total_requests,
            duration,
            requests_per_second: total_requests as f64 / duration.as_secs_f64(),
            latency_p50: histogram.value_at_quantile(0.5) as f64 / 1000.0, // Convert to milliseconds
            latency_p95: histogram.value_at_quantile(0.95) as f64 / 1000.0,
            latency_p99: histogram.value_at_quantile(0.99) as f64 / 1000.0,
            latency_max: histogram.max() as f64 / 1000.0,
            throughput_mbps: (total_requests * message_size) as f64 / duration.as_secs_f64() / 1_048_576.0,
            errors,
        })
    }

    async fn run_warmup(
        transport: &QuicTransport,
        warmup_requests: usize,
        message_size: usize,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = transport.connect("127.0.0.1:50051").await?;
        let mut stream = conn.open_bi().await?;

        for i in 0..warmup_requests {
            let msg = BenchmarkMessage::new(message_size, i as u32);
            let msg_bytes = serde_json::to_vec(&msg)?;
            
            stream.send_message(Bytes::from(msg_bytes)).await?;
            let _ = stream.receive_message().await?;
        }

        Ok(())
    }
}

fn print_results_table(results: &[BenchmarkResults]) {
    println!("📈 QUICCRUNCH PERFORMANCE RESULTS");
    println!("{}", "=".repeat(100));
    
    println!("{:<12} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<15}", 
        "Protocol", "Msg Size", "RPS", "P50 (ms)", "P95 (ms)", "P99 (ms)", "Max (ms)", "Mbps", "Errors");
    println!("{}", "-".repeat(100));
    
    for result in results {
        println!("{:<12} {:<10} {:<10.0} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<15}",
            result.protocol, 
            format!("{}B", result.message_size),
            result.requests_per_second,
            result.latency_p50,
            result.latency_p95,
            result.latency_p99,
            result.latency_max,
            result.throughput_mbps,
            result.errors);
    }
    println!();
}

fn generate_summary_report(results: &[BenchmarkResults]) -> String {
    let mut summary = String::new();
    
    summary.push_str("# QuicCrunch Performance Report\n\n");
    summary.push_str(&format!("**Generated:** {}\n", chrono::Utc::now().to_rfc3339()));
    summary.push_str("**Protocol:** QuicCrunch only (gRPC comparison not enabled)\n\n");
    
    summary.push_str("## Results Summary\n\n");
    
    for result in results {
        summary.push_str(&format!("### {} byte messages\n", result.message_size));
        summary.push_str(&format!("- Requests per second: {:.0}\n", result.requests_per_second));
        summary.push_str(&format!("- P99 latency: {:.2} ms\n", result.latency_p99));
        summary.push_str(&format!("- Throughput: {:.2} Mbps\n", result.throughput_mbps));
        summary.push_str(&format!("- Errors: {}\n\n", result.errors));
    }
    
    summary.push_str("## Notes\n\n");
    summary.push_str("- To enable gRPC comparison, build with `--features grpc-comparison`\n");
    summary.push_str("- These results show QuicCrunch performance in isolation\n");
    summary.push_str("- For production comparison, run head-to-head tests with gRPC\n\n");
    
    summary
}

async fn run_quiccrunch_benchmark() -> Result<Vec<BenchmarkResults>, Box<dyn Error + Send + Sync>> {
    let config = BenchmarkConfig::default();
    println!("🚀 Running QuicCrunch Performance Benchmark");
    println!("Configuration: {:?}", config);
    println!();

    let mut results = Vec::new();

    for &message_size in &config.message_sizes {
        println!("📊 Testing message size: {} bytes", message_size);
        
        print!("  Running QuicCrunch benchmark... ");
        let result = QuicCrunchBenchmark::run_latency_test(&config, message_size).await?;
        println!("✅ Done");
        
        results.push(result);
        println!();
    }

    Ok(results)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("🔥 QuicCrunch Performance Test");
    println!("⚠️  Note: gRPC comparison not enabled in this build");
    println!();
    
    let results = run_quiccrunch_benchmark().await?;
    
    print_results_table(&results);
    
    // Save results
    let json_report = serde_json::to_string_pretty(&results)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    
    std::fs::write(format!("quiccrunch_results_{}.json", timestamp), &json_report)?;
    println!("📁 Results saved to: quiccrunch_results_{}.json", timestamp);
    
    let summary = generate_summary_report(&results);
    std::fs::write(format!("quiccrunch_summary_{}.md", timestamp), &summary)?;
    println!("📄 Summary report saved to: quiccrunch_summary_{}.md", timestamp);
    
    // Calculate average performance
    let avg_rps: f64 = results.iter().map(|r| r.requests_per_second).sum::<f64>() / results.len() as f64;
    let avg_latency: f64 = results.iter().map(|r| r.latency_p99).sum::<f64>() / results.len() as f64;
    
    println!();
    println!("🎯 PERFORMANCE SUMMARY:");
    println!("   Average RPS: {:.0}", avg_rps);
    println!("   Average P99 latency: {:.2} ms", avg_latency);
    println!("   Total errors: {}", results.iter().map(|r| r.errors).sum::<usize>());
    
    println!();
    println!("💡 To enable gRPC comparison:");
    println!("   cargo run --example grpc_comparison --features grpc-comparison --release");
    
    Ok(())
}