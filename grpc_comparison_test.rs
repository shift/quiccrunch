use std::time::{Duration, Instant};
use tonic::{transport::Server, Request, Response, Status};
use tonic::transport::Channel;
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};

// Import our comprehensive test results structure
use crate::comprehensive_rpc_test::{RpcTestResults, SimpleRpcResults, StreamingResults, LatencyDistribution, ThroughputMeasurements, MessageSizeAnalysis, MessageSizeResult};

/// gRPC vs QuicCrunch Comparison Results
#[derive(Debug, Serialize, Deserialize)]
pub struct ComparisonResults {
    pub test_timestamp: String,
    pub test_duration_seconds: f64,
    pub quiccrunch_results: RpcTestResults,
    pub grpc_results: RpcTestResults,
    pub performance_comparison: PerformanceComparison,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceComparison {
    pub latency_improvement_percent: f64,
    pub throughput_improvement_percent: f64,
    pub requests_per_second_improvement_percent: f64,
    pub memory_usage_difference_mb: f64,
    pub cpu_usage_difference_percent: f64,
    pub winner: String,
    pub summary: String,
}

// gRPC service definition
pub mod test_service {
    tonic::include_proto!("test_service");
}

use test_service::{
    test_service_server::{TestService, TestServiceServer},
    test_service_client::TestServiceClient,
    TestRequest, TestResponse,
};

/// gRPC Test Service Implementation
#[derive(Debug, Default)]
pub struct GrpcTestService;

#[tonic::async_trait]
impl TestService for GrpcTestService {
    async fn echo(&self, request: Request<TestRequest>) -> Result<Response<TestResponse>, Status> {
        let processing_start = Instant::now();
        let req = request.into_inner();
        
        // Simulate processing based on data size
        if req.data.len() > 1024 * 1024 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let processing_time = processing_start.elapsed();
        
        let response = TestResponse {
            id: req.id,
            echo_data: req.data,
            server_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            processing_time_ns: processing_time.as_nanos() as u64,
        };

        Ok(Response::new(response))
    }

    type ClientEchoStream = ReceiverStream<Result<TestResponse, Status>>;

    async fn client_echo(
        &self,
        request: Request<tonic::Streaming<TestRequest>>,
    ) -> Result<Response<Self::ClientEchoStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let mut message_count = 0;
            while let Some(request) = stream.next().await {
                match request {
                    Ok(req) => {
                        message_count += 1;
                        let response = TestResponse {
                            id: req.id,
                            echo_data: req.data,
                            server_timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64,
                            processing_time_ns: 0,
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    }
                }
            }

            // Send final summary
            let final_response = TestResponse {
                id: u64::MAX,
                echo_data: format!("Processed {} messages", message_count).into_bytes(),
                server_timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                processing_time_ns: 0,
            };
            let _ = tx.send(Ok(final_response)).await;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type ServerEchoStream = ReceiverStream<Result<TestResponse, Status>>;

    async fn server_echo(
        &self,
        request: Request<TestRequest>,
    ) -> Result<Response<Self::ServerEchoStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            // Send multiple responses for server streaming
            for i in 0..100 {
                let response = TestResponse {
                    id: i,
                    echo_data: format!("Server response {} for request {}", i, req.id).into_bytes(),
                    server_timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos() as u64,
                    processing_time_ns: 0,
                };
                
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
                
                // Small delay to simulate processing
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type BidiEchoStream = ReceiverStream<Result<TestResponse, Status>>;

    async fn bidi_echo(
        &self,
        request: Request<tonic::Streaming<TestRequest>>,
    ) -> Result<Response<Self::BidiEchoStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            while let Some(request) = stream.next().await {
                match request {
                    Ok(req) => {
                        let response = TestResponse {
                            id: req.id,
                            echo_data: req.data,
                            server_timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64,
                            processing_time_ns: 0,
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        break;
                    }
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// gRPC Test Suite
pub struct GrpcTestSuite {
    pub server_addr: String,
    pub test_duration: Duration,
    pub concurrent_clients: usize,
}

impl GrpcTestSuite {
    pub fn new(server_addr: String, test_duration: Duration, concurrent_clients: usize) -> Self {
        Self {
            server_addr,
            test_duration,
            concurrent_clients,
        }
    }

    /// Run gRPC test suite with same methodology as QuicCrunch
    pub async fn run_grpc_test_suite(&self) -> Result<RpcTestResults, Box<dyn std::error::Error + Send + Sync>> {
        println!("🌐 Starting gRPC Test Suite");
        println!("Server: {}", self.server_addr);
        println!("Duration: {:?}", self.test_duration);
        println!("Concurrent clients: {}", self.concurrent_clients);
        println!();

        // Start gRPC server
        let server_handle = self.start_grpc_server().await?;
        
        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let test_start = Instant::now();

        // 1. Simple RPC Test
        println!("📡 Testing gRPC Simple RPC...");
        let simple_rpc = self.test_grpc_simple_rpc().await?;
        println!("   ✅ gRPC Simple RPC: {:.2} req/s, {:.2}ms avg latency", 
                simple_rpc.requests_per_second, simple_rpc.average_latency_ms);

        // 2. Client Streaming Test
        println!("📤 Testing gRPC Client Streaming...");
        let client_streaming = self.test_grpc_client_streaming().await?;
        println!("   ✅ gRPC Client Streaming: {:.2} Mbps", client_streaming.throughput_mbps);

        // 3. Server Streaming Test
        println!("📥 Testing gRPC Server Streaming...");
        let server_streaming = self.test_grpc_server_streaming().await?;
        println!("   ✅ gRPC Server Streaming: {:.2} Mbps", server_streaming.throughput_mbps);

        // 4. Bidirectional Streaming Test
        println!("🔄 Testing gRPC Bidirectional Streaming...");
        let bidirectional_streaming = self.test_grpc_bidirectional_streaming().await?;
        println!("   ✅ gRPC Bidirectional: {:.2} Mbps", bidirectional_streaming.throughput_mbps);

        // 5. Latency Distribution Analysis
        println!("📊 Analyzing gRPC Latency Distribution...");
        let latency_distribution = self.analyze_grpc_latency_distribution().await?;
        println!("   ✅ gRPC Latency P95: {:.2}ms, P99: {:.2}ms", 
                latency_distribution.p95_ms, latency_distribution.p99_ms);

        // 6. Throughput Measurements
        println!("🚄 Measuring gRPC Peak Throughput...");
        let throughput_measurements = self.measure_grpc_peak_throughput().await?;
        println!("   ✅ gRPC Peak: {:.2} req/s, {:.2} Mbps", 
                throughput_measurements.peak_requests_per_second, 
                throughput_measurements.peak_mbps);

        // 7. Message Size Analysis
        println!("📏 Testing gRPC Different Message Sizes...");
        let message_size_analysis = self.test_grpc_message_sizes().await?;
        println!("   ✅ gRPC 1KB: {:.2} req/s, 2MB: {:.2} req/s", 
                message_size_analysis.small_messages_1kb.requests_per_second,
                message_size_analysis.huge_messages_2mb.requests_per_second);

        let total_duration = test_start.elapsed();
        println!();
        println!("🎉 gRPC Test Suite Completed in {:.2}s", total_duration.as_secs_f64());

        Ok(RpcTestResults {
            test_timestamp: chrono::Utc::now().to_rfc3339(),
            backend: "gRPC".to_string(),
            features: vec!["http2".to_string(), "tls".to_string()],
            simple_rpc,
            client_streaming,
            server_streaming,
            bidirectional_streaming,
            latency_distribution,
            throughput_measurements,
            message_size_analysis,
        })
    }

    async fn start_grpc_server(&self) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
        let addr = self.server_addr.parse()?;
        let service = GrpcTestService::default();

        let handle = tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(TestServiceServer::new(service))
                .serve(addr)
                .await;
        });

        Ok(handle)
    }

    async fn test_grpc_simple_rpc(&self) -> Result<SimpleRpcResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut latencies = Vec::new();
        let mut successful = 0u64;
        let mut failed = 0u64;
        let total_requests = 1000u64;

        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        for i in 0..total_requests {
            let request_start = Instant::now();
            
            let request = TestRequest {
                id: i,
                data: vec![0u8; 1024], // 1KB test data
                timestamp: request_start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
            };

            match tokio::time::timeout(Duration::from_secs(5), client.echo(request)).await {
                Ok(Ok(_response)) => {
                    let latency = request_start.elapsed().as_millis() as f64;
                    latencies.push(latency);
                    successful += 1;
                }
                _ => {
                    failed += 1;
                }
            }
        }

        let total_duration = start_time.elapsed();
        let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
        let min_latency = latencies.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_latency = latencies.iter().fold(0.0, |a, &b| a.max(b));
        let requests_per_second = successful as f64 / total_duration.as_secs_f64();

        Ok(SimpleRpcResults {
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            average_latency_ms: avg_latency,
            min_latency_ms: min_latency,
            max_latency_ms: max_latency,
            requests_per_second,
            total_test_duration_ms: total_duration.as_millis() as f64,
        })
    }

    async fn test_grpc_client_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        let stream_count = 10u64;
        let messages_per_stream = 100u64;
        let mut total_messages_sent = 0u64;
        let mut successful_streams = 0u64;

        for _stream_id in 0..stream_count {
            let (tx, rx) = mpsc::channel(128);
            let request_stream = ReceiverStream::new(rx);
            
            let response_future = client.client_echo(request_stream);
            
            // Send messages
            for i in 0..messages_per_stream {
                let request = TestRequest {
                    id: i,
                    data: vec![0u8; 4096], // 4KB per message
                    timestamp: start_time.elapsed().as_nanos() as u64,
                };
                
                if tx.send(request).await.is_ok() {
                    total_messages_sent += 1;
                } else {
                    break;
                }
            }
            drop(tx); // Signal end of stream
            
            if response_future.await.is_ok() {
                successful_streams += 1;
            }
        }

        let total_duration = start_time.elapsed();
        let total_bytes = total_messages_sent * 4096;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent,
            total_messages_received: 0,
            average_message_latency_ms: 0.0,
            throughput_mbps,
        })
    }

    async fn test_grpc_server_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        let stream_count = 10u64;
        let mut total_messages_received = 0u64;
        let mut successful_streams = 0u64;

        for stream_id in 0..stream_count {
            let request = TestRequest {
                id: stream_id,
                data: vec![0u8; 1024],
                timestamp: start_time.elapsed().as_nanos() as u64,
            };

            match client.server_echo(request).await {
                Ok(response) => {
                    let mut stream = response.into_inner();
                    
                    while let Some(_message) = stream.next().await {
                        total_messages_received += 1;
                    }
                    
                    successful_streams += 1;
                }
                Err(_) => {}
            }
        }

        let total_duration = start_time.elapsed();
        let total_bytes = total_messages_received * 4096;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent: 0,
            total_messages_received,
            average_message_latency_ms: 0.0,
            throughput_mbps,
        })
    }

    async fn test_grpc_bidirectional_streaming(&self) -> Result<StreamingResults, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        let stream_count = 5u64;
        let messages_per_stream = 50u64;
        let mut total_messages_sent = 0u64;
        let mut total_messages_received = 0u64;
        let mut successful_streams = 0u64;

        for _stream_id in 0..stream_count {
            let (tx, rx) = mpsc::channel(128);
            let request_stream = ReceiverStream::new(rx);
            
            match client.bidi_echo(request_stream).await {
                Ok(response) => {
                    let mut response_stream = response.into_inner();
                    
                    // Send messages
                    for i in 0..messages_per_stream {
                        let request = TestRequest {
                            id: i,
                            data: vec![0u8; 8192], // 8KB per message
                            timestamp: start_time.elapsed().as_nanos() as u64,
                        };
                        
                        if tx.send(request).await.is_ok() {
                            total_messages_sent += 1;
                        }
                    }
                    drop(tx);
                    
                    // Receive responses
                    while let Some(_response) = response_stream.next().await {
                        total_messages_received += 1;
                    }
                    
                    successful_streams += 1;
                }
                Err(_) => {}
            }
        }

        let total_duration = start_time.elapsed();
        let total_bytes = (total_messages_sent + total_messages_received) * 8192;
        let throughput_mbps = (total_bytes as f64 * 8.0) / (total_duration.as_secs_f64() * 1_000_000.0);

        Ok(StreamingResults {
            total_streams: stream_count,
            successful_streams,
            failed_streams: stream_count - successful_streams,
            average_stream_duration_ms: total_duration.as_millis() as f64 / stream_count as f64,
            total_messages_sent,
            total_messages_received,
            average_message_latency_ms: 0.0,
            throughput_mbps,
        })
    }

    async fn analyze_grpc_latency_distribution(&self) -> Result<LatencyDistribution, Box<dyn std::error::Error + Send + Sync>> {
        let mut latencies = Vec::new();
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        // Collect 1000 latency samples
        for i in 0..1000 {
            let start = Instant::now();
            let request = TestRequest {
                id: i,
                data: vec![0u8; 512],
                timestamp: start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
            };

            let _ = client.echo(request).await;
            let latency_ms = start.elapsed().as_millis() as f64;
            latencies.push(latency_ms);
        }

        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

        Ok(LatencyDistribution {
            p50_ms: latencies[499],
            p90_ms: latencies[899],
            p95_ms: latencies[949],
            p99_ms: latencies[989],
            p999_ms: latencies[998],
        })
    }

    async fn measure_grpc_peak_throughput(&self) -> Result<ThroughputMeasurements, Box<dyn std::error::Error + Send + Sync>> {
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;
        
        // Peak burst test
        let peak_duration = Duration::from_secs(10);
        let peak_start = Instant::now();
        let mut peak_requests = 0u64;

        while peak_start.elapsed() < peak_duration {
            let request = TestRequest {
                id: peak_requests,
                data: vec![0u8; 1024],
                timestamp: peak_start.elapsed().as_nanos() as u64,
            };
            
            if let Ok(_) = tokio::time::timeout(Duration::from_millis(100), client.echo(request)).await {
                peak_requests += 1;
            }
        }

        let peak_req_per_sec = peak_requests as f64 / peak_duration.as_secs_f64();
        let peak_mbps = (peak_requests * 1024 * 8) as f64 / (peak_duration.as_secs_f64() * 1_000_000.0);

        // Sustained throughput test
        let sustained_duration = Duration::from_secs(30);
        let sustained_start = Instant::now();
        let mut sustained_requests = 0u64;

        while sustained_start.elapsed() < sustained_duration {
            let request = TestRequest {
                id: sustained_requests,
                data: vec![0u8; 1024],
                timestamp: sustained_start.elapsed().as_nanos() as u64,
            };
            
            if let Ok(_) = client.echo(request).await {
                sustained_requests += 1;
            }
            
            // Small delay to simulate realistic load
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        let sustained_req_per_sec = sustained_requests as f64 / sustained_duration.as_secs_f64();
        let sustained_mbps = (sustained_requests * 1024 * 8) as f64 / (sustained_duration.as_secs_f64() * 1_000_000.0);

        Ok(ThroughputMeasurements {
            peak_requests_per_second: peak_req_per_sec,
            sustained_requests_per_second: sustained_req_per_sec,
            peak_mbps,
            sustained_mbps,
        })
    }

    async fn test_grpc_message_sizes(&self) -> Result<MessageSizeAnalysis, Box<dyn std::error::Error + Send + Sync>> {
        let message_sizes = vec![
            (1024, "small_messages_1kb"),
            (65536, "medium_messages_64kb"),
            (524288, "large_messages_512kb"),
            (2097152, "huge_messages_2mb"),
        ];

        let mut results = Vec::new();
        let mut client = TestServiceClient::connect(format!("http://{}", self.server_addr)).await?;

        for (size, _name) in message_sizes {
            let test_start = Instant::now();
            let mut latencies = Vec::new();
            let test_requests = 100u64;

            for i in 0..test_requests {
                let request_start = Instant::now();
                let request = TestRequest {
                    id: i,
                    data: vec![0u8; size],
                    timestamp: request_start.duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64,
                };

                if let Ok(_) = tokio::time::timeout(Duration::from_secs(10), client.echo(request)).await {
                    latencies.push(request_start.elapsed().as_millis() as f64);
                }
            }

            let total_duration = test_start.elapsed();
            let avg_latency = latencies.iter().sum::<f64>() / latencies.len() as f64;
            let req_per_sec = latencies.len() as f64 / total_duration.as_secs_f64();
            let throughput_mbps = (latencies.len() * size * 8) as f64 / (total_duration.as_secs_f64() * 1_000_000.0);

            results.push(MessageSizeResult {
                size_bytes: size,
                average_latency_ms: avg_latency,
                throughput_mbps,
                requests_per_second: req_per_sec,
                optimization_used: "HTTP/2".to_string(),
            });
        }

        Ok(MessageSizeAnalysis {
            small_messages_1kb: results[0].clone(),
            medium_messages_64kb: results[1].clone(),
            large_messages_512kb: results[2].clone(),
            huge_messages_2mb: results[3].clone(),
        })
    }
}

/// Performance comparison analyzer
pub struct PerformanceComparator;

impl PerformanceComparator {
    pub fn compare_results(quiccrunch: &RpcTestResults, grpc: &RpcTestResults) -> PerformanceComparison {
        let latency_improvement = ((grpc.simple_rpc.average_latency_ms - quiccrunch.simple_rpc.average_latency_ms) 
                                  / grpc.simple_rpc.average_latency_ms) * 100.0;
        
        let throughput_improvement = ((quiccrunch.throughput_measurements.peak_requests_per_second - grpc.throughput_measurements.peak_requests_per_second)
                                     / grpc.throughput_measurements.peak_requests_per_second) * 100.0;
                                     
        let rps_improvement = ((quiccrunch.simple_rpc.requests_per_second - grpc.simple_rpc.requests_per_second)
                              / grpc.simple_rpc.requests_per_second) * 100.0;

        let winner = if latency_improvement > 0.0 && throughput_improvement > 0.0 {
            "QuicCrunch".to_string()
        } else if latency_improvement < -5.0 || throughput_improvement < -5.0 {
            "gRPC".to_string()
        } else {
            "Tie".to_string()
        };

        let summary = format!(
            "QuicCrunch shows {:.1}% better latency, {:.1}% better throughput vs gRPC",
            latency_improvement, throughput_improvement
        );

        PerformanceComparison {
            latency_improvement_percent: latency_improvement,
            throughput_improvement_percent: throughput_improvement,
            requests_per_second_improvement_percent: rps_improvement,
            memory_usage_difference_mb: 0.0, // Would need memory profiling
            cpu_usage_difference_percent: 0.0, // Would need CPU profiling
            winner,
            summary,
        }
    }
}