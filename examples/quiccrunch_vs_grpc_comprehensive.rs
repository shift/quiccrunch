// QuicCrunch vs gRPC Comprehensive Performance Comparison
// This benchmark demonstrates QuicCrunch's performance advantages including connection pooling

use quiccrunch_core::{QuicTransport, Transport};
use quiccrunch_core::transport::tquic::TlsConfig;
use quiccrunch_core::connection_pool::{ConnectionPool, ConnectionPoolConfig};
use std::error::Error as StdError;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use bytes::Bytes;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use hdrhistogram::Histogram;
use std::sync::Arc;

#[cfg(feature = "grpc-comparison")]
#[cfg(feature = "grpc-comparison")]
use tonic::transport::{Server, Channel};
#[cfg(feature = "grpc-comparison")]
use tonic::{Request, Response, Status};

#[path = "../quiccrunch-core/examples/cert_utils.rs"]
mod cert_utils;

// Configuration for the comprehensive benchmark
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComprehensiveBenchmarkConfig {
    message_sizes: Vec<usize>,
    num_requests_per_size: usize,
    concurrent_clients: usize,
    warmup_requests: usize,
    test_connection_reuse: bool,  // Test connection pooling benefits
    pool_size: usize,
}

impl Default for ComprehensiveBenchmarkConfig {
    fn default() -> Self {
        Self {
            message_sizes: vec![
                64,      // Tiny: minimal overhead test
                1024,    // Small: typical API call
                16384,   // Medium: moderate data transfer
                65536,   // Large: substantial payload
                262144,  // Very Large: bulk data
                1048576, // Huge: 1MB file transfer simulation
            ],
            num_requests_per_size: 200,
            concurrent_clients: 8, // More realistic concurrent load
            warmup_requests: 50,
            test_connection_reuse: true,
            pool_size: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DetailedBenchmarkResults {
    protocol: String,
    test_type: String, // "fresh_connections" or "pooled_connections"
    message_size: usize,
    total_requests: usize,
    duration: Duration,
    requests_per_second: f64,
    latency_p50_ms: f64,
    latency_p95_ms: f64,
    latency_p99_ms: f64,
    latency_max_ms: f64,
    throughput_mbps: f64,
    connection_pool_hit_rate: Option<f64>, // Only for QuicCrunch
    errors: usize,
    cpu_efficiency: f64, // Requests per CPU second
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ComprehensiveReport {
    timestamp: String,
    system_info: SystemInfo,
    config: ComprehensiveBenchmarkConfig,
    quiccrunch_fresh_results: Vec<DetailedBenchmarkResults>,
    quiccrunch_pooled_results: Vec<DetailedBenchmarkResults>,
    grpc_results: Vec<DetailedBenchmarkResults>,
    performance_advantages: Vec<PerformanceComparison>,
    executive_summary: ExecutiveSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemInfo {
    cpu_cores: usize,
    os: String,
    architecture: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PerformanceComparison {
    message_size: usize,
    quiccrunch_vs_grpc_latency_improvement: f64,
    quiccrunch_vs_grpc_throughput_improvement: f64,
    pooling_vs_fresh_latency_improvement: f64,
    pooling_vs_fresh_throughput_improvement: f64,
    connection_reuse_advantage: f64,
    quiccrunch_wins_latency: bool,
    quiccrunch_wins_throughput: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutiveSummary {
    quiccrunch_wins_count: usize,
    total_test_cases: usize,
    average_latency_improvement: f64,
    average_throughput_improvement: f64,
    connection_pooling_benefit: f64,
    verdict: String,
}

// gRPC benchmark implementation
#[cfg(feature = "grpc-comparison")]
mod grpc_benchmark {
    use super::*;
use tonic::transport::Channel;
use tonic::Request;
    use std::net::SocketAddr;

    // Include generated protobuf code
    pub mod proto {
        tonic::include_proto!("benchmark");
    }

    use proto::{
        benchmark_service_server::{BenchmarkService, BenchmarkServiceServer},
        benchmark_service_client::BenchmarkServiceClient,
        SimpleRequest, SimpleResponse,
    };

    pub struct GrpcBenchmarkService;

    #[tonic::async_trait]
    impl BenchmarkService for GrpcBenchmarkService {
        async fn unary_call(
            &self,
            request: Request<SimpleRequest>,
        ) -> Result<Response<SimpleResponse>, Status> {
            let req = request.into_inner();
            
            let response = SimpleResponse {
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

        async fn client_streaming(
            &self,
            _request: Request<tonic::Streaming<SimpleRequest>>,
        ) -> Result<Response<SimpleResponse>, Status> {
            Err(Status::unimplemented("Not implemented for this benchmark"))
        }

        async fn server_streaming(
            &self,
            _request: Request<SimpleRequest>,
        ) -> Result<Response<Self::ServerStreamingStream>, Status> {
            Err(Status::unimplemented("Not implemented for this benchmark"))
        }

        async fn bidirectional_streaming(
            &self,
            _request: Request<tonic::Streaming<SimpleRequest>>,
        ) -> Result<Response<Self::BidirectionalStreamingStream>, Status> {
            Err(Status::unimplemented("Not implemented for this benchmark"))
        }

        type ServerStreamingStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<SimpleResponse, Status>> + Send>>;
        type BidirectionalStreamingStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<SimpleResponse, Status>> + Send>>;

        // Legacy methods - implement stubs
        async fn latency_test(
            &self,
            _request: Request<proto::BenchmarkRequest>,
        ) -> Result<Response<proto::BenchmarkResponse>, Status> {
            Err(Status::unimplemented("Use unary_call instead"))
        }

        async fn flood_test(
            &self,
            _request: Request<tonic::Streaming<proto::BenchmarkRequest>>,
        ) -> Result<Response<proto::BenchmarkResponse>, Status> {
            Err(Status::unimplemented("Use client_streaming instead"))
        }

        async fn stream_test(
            &self,
            _request: Request<tonic::Streaming<proto::BenchmarkRequest>>,
        ) -> Result<Response<Self::StreamTestStream>, Status> {
            Err(Status::unimplemented("Use bidirectional_streaming instead"))
        }

        type StreamTestStream = std::pin::Pin<Box<dyn futures::Stream<Item = Result<proto::BenchmarkResponse, Status>> + Send>>;
    }

    pub struct GrpcBenchmark;

    impl GrpcBenchmark {
        pub async fn run_benchmark(
            config: &ComprehensiveBenchmarkConfig,
            message_size: usize,
        ) -> Result<DetailedBenchmarkResults, Box<dyn StdError + Send + Sync>> {
            let addr: SocketAddr = "127.0.0.1:50052".parse()?;
            
            // Start gRPC server
            let service = GrpcBenchmarkService;
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
            let server_handle = tokio::spawn(async move {
                Server::builder()
                    .add_service(BenchmarkServiceServer::new(service))
                    .serve_with_shutdown(addr, async {
                        shutdown_rx.await.ok();
                    })
                    .await
            });

            // Wait for server to start
            sleep(Duration::from_millis(200)).await;

            // Run warmup
            Self::run_warmup(config.warmup_requests, message_size).await?;

            // Run benchmark
            let start_time = Instant::now();
            let cpu_start = Self::get_cpu_time();
            let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
            let mut total_requests = 0;
            let mut errors = 0;

            // Create concurrent clients with fresh connections (simulating gRPC's typical usage)
            let mut handles = Vec::new();
            let requests_per_client = config.num_requests_per_size / config.concurrent_clients;

            for _ in 0..config.concurrent_clients {
                let handle = tokio::spawn(async move {
                    // Each client creates its own connection (no pooling in this gRPC test)
                    let channel = Channel::from_static("http://127.0.0.1:50052")
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
                    let mut local_errors = 0;

                    for i in 0..requests_per_client {
                        let request = SimpleRequest {
                            id: format!("grpc-{}", i),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros() as u64,
                            payload: vec![42u8; message_size],
                            test_type: "unary".to_string(),
                            sequence_number: i as u32,
                        };

                        let request_start = Instant::now();
                        
                        match client.unary_call(Request::new(request)).await {
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
            let cpu_end = Self::get_cpu_time();
            let cpu_efficiency = total_requests as f64 / (cpu_end - cpu_start).max(0.001);
            
            // Stop server gracefully
            let _ = shutdown_tx.send(());
            if tokio::time::timeout(Duration::from_secs(2), server_handle).await.is_err() {
                // Force abort if graceful shutdown times out
            }

            Ok(DetailedBenchmarkResults {
                protocol: "gRPC".to_string(),
                test_type: "fresh_connections".to_string(),
                message_size,
                total_requests,
                duration,
                requests_per_second: total_requests as f64 / duration.as_secs_f64(),
                latency_p50_ms: histogram.value_at_quantile(0.5) as f64 / 1000.0,
                latency_p95_ms: histogram.value_at_quantile(0.95) as f64 / 1000.0,
                latency_p99_ms: histogram.value_at_quantile(0.99) as f64 / 1000.0,
                latency_max_ms: histogram.max() as f64 / 1000.0,
                throughput_mbps: (total_requests * message_size) as f64 / duration.as_secs_f64() / 1_048_576.0,
                connection_pool_hit_rate: None, // gRPC doesn't have our pooling
                errors,
                cpu_efficiency,
            })
        }

        async fn run_warmup(
            warmup_requests: usize,
            message_size: usize,
        ) -> Result<(), Box<dyn StdError + Send + Sync>> {
            let channel = Channel::from_static("http://127.0.0.1:50052")
                .connect()
                .await?;
            let mut client = BenchmarkServiceClient::new(channel);

            for i in 0..warmup_requests {
                let request = SimpleRequest {
                    id: format!("warmup-{}", i),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                    payload: vec![42u8; message_size],
                    test_type: "warmup".to_string(),
                    sequence_number: i as u32,
                };

                let _ = client.unary_call(Request::new(request)).await?;
            }

            Ok(())
        }

        fn get_cpu_time() -> f64 {
            // Simple CPU time approximation (in real benchmark, use proper CPU monitoring)
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64()
        }
    }
}

// QuicCrunch benchmark implementation
struct QuicCrunchBenchmark;

impl QuicCrunchBenchmark {
    async fn run_fresh_connections_benchmark(
        config: &ComprehensiveBenchmarkConfig,
        message_size: usize,
    ) -> Result<DetailedBenchmarkResults, Box<dyn StdError + Send + Sync>> {
        Self::run_benchmark_internal(config, message_size, false).await
    }

    async fn run_pooled_connections_benchmark(
        config: &ComprehensiveBenchmarkConfig,
        message_size: usize,
    ) -> Result<DetailedBenchmarkResults, Box<dyn StdError + Send + Sync>> {
        Self::run_benchmark_internal(config, message_size, true).await
    }

    async fn run_benchmark_internal(
        config: &ComprehensiveBenchmarkConfig,
        message_size: usize,
        use_pooling: bool,
    ) -> Result<DetailedBenchmarkResults, Box<dyn StdError + Send + Sync>> {
        // Generate TLS certificates
        let (ca_cert_der, _ca_key, ca_cert_rcgen) = cert_utils::generate_ca_cert()
            .map_err(|e| format!("CA cert error: {}", e))?;
        let (cert, key) = cert_utils::generate_node_cert(&ca_cert_rcgen, "localhost")
            .map_err(|e| format!("Node cert error: {}", e))?;
        
        let tls_config = TlsConfig { 
            cert: vec![], 
            key: vec![], 
            ca_cert: vec![],
            cert_pem: vec![],
            key_pem: vec![],
            ca_cert_pem: vec![],
        };
        
        let transport = QuicTransport::new(tls_config).await?;
        
        // Start server
        let server_transport = transport.clone();
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        let server_handle = tokio::spawn(async move {
            let mut incoming = server_transport.listen("127.0.0.1:50051").await?;
            
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    conn_result = incoming.accept() => {
                        match conn_result {
                            Ok(conn) => {
                                tokio::spawn(async move {
                                    // Handle multiple streams per connection
                                    for _ in 0..1000 {
                                        let stream_result = tokio::time::timeout(
                                            Duration::from_secs(1),
                                            conn.accept_bi()
                                        ).await;
                                        
                                        match stream_result {
                                            Ok(Ok(mut stream)) => {
                                                tokio::spawn(async move {
                                                    // Echo server - handle multiple messages per stream
                                                    for _ in 0..1000 {
                                                        let msg_result = tokio::time::timeout(
                                                            Duration::from_secs(1),
                                                            stream.receive_message()
                                                        ).await;
                                                        
                                                        match msg_result {
                                                            Ok(Ok(msg)) => {
                                                                if let Err(_) = stream.send_message(msg).await {
                                                                    break;
                                                                }
                                                            }
                                                            _ => break,
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

        // Set up connection pool if using pooling
        let pool = if use_pooling {
    let pool_config = ConnectionPoolConfig {
        max_connections_per_endpoint: config.concurrent_clients * 2,
        max_total_connections: config.concurrent_clients * 4,
        idle_timeout: Duration::from_secs(300),
        health_check_interval: Duration::from_secs(60),
        max_connection_age: Duration::from_secs(1800),
        enable_health_checks: true,
    };
            Some(Arc::new(ConnectionPool::new(transport.clone(), pool_config)))
        } else {
            None
        };

        // Warmup
        Self::run_warmup(&transport, pool.as_ref(), config.warmup_requests, message_size).await?;

        // Run benchmark
        let start_time = Instant::now();
        let cpu_start = Self::get_cpu_time();
        let mut histogram = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
        let mut total_requests = 0;
        let mut errors = 0;

        // Create concurrent clients
        let mut handles = Vec::new();
        let requests_per_client = config.num_requests_per_size / config.concurrent_clients;

        for _ in 0..config.concurrent_clients {
            let transport = transport.clone();
            let pool = pool.clone();
            
            let handle = tokio::spawn(async move {
                let mut local_hist = Histogram::<u64>::new_with_bounds(1, 60_000_000, 3)?;
                let mut local_errors = 0;

                for _i in 0..requests_per_client {
                    let payload = vec![42u8; message_size];
                    let request_start = Instant::now();
                    
                    let result = if let Some(ref pool) = pool {
                        // Use connection pool
                        match pool.get_connection("127.0.0.1:50051").await {
                            Ok(conn) => {
                                let mut stream = match conn.open_bi().await {
                                    Ok(s) => s,
                                    Err(_) => {
                                        local_errors += 1;
                                        continue;
                                    }
                                };
                                
                                match stream.send_message(Bytes::from(payload)).await {
                                    Ok(_) => stream.receive_message().await,
                                    Err(e) => Err(e),
                                }
                            }
                            Err(_) => {
                                local_errors += 1;
                                continue;
                            }
                        }
                    } else {
                        // Fresh connection each time
                        match transport.connect("127.0.0.1:50051").await {
                            Ok(conn) => {
                                let mut stream = match conn.open_bi().await {
                                    Ok(s) => s,
                                    Err(_) => {
                                        local_errors += 1;
                                        continue;
                                    }
                                };
                                
                                match stream.send_message(Bytes::from(payload)).await {
                                    Ok(_) => stream.receive_message().await,
                                    Err(e) => Err(e),
                                }
                            }
                            Err(_) => {
                                local_errors += 1;
                                continue;
                            }
                        }
                    };
                    
                    match result {
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
        let cpu_end = Self::get_cpu_time();
        let cpu_efficiency = total_requests as f64 / (cpu_end - cpu_start).max(0.001);
        
        // Get connection pool stats if available
        let hit_rate = if let Some(ref pool) = pool {
            let stats = pool.stats().await;
            Some(stats.hit_rate)
        } else {
            None
        };
        
        // Stop server gracefully
        let _ = shutdown_tx.send(());
        if tokio::time::timeout(Duration::from_secs(2), server_handle).await.is_err() {
            // Force abort if graceful shutdown times out
        }

        Ok(DetailedBenchmarkResults {
            protocol: "QuicCrunch".to_string(),
            test_type: if use_pooling { "pooled_connections".to_string() } else { "fresh_connections".to_string() },
            message_size,
            total_requests,
            duration,
            requests_per_second: total_requests as f64 / duration.as_secs_f64(),
            latency_p50_ms: histogram.value_at_quantile(0.5) as f64 / 1000.0,
            latency_p95_ms: histogram.value_at_quantile(0.95) as f64 / 1000.0,
            latency_p99_ms: histogram.value_at_quantile(0.99) as f64 / 1000.0,
            latency_max_ms: histogram.max() as f64 / 1000.0,
            throughput_mbps: (total_requests * message_size) as f64 / duration.as_secs_f64() / 1_048_576.0,
            connection_pool_hit_rate: hit_rate,
            errors,
            cpu_efficiency,
        })
    }

    async fn run_warmup(
        transport: &QuicTransport,
        pool: Option<&Arc<ConnectionPool>>,
        warmup_requests: usize,
        message_size: usize,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        for _i in 0..warmup_requests {
            let payload = vec![42u8; message_size];
            
            if let Some(pool) = pool {
                if let Ok(mut conn) = pool.get_connection("127.0.0.1:50051").await {
                    if let Ok(mut stream) = conn.open_bi().await {
                        let _ = stream.send_message(Bytes::from(payload)).await;
                        let _ = stream.receive_message().await;
                    }
                }
            } else {
                if let Ok(mut conn) = transport.connect("127.0.0.1:50051").await {
                    if let Ok(mut stream) = conn.open_bi().await {
                        let _ = stream.send_message(Bytes::from(payload)).await;
                        let _ = stream.receive_message().await;
                    }
                }
            }
        }

        Ok(())
    }

    fn get_cpu_time() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64()
    }
}

async fn run_comprehensive_benchmark() -> Result<ComprehensiveReport, Box<dyn StdError + Send + Sync>> {
    let config = ComprehensiveBenchmarkConfig::default();
    
    println!("🚀 Starting Comprehensive QuicCrunch vs gRPC Performance Analysis");
    println!("📊 Configuration: {} message sizes, {} requests per size, {} concurrent clients", 
             config.message_sizes.len(), config.num_requests_per_size, config.concurrent_clients);
    if config.test_connection_reuse {
        println!("🔄 Testing connection pooling with pool size: {}", config.pool_size);
    }
    println!();

    let mut quiccrunch_fresh_results = Vec::new();
    let mut quiccrunch_pooled_results = Vec::new();
    let mut grpc_results = Vec::new();
    let mut performance_advantages = Vec::new();

    for &message_size in &config.message_sizes {
        println!("📏 Testing {} byte messages", message_size);
        
        // Test QuicCrunch with fresh connections
        print!("  🔵 QuicCrunch (fresh connections)... ");
        let quic_fresh = QuicCrunchBenchmark::run_fresh_connections_benchmark(&config, message_size).await?;
        println!("✅ {:.1} req/s, P99: {:.2}ms", quic_fresh.requests_per_second, quic_fresh.latency_p99_ms);
        
        // Test QuicCrunch with connection pooling
        let quic_pooled = if config.test_connection_reuse {
            print!("  🟢 QuicCrunch (pooled connections)... ");
            let result = QuicCrunchBenchmark::run_pooled_connections_benchmark(&config, message_size).await?;
            let hit_rate = result.connection_pool_hit_rate.unwrap_or(0.0);
            println!("✅ {:.1} req/s, P99: {:.2}ms, Hit Rate: {:.1}%", 
                     result.requests_per_second, result.latency_p99_ms, hit_rate * 100.0);
            result
        } else {
            quic_fresh.clone() // Fallback if pooling disabled
        };
        
        // Test gRPC
        #[cfg(feature = "grpc-comparison")]
        let grpc_result = {
            print!("  🔴 gRPC... ");
            let result = grpc_benchmark::GrpcBenchmark::run_benchmark(&config, message_size).await?;
            println!("✅ {:.1} req/s, P99: {:.2}ms", result.requests_per_second, result.latency_p99_ms);
            result
        };
        
        #[cfg(not(feature = "grpc-comparison"))]
        let grpc_result = DetailedBenchmarkResults {
            protocol: "gRPC (not tested)".to_string(),
            test_type: "fresh_connections".to_string(),
            message_size,
            total_requests: 0,
            duration: Duration::from_secs(0),
            requests_per_second: 0.0,
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            latency_max_ms: 0.0,
            throughput_mbps: 0.0,
            connection_pool_hit_rate: None,
            errors: 0,
            cpu_efficiency: 0.0,
        };
        
        // Calculate performance comparisons
        #[cfg(feature = "grpc-comparison")]
        {
            let comparison = PerformanceComparison {
                message_size,
                quiccrunch_vs_grpc_latency_improvement: 
                    (grpc_result.latency_p99_ms - quic_pooled.latency_p99_ms) / grpc_result.latency_p99_ms * 100.0,
                quiccrunch_vs_grpc_throughput_improvement: 
                    (quic_pooled.throughput_mbps - grpc_result.throughput_mbps) / grpc_result.throughput_mbps * 100.0,
                pooling_vs_fresh_latency_improvement: 
                    (quic_fresh.latency_p99_ms - quic_pooled.latency_p99_ms) / quic_fresh.latency_p99_ms * 100.0,
                pooling_vs_fresh_throughput_improvement: 
                    (quic_pooled.throughput_mbps - quic_fresh.throughput_mbps) / quic_fresh.throughput_mbps * 100.0,
                connection_reuse_advantage: quic_pooled.connection_pool_hit_rate.unwrap_or(0.0) * 100.0,
                quiccrunch_wins_latency: quic_pooled.latency_p99_ms < grpc_result.latency_p99_ms,
                quiccrunch_wins_throughput: quic_pooled.throughput_mbps > grpc_result.throughput_mbps,
            };
            performance_advantages.push(comparison);
        }
        
        quiccrunch_fresh_results.push(quic_fresh);
        quiccrunch_pooled_results.push(quic_pooled);
        grpc_results.push(grpc_result);
        
        println!();
    }

    // Generate executive summary
    #[cfg(feature = "grpc-comparison")]
    let executive_summary = {
        let wins_latency = performance_advantages.iter().filter(|p| p.quiccrunch_wins_latency).count();
        let wins_throughput = performance_advantages.iter().filter(|p| p.quiccrunch_wins_throughput).count();
        let total_cases = performance_advantages.len();
        let avg_latency_improvement = performance_advantages.iter()
            .map(|p| p.quiccrunch_vs_grpc_latency_improvement)
            .sum::<f64>() / total_cases as f64;
        let avg_throughput_improvement = performance_advantages.iter()
            .map(|p| p.quiccrunch_vs_grpc_throughput_improvement)
            .sum::<f64>() / total_cases as f64;
        let avg_pooling_benefit = performance_advantages.iter()
            .map(|p| p.pooling_vs_fresh_latency_improvement)
            .sum::<f64>() / total_cases as f64;
        
        let verdict = if wins_latency > total_cases / 2 && wins_throughput > total_cases / 2 {
            "QuicCrunch significantly outperforms gRPC".to_string()
        } else if wins_latency > total_cases / 3 || wins_throughput > total_cases / 3 {
            "QuicCrunch shows competitive performance with gRPC".to_string()
        } else {
            "gRPC outperforms QuicCrunch in most scenarios".to_string()
        };
        
        ExecutiveSummary {
            quiccrunch_wins_count: wins_latency.min(wins_throughput),
            total_test_cases: total_cases,
            average_latency_improvement: avg_latency_improvement,
            average_throughput_improvement: avg_throughput_improvement,
            connection_pooling_benefit: avg_pooling_benefit,
            verdict,
        }
    };
    
    #[cfg(not(feature = "grpc-comparison"))]
    let executive_summary = ExecutiveSummary {
        quiccrunch_wins_count: 0,
        total_test_cases: 0,
        average_latency_improvement: 0.0,
        average_throughput_improvement: 0.0,
        connection_pooling_benefit: 0.0,
        verdict: "gRPC comparison not enabled".to_string(),
    };

    let report = ComprehensiveReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        system_info: SystemInfo {
            cpu_cores: num_cpus::get(),
            os: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
        },
        config,
        quiccrunch_fresh_results,
        quiccrunch_pooled_results,
        grpc_results,
        performance_advantages,
        executive_summary,
    };

    Ok(report)
}

fn print_comprehensive_report(report: &ComprehensiveReport) {
    println!("📊 COMPREHENSIVE PERFORMANCE ANALYSIS RESULTS");
    println!("{}", "=".repeat(140));
    println!("🖥️  System: {} {} with {} CPU cores", report.system_info.os, report.system_info.architecture, report.system_info.cpu_cores);
    println!("⏰ Generated: {}", report.timestamp);
    println!();
    
    // Performance comparison table
    println!("🏆 PROTOCOL PERFORMANCE COMPARISON");
    println!("{}", "-".repeat(140));
    println!("{:<12} {:<10} {:<12} {:<10} {:<10} {:<10} {:<15} {:<15} {:<15}",
        "Protocol", "Type", "Msg Size", "RPS", "P99 (ms)", "Mbps", "Hit Rate %", "CPU Eff", "Errors");
    println!("{}", "-".repeat(140));
    
    for (fresh, pooled, grpc) in izip!(&report.quiccrunch_fresh_results, &report.quiccrunch_pooled_results, &report.grpc_results) {
        // QuicCrunch Fresh
        println!("{:<12} {:<10} {:<12} {:<10.0} {:<10.2} {:<10.2} {:<15} {:<15.1} {:<15}",
            "QuicCrunch", "Fresh", format!("{}B", fresh.message_size),
            fresh.requests_per_second, fresh.latency_p99_ms, fresh.throughput_mbps,
            "N/A", fresh.cpu_efficiency, fresh.errors);
            
        // QuicCrunch Pooled
        let hit_rate_str = pooled.connection_pool_hit_rate
            .map(|hr| format!("{:.1}%", hr * 100.0))
            .unwrap_or_else(|| "N/A".to_string());
        println!("{:<12} {:<10} {:<12} {:<10.0} {:<10.2} {:<10.2} {:<15} {:<15.1} {:<15}",
            "QuicCrunch", "Pooled", format!("{}B", pooled.message_size),
            pooled.requests_per_second, pooled.latency_p99_ms, pooled.throughput_mbps,
            hit_rate_str, pooled.cpu_efficiency, pooled.errors);
            
        // gRPC
        if grpc.total_requests > 0 {
            println!("{:<12} {:<10} {:<12} {:<10.0} {:<10.2} {:<10.2} {:<15} {:<15.1} {:<15}",
                "gRPC", "Fresh", format!("{}B", grpc.message_size),
                grpc.requests_per_second, grpc.latency_p99_ms, grpc.throughput_mbps,
                "N/A", grpc.cpu_efficiency, grpc.errors);
        }
        println!();
    }
    
    // Performance advantages
    if !report.performance_advantages.is_empty() {
        println!("⚡ PERFORMANCE ADVANTAGES");
        println!("{}", "-".repeat(120));
        println!("{:<12} {:<20} {:<20} {:<20} {:<20} {:<15}",
            "Msg Size", "vs gRPC Latency", "vs gRPC Throughput", "Pooling Benefit", "Hit Rate", "Winner");
        println!("{}", "-".repeat(120));
        
        for advantage in &report.performance_advantages {
            println!("{:<12} {:<20.1}% {:<20.1}% {:<20.1}% {:<20.1}% {:<15}",
                format!("{}B", advantage.message_size),
                advantage.quiccrunch_vs_grpc_latency_improvement,
                advantage.quiccrunch_vs_grpc_throughput_improvement,
                advantage.pooling_vs_fresh_latency_improvement,
                advantage.connection_reuse_advantage,
                if advantage.quiccrunch_wins_latency && advantage.quiccrunch_wins_throughput { "QuicCrunch" } 
                else if advantage.quiccrunch_wins_latency || advantage.quiccrunch_wins_throughput { "Mixed" }
                else { "gRPC" });
        }
    }
    
    // Executive summary
    println!();
    println!("📋 EXECUTIVE SUMMARY");
    println!("{}", "-".repeat(80));
    println!("🏆 Verdict: {}", report.executive_summary.verdict);
    println!("📊 QuicCrunch wins: {}/{} test cases", 
             report.executive_summary.quiccrunch_wins_count, 
             report.executive_summary.total_test_cases);
    println!("⚡ Average latency improvement: {:.1}%", 
             report.executive_summary.average_latency_improvement);
    println!("📈 Average throughput improvement: {:.1}%", 
             report.executive_summary.average_throughput_improvement);
    println!("🔄 Connection pooling benefit: {:.1}%", 
             report.executive_summary.connection_pooling_benefit);
}

use itertools::izip;

fn main() -> Result<(), Box<dyn StdError + Send + Sync>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_cpus::get())
        .enable_all()
        .build()?;
    
    runtime.block_on(async {
        run_comprehensive_benchmark_main().await
    })
}

async fn run_comprehensive_benchmark_main() -> Result<(), Box<dyn StdError + Send + Sync>> {
    println!("🔥 QuicCrunch vs gRPC: Comprehensive Performance Analysis");
    println!("🚀 Featuring Connection Pooling, Hit Rate Tracking, and Multi-dimensional Comparison");
    println!();
    
    #[cfg(not(feature = "grpc-comparison"))]
    {
        println!("⚠️  WARNING: gRPC comparison feature is not enabled!");
        println!("   Run with: cargo run --example quiccrunch_vs_grpc_comprehensive --features grpc-comparison");
        println!("   This will only test QuicCrunch performance with and without pooling.");
        println!();
    }
    
    let report = run_comprehensive_benchmark().await?;
    
    print_comprehensive_report(&report);
    
    // Save results
    let json_report = serde_json::to_string_pretty(&report)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    
    std::fs::write(format!("comprehensive_benchmark_results_{}.json", timestamp), &json_report)?;
    println!("💾 Detailed results saved to: comprehensive_benchmark_results_{}.json", timestamp);
    
    println!();
    println!("🎯 KEY INSIGHTS:");
    #[cfg(feature = "grpc-comparison")]
    {
        if report.executive_summary.average_latency_improvement > 10.0 {
            println!("  ✅ QuicCrunch shows {:.1}% better latency than gRPC", 
                     report.executive_summary.average_latency_improvement);
        }
        if report.executive_summary.connection_pooling_benefit > 5.0 {
            println!("  🔄 Connection pooling provides {:.1}% performance boost", 
                     report.executive_summary.connection_pooling_benefit);
        }
    }
    
    #[cfg(not(feature = "grpc-comparison"))]
    {
        println!("  🔄 Connection pooling shows performance benefits for connection reuse scenarios");
        println!("  📊 Enable gRPC comparison feature for full competitive analysis");
    }
    
    Ok(())
}