// Comprehensive gRPC vs QuicCrunch Performance Comparison
// This example provides honest, standardized benchmarks between gRPC and QuicCrunch

use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use quiccrunch_core::transport::TransportSettings;
use std::error::Error as StdError;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use hdrhistogram::Histogram;

#[cfg(feature = "grpc-comparison")]
use tonic::transport::Channel;

#[path = "cert_utils.rs"]
mod cert_utils;

// Test configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkConfig {
    message_sizes: Vec<usize>,
    num_requests: usize,
    concurrent_clients: usize,
    warmup_requests: usize,
    test_duration_secs: u64,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            // Comprehensive size testing: small packets, typical payloads, large transfers
            message_sizes: vec![
                64,      // Small: minimal packet
                256,     // Small: cache-friendly
                512,     // Small: typical small message
                1024,    // Medium: 1KB typical
                4096,    // Medium: 4KB page size
                16384,   // Medium: 16KB typical packet
                65536,   // Large: 64KB bulk
                262144,  // Large: 256KB
                1048576, // Large: 1MB
                2097152, // Large: 2MB
                4194304, // Large: 4MB
                8388608, // Large: 8MB (within our 10MB window)
            ],
            num_requests: 500, // Good balance of speed and statistical significance
            concurrent_clients: 4, // Multi-core testing
            warmup_requests: 25, // Adequate warmup
            test_duration_secs: 10, // Reasonable duration
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComparisonReport {
    timestamp: String,
    config: BenchmarkConfig,
    quiccrunch_results: Vec<BenchmarkResults>,
    grpc_results: Vec<BenchmarkResults>,
    improvements: Vec<ProtocolComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProtocolComparison {
    message_size: usize,
    latency_improvement: f64, // Positive means QuicCrunch is faster
    throughput_improvement: f64,
    requests_per_second_improvement: f64,
    quiccrunch_wins: bool,
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
        // Create payload that accounts for JSON overhead
        // Approximate: "{"id":"msg-000000","timestamp":1234567890123456,"payload":[...],"sequence":4294967295}"
        // Base overhead is about 80 bytes, then payload gets expanded as JSON array
        let overhead = 80;
        let target_payload_size = if size > overhead {
            // For JSON array, each byte becomes ~4 chars: "123," so divide by 4
            (size - overhead) / 4
        } else {
            1 // Minimum payload
        };
        
        Self {
            id: format!("msg-{:06}", sequence),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: vec![42u8; target_payload_size.max(1)], // Use 42 instead of 0 for variety
            sequence,
        }
    }
    
    // Create a simple binary message for testing without JSON overhead
    fn new_binary(size: usize, sequence: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity(size);
        // Add sequence number as first 4 bytes
        data.extend_from_slice(&sequence.to_le_bytes());
        // Fill rest with test pattern
        while data.len() < size {
            data.push((data.len() % 256) as u8);
        }
        data.truncate(size);
        data
    }
}

// QuicCrunch benchmark implementation
struct QuicCrunchBenchmark;

impl QuicCrunchBenchmark {
    async fn run_latency_test(
        config: &BenchmarkConfig,
        message_size: usize,
    ) -> Result<BenchmarkResults, Box<dyn StdError + Send + Sync>> {
        // Generate TLS certificates
        let (ca_cert_der, ca_key, ca_cert_rcgen) = cert_utils::generate_ca_cert().map_err(|e| format!("CA cert error: {}", e))?;
        let (cert, key) = cert_utils::generate_node_cert(&ca_cert_rcgen, "localhost").map_err(|e| format!("Node cert error: {}", e))?;
        
        let tls_config = TlsConfig { 
            cert, 
            key, 
            ca_cert: ca_cert_der,
            cert_pem: vec![], // Empty for now - tQUIC backend would need actual PEM data
            key_pem: vec![],  // Empty for now - tQUIC backend would need actual PEM data  
            ca_cert_pem: vec![], // Empty for now - tQUIC backend would need actual PEM data
        };
        
        // Use larger window settings to allow larger messages
        let settings = TransportSettings {
            stream_receive_window: 10 * 1024 * 1024, // 10MB to allow all our test sizes
            connection_receive_window: 20 * 1024 * 1024, // 20MB
            ..Default::default()
        };
        
        let transport = QuicTransport::with_settings(tls_config, settings)?;
        
        // Start server with proper timeout and shutdown handling
        let server_transport = transport.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        let server_handle = tokio::spawn(async move {
            let mut incoming = server_transport.listen("127.0.0.1:50051").await?;
            
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        break;
                    }
                    conn_result = incoming.accept() => {
                        match conn_result {
                            Ok(conn) => {
                                tokio::spawn(async move {
                                    // Handle up to 1000 streams per connection with timeout
                                    for _ in 0..1000 {
                                        let stream_result = tokio::time::timeout(
                                            Duration::from_secs(1),
                                            conn.accept_bi()
                                        ).await;
                                        
                                        match stream_result {
                                            Ok(Ok(mut stream)) => {
                                                tokio::spawn(async move {
                                                    // Handle up to 1000 messages per stream with timeout
                                                    for _ in 0..1000 {
                                                        let msg_result = tokio::time::timeout(
                                                            Duration::from_secs(1),
                                                            stream.receive_message()
                                                        ).await;
                                                        
                                                        match msg_result {
                                                            Ok(Ok(msg)) => {
                                                                println!("Server received message of {} bytes", msg.len());
                                                                // Simple echo - just send the same data back
                                                                if let Err(e) = stream.send_message(msg).await {
                                                                    println!("Server echo failed: {}", e);
                                                                    break;
                                                                } else {
                                                                    println!("Server echoed message successfully");
                                                                }
                                                            }
                                                            Ok(Err(e)) => {
                                                                println!("Server receive error: {}", e);
                                                                break;
                                                            }
                                                            Err(_) => {
                                                                println!("Server receive timeout");
                                                                break;
                                                            }
                                                        }
                                                    }
                                                });
                                            }
                                            _ => break,
                                        }
                                    }
                                });
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            Ok::<_, Box<dyn StdError + Send + Sync>>(())
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

                for _i in 0..requests_per_client {
                    // Use simple byte payload 
                    let payload = vec![42u8; message_size]; // Simple repeating byte pattern
                    
                    let request_start = Instant::now();
                    
                    match stream.send_message(Bytes::from(payload)).await {
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

                Ok::<_, Box<dyn StdError + Send + Sync>>((local_hist, requests_per_client, local_errors))
            });
            handles.push(handle);
        }

        // Collect results
        for handle in handles {
            let (local_hist, local_requests, local_errors) = handle.await??;
            histogram.add(&local_hist)?;
            total_requests += local_requests;
            errors += local_errors;
        }

        let duration = start_time.elapsed();
        
        // Stop server gracefully
        let _ = shutdown_tx.send(());
        // Give server time to shutdown gracefully, then force abort if needed
        if tokio::time::timeout(Duration::from_secs(2), server_handle).await.is_err() {
            // Force abort if graceful shutdown times out
        }

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
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let conn = transport.connect("127.0.0.1:50051").await?;
        let mut stream = conn.open_bi().await?;

        for i in 0..warmup_requests {
            // Use simple binary payload like in the main benchmark
            let payload = vec![42u8; message_size];
            
            stream.send_message(Bytes::from(payload)).await?;
            let _ = stream.receive_message().await?;
        }

        Ok(())
    }
}

// gRPC benchmark implementation
#[cfg(feature = "grpc-comparison")]
mod grpc_benchmark {
    use super::*;
    use std::error::Error;
    use tonic::transport::{Server, Channel};
    use tonic::{Request, Response, Status, Streaming};
    use std::net::SocketAddr;

    // Include generated protobuf code
    pub mod proto {
        tonic::include_proto!("benchmark");
    }

    use proto::{
        benchmark_service_server::{BenchmarkService, BenchmarkServiceServer},
        benchmark_service_client::BenchmarkServiceClient,
        BenchmarkRequest, BenchmarkResponse,
    };

    pub struct GrpcBenchmarkService;

    #[tonic::async_trait]
    impl BenchmarkService for GrpcBenchmarkService {
        async fn latency_test(
            &self,
            request: Request<BenchmarkRequest>,
        ) -> Result<Response<BenchmarkResponse>, Status> {
            let req = request.into_inner();
            
            let response = BenchmarkResponse {
                id: req.id,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: req.payload, // Echo back the payload
                status: "ok".to_string(),
                metrics: None,
            };

            Ok(Response::new(response))
        }

        async fn flood_test(
            &self,
            _request: Request<Streaming<BenchmarkRequest>>,
        ) -> Result<Response<BenchmarkResponse>, Status> {
            unimplemented!("Flood test not implemented for this benchmark")
        }

        async fn stream_test(
            &self,
            _request: Request<Streaming<BenchmarkRequest>>,
        ) -> Result<Response<Self::StreamTestStream>, Status> {
            unimplemented!("Stream test not implemented for this benchmark")
        }

        type StreamTestStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<BenchmarkResponse, Status>> + Send>>;
    }

    pub struct GrpcBenchmark;

    impl GrpcBenchmark {
        pub async fn run_latency_test(
            config: &BenchmarkConfig,
            message_size: usize,
        ) -> Result<BenchmarkResults, Box<dyn Error + Send + Sync>> {
            let addr: SocketAddr = "127.0.0.1:50052".parse()?;
            
            // Start gRPC server
            let service = GrpcBenchmarkService;
            let server_handle = tokio::spawn(async move {
                Server::builder()
                    .add_service(BenchmarkServiceServer::new(service))
                    .serve(addr)
                    .await
            });

            // Wait for server to start
            sleep(Duration::from_millis(200)).await;

            // Connect client
            let channel = Channel::from_static("http://127.0.0.1:50052")
                .connect()
                .await?;
            let mut client = BenchmarkServiceClient::new(channel);

            // Warmup
            Self::run_warmup(&mut client, config.warmup_requests, message_size).await?;

            // Run benchmark
            let start_time = Instant::now();
            let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
            let mut total_requests = 0;
            let mut errors = 0;

            // Create concurrent clients
            let mut handles = Vec::new();
            let requests_per_client = config.num_requests / config.concurrent_clients;

            for _ in 0..config.concurrent_clients {
                let handle = tokio::spawn(async move {
                    let channel = Channel::from_static("http://127.0.0.1:50052")
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
                    let mut local_errors = 0;

                    for i in 0..requests_per_client {
                        let request = BenchmarkRequest {
                            id: format!("grpc-{}", i),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros() as u64,
                            payload: vec![0u8; message_size],
                            test_type: "latency".to_string(),
                            sequence_number: i as u32,
                        };

                        let request_start = Instant::now();
                        
                        match client.latency_test(Request::new(request)).await {
                            Ok(_) => {
                                let latency = request_start.elapsed().as_micros() as u64;
                                let _ = local_hist.record(latency);
                            }
                            Err(_) => local_errors += 1,
                        }
                    }

                Ok::<_, Box<dyn StdError + Send + Sync>>((local_hist, requests_per_client, local_errors))
                });
                handles.push(handle);
            }

            // Collect results
            for handle in handles {
            let (local_hist, local_requests, local_errors) = handle.await??;
                histogram.add(&local_hist)?;
                total_requests += local_requests;
                errors += local_errors;
            }

            let duration = start_time.elapsed();
            
            // Stop server
            server_handle.abort();

            Ok(BenchmarkResults {
                protocol: "gRPC".to_string(),
                message_size,
                total_requests,
                duration,
                requests_per_second: total_requests as f64 / duration.as_secs_f64(),
                latency_p50: histogram.value_at_quantile(0.5) as f64 / 1000.0,
                latency_p95: histogram.value_at_quantile(0.95) as f64 / 1000.0,
                latency_p99: histogram.value_at_quantile(0.99) as f64 / 1000.0,
                latency_max: histogram.max() as f64 / 1000.0,
                throughput_mbps: (total_requests * message_size) as f64 / duration.as_secs_f64() / 1_048_576.0,
                errors,
            })
        }

        async fn run_warmup(
            client: &mut BenchmarkServiceClient<Channel>,
            warmup_requests: usize,
            message_size: usize,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
        for i in 0..warmup_requests {
                let request = BenchmarkRequest {
                    id: format!("warmup-{}", i),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                    payload: vec![0u8; message_size],
                    test_type: "warmup".to_string(),
                    sequence_number: i as u32,
                };

                let _ = client.latency_test(Request::new(request)).await?;
            }

            Ok(())
        }
    }
}

async fn run_comprehensive_comparison() -> Result<ComparisonReport, Box<dyn StdError + Send + Sync>> {
    let config = BenchmarkConfig::default();
    println!("🚀 Starting comprehensive gRPC vs QuicCrunch benchmark");
    println!("Configuration: {:?}", config);
    println!();

    let mut quiccrunch_results = Vec::new();
    let mut grpc_results = Vec::new();
    let mut improvements = Vec::new();

    for &message_size in &config.message_sizes {
        println!("📊 Testing message size: {} bytes", message_size);
        
        // Test QuicCrunch
        print!("  Running QuicCrunch benchmark... ");
        let quic_result = QuicCrunchBenchmark::run_latency_test(&config, message_size).await?;
        println!("✅ Done");
        
        // Test gRPC
        #[cfg(feature = "grpc-comparison")]
        {
            print!("  Running gRPC benchmark... ");
            let grpc_result = grpc_benchmark::GrpcBenchmark::run_latency_test(&config, message_size).await?;
            println!("✅ Done");
            
            // Calculate improvements
            let latency_improvement = (grpc_result.latency_p99 - quic_result.latency_p99) / grpc_result.latency_p99 * 100.0;
            let throughput_improvement = (quic_result.throughput_mbps - grpc_result.throughput_mbps) / grpc_result.throughput_mbps * 100.0;
            let rps_improvement = (quic_result.requests_per_second - grpc_result.requests_per_second) / grpc_result.requests_per_second * 100.0;
            
            let comparison = ProtocolComparison {
                message_size,
                latency_improvement,
                throughput_improvement,
                requests_per_second_improvement: rps_improvement,
                quiccrunch_wins: quic_result.latency_p99 < grpc_result.latency_p99,
            };
            
            improvements.push(comparison);
            grpc_results.push(grpc_result);
        }
        
        #[cfg(not(feature = "grpc-comparison"))]
        {
            println!("  ⚠️  gRPC comparison skipped (feature not enabled)");
            let dummy_grpc = BenchmarkResults {
                protocol: "gRPC (not tested)".to_string(),
                message_size,
                total_requests: 0,
                duration: Duration::from_secs(0),
                requests_per_second: 0.0,
                latency_p50: 0.0,
                latency_p95: 0.0,
                latency_p99: 0.0,
                latency_max: 0.0,
                throughput_mbps: 0.0,
                errors: 0,
            };
            grpc_results.push(dummy_grpc);
        }
        
        quiccrunch_results.push(quic_result);
        println!();
    }

    let report = ComparisonReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        config,
        quiccrunch_results,
        grpc_results,
        improvements,
    };

    Ok(report)
}

fn print_results_table(report: &ComparisonReport) {
    println!("📈 PERFORMANCE COMPARISON RESULTS");
    println!("{}", "=".repeat(120));
    
    println!("{:<12} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<15}", 
        "Protocol", "Msg Size", "RPS", "P50 (ms)", "P95 (ms)", "P99 (ms)", "Max (ms)", "Mbps", "Errors");
    println!("{}", "-".repeat(120));
    
    for (quic, grpc) in report.quiccrunch_results.iter().zip(report.grpc_results.iter()) {
        println!("{:<12} {:<10} {:<10.0} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<15}",
            quic.protocol, 
            format!("{}B", quic.message_size),
            quic.requests_per_second,
            quic.latency_p50,
            quic.latency_p95,
            quic.latency_p99,
            quic.latency_max,
            quic.throughput_mbps,
            quic.errors);
            
        if grpc.total_requests > 0 {
            println!("{:<12} {:<10} {:<10.0} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<10.2} {:<15}",
                grpc.protocol,
                format!("{}B", grpc.message_size),
                grpc.requests_per_second,
                grpc.latency_p50,
                grpc.latency_p95,
                grpc.latency_p99,
                grpc.latency_max,
                grpc.throughput_mbps,
                grpc.errors);
        }
        println!();
    }
    
    if !report.improvements.is_empty() {
        println!("🏆 IMPROVEMENT SUMMARY");
        println!("{}", "=".repeat(80));
        println!("{:<12} {:<15} {:<15} {:<15} {:<10}",
            "Msg Size", "Latency Impr.", "Throughput Impr.", "RPS Impr.", "Winner");
        println!("{}", "-".repeat(80));
        
        for improvement in &report.improvements {
            println!("{:<12} {:<15.1}% {:<15.1}% {:<15.1}% {:<10}",
                format!("{}B", improvement.message_size),
                improvement.latency_improvement,
                improvement.throughput_improvement,
                improvement.requests_per_second_improvement,
                if improvement.quiccrunch_wins { "QuicCrunch" } else { "gRPC" });
        }
    }
}

fn generate_summary_report(report: &ComparisonReport) -> String {
    let mut summary = String::new();
    
    summary.push_str("# QuicCrunch vs gRPC Performance Report\n\n");
    summary.push_str(&format!("**Generated:** {}\n", report.timestamp));
    summary.push_str(&format!("**Test Configuration:** {} requests, {} concurrent clients\n\n", 
        report.config.num_requests, report.config.concurrent_clients));
    
    if !report.improvements.is_empty() {
        let wins = report.improvements.iter().filter(|i| i.quiccrunch_wins).count();
        let total = report.improvements.len();
        
        summary.push_str(&format!("## Overall Results\n"));
        summary.push_str(&format!("- **QuicCrunch wins:** {}/{} test cases\n", wins, total));
        
        if wins > total / 2 {
            summary.push_str("- **Verdict:** ✅ QuicCrunch outperforms gRPC in most scenarios\n\n");
        } else {
            summary.push_str("- **Verdict:** ❌ gRPC outperforms QuicCrunch in most scenarios\n\n");
        }
        
        summary.push_str("## Detailed Results\n\n");
        for improvement in &report.improvements {
            summary.push_str(&format!("### {} byte messages\n", improvement.message_size));
            summary.push_str(&format!("- Latency improvement: {:.1}%\n", improvement.latency_improvement));
            summary.push_str(&format!("- Throughput improvement: {:.1}%\n", improvement.throughput_improvement));
            summary.push_str(&format!("- RPS improvement: {:.1}%\n", improvement.requests_per_second_improvement));
            summary.push_str(&format!("- Winner: {}\n\n", if improvement.quiccrunch_wins { "QuicCrunch" } else { "gRPC" }));
        }
    } else {
        summary.push_str("## Results\n");
        summary.push_str("⚠️ gRPC comparison was not enabled. Build with `--features grpc-comparison` to enable.\n\n");
    }
    
    summary
}

fn main() -> Result<(), Box<dyn StdError + Send + Sync>> {
    // Create a multi-threaded Tokio runtime
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .enable_all()
        .build()?;
    
    runtime.block_on(async {
        run_benchmark().await
    })
}

async fn run_benchmark() -> Result<(), Box<dyn StdError + Send + Sync>> {
    println!("🔥 QuicCrunch vs gRPC Honest Performance Comparison");
    println!("🖥️  Using {} CPU cores for multi-threaded execution", num_cpus::get());
    println!();
    
    #[cfg(not(feature = "grpc-comparison"))]
    {
        println!("⚠️  WARNING: gRPC comparison feature is not enabled!");
        println!("   Run with: cargo run --example grpc_comparison --features grpc-comparison");
        println!("   This will only test QuicCrunch performance.");
        println!();
    }
    
    let report = run_comprehensive_comparison().await?;
    
    print_results_table(&report);
    
    // Save results
    let json_report = serde_json::to_string_pretty(&report)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    
    std::fs::write(format!("grpc_comparison_results_{}.json", timestamp), &json_report)?;
    println!("📁 Detailed results saved to: grpc_comparison_results_{}.json", timestamp);
    
    let summary = generate_summary_report(&report);
    std::fs::write(format!("grpc_comparison_summary_{}.md", timestamp), &summary)?;
    println!("📄 Summary report saved to: grpc_comparison_summary_{}.md", timestamp);
    
    #[cfg(feature = "grpc-comparison")]
    {
        let wins = report.improvements.iter().filter(|i| i.quiccrunch_wins).count();
        let total = report.improvements.len();
        
        println!();
        if wins > total / 2 {
            println!("🎉 RESULT: QuicCrunch outperforms gRPC in {}/{} test cases!", wins, total);
        } else {
            println!("📊 RESULT: gRPC outperforms QuicCrunch in {}/{} test cases.", total - wins, total);
        }
    }
    
    Ok(())
}