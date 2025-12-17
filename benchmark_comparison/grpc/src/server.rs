use tonic::{transport::Server, Request, Response, Status, Streaming};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};

use crate::proto::benchmark::{
    benchmark_service_server::BenchmarkService,
    SimpleRequest, SimpleResponse, ServerMetrics,
    BenchmarkRequest, BenchmarkResponse,
};
use crate::common::{get_current_timestamp_us, generate_payload, PayloadPattern};

#[derive(Debug, Clone)]
pub struct BenchmarkServer {
    metrics: Arc<ServerMetricsState>,
    config: ServerConfig,
}

#[derive(Debug, Clone)]
struct ServerConfig {
    payload_size: usize,
    processing_delay: Duration,
    cpu_work_intensity: u32,
}

#[derive(Debug)]
struct ServerMetricsState {
    concurrent_connections: std::sync::atomic::AtomicU32,
    total_messages: std::sync::atomic::AtomicU64,
    start_time: std::time::Instant,
}

impl Default for ServerMetricsState {
    fn default() -> Self {
        Self {
            concurrent_connections: std::sync::atomic::AtomicU32::new(0),
            total_messages: std::sync::atomic::AtomicU64::new(0),
            start_time: std::time::Instant::now(),
        }
    }
}

impl BenchmarkServer {
    pub fn new() -> Self {
        Self::with_config(ServerConfig {
            payload_size: 1024,
            processing_delay: Duration::from_micros(100),
            cpu_work_intensity: 1000,
        })
    }

    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            metrics: Arc::new(ServerMetricsState::default()),
            config,
        }
    }

    fn process_request(&self, request: &SimpleRequest) -> SimpleResponse {
        let start_time = Instant::now();
        
        // Increment message counter
        self.metrics.total_messages.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Simulate processing work
        self.simulate_workload();
        
        // Generate response payload
        let response_payload = generate_payload(self.config.payload_size, &PayloadPattern::Random);
        
        let processing_time = start_time.elapsed();
        debug!("Request processed in {:?}", processing_time);
        
        // Metrics would be recorded here in a full implementation
        
        SimpleResponse {
            id: request.id.clone(),
            timestamp: get_current_timestamp_us(),
            payload: response_payload,
            status: "success".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                cpu_usage: self.get_cpu_usage(),
                memory_usage: self.get_memory_usage(),
            }),
        }
    }

    fn simulate_workload(&self) {
        // Simulate CPU-bound work
        let mut sum = 0u64;
        for i in 0..self.config.cpu_work_intensity {
            sum = sum.wrapping_add(i * i);
        }
        
        // Simulate I/O delay
        if self.config.processing_delay > Duration::ZERO {
            tokio::task::block_in_place(|| {
                std::thread::sleep(self.config.processing_delay);
            });
        }
        
        // Prevent compiler optimization
        std::hint::black_box(sum);
    }

    fn get_cpu_usage(&self) -> f32 {
        // Simple CPU usage estimation based on request rate
        let elapsed = self.metrics.start_time.elapsed();
        let messages_per_sec = self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed) as f64 / elapsed.as_secs_f64();
        (messages_per_sec * 0.001).min(100.0) as f32
    }

    fn get_memory_usage(&self) -> u64 {
        // Estimate memory usage based on concurrent connections and message count
        let base_memory = 50 * 1024 * 1024; // 50MB base
        let per_connection_memory = 1024 * 1024; // 1MB per connection
        let connections = self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed) as u64;
        base_memory + (connections * per_connection_memory)
    }
}

#[tonic::async_trait]
impl BenchmarkService for BenchmarkServer {
    async fn unary_call(&self, request: Request<SimpleRequest>) -> Result<Response<SimpleResponse>, Status> {
        let req = request.into_inner();
        info!("Received unary call: {}", req.id);
        
        let response = self.process_request(&req);
        Ok(Response::new(response))
    }

    type ClientStreamingStream = Response<SimpleResponse>;

    async fn client_streaming(
        &self,
        request: Request<Streaming<SimpleRequest>>,
    ) -> Result<Self::ClientStreamingStream, Status> {
        info!("Received client streaming request");
        
        let mut stream = request.into_inner();
        let mut message_count = 0u64;
        let mut total_size = 0u64;
        let first_request_id = String::new();
        
        while let Some(req_result) = stream.next().await {
            match req_result {
                Ok(req) => {
                    message_count += 1;
                    total_size += req.payload.len() as u64;
                    debug!("Processed client stream message: {}", req.id);
                }
                Err(e) => {
                    error!("Error in client stream: {}", e);
                    return Err(e);
                }
            }
        }
        
        let response = SimpleResponse {
            id: first_request_id,
            timestamp: get_current_timestamp_us(),
            payload: generate_payload(self.config.payload_size, &PayloadPattern::Random),
            status: "success".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                cpu_usage: self.get_cpu_usage(),
                memory_usage: self.get_memory_usage(),
            }),
        };
        
        // Metrics would be recorded here in a full implementation
        
        Ok(Response::new(response))
    }

    type ServerStreamingStream = ReceiverStream<Result<SimpleResponse, Status>>;

    async fn server_streaming(
        &self,
        request: Request<SimpleRequest>,
    ) -> Result<Self::ServerStreamingStream, Status> {
        let req = request.into_inner();
        info!("Received server streaming request: {}", req.id);
        
        let (tx, rx) = mpsc::channel(128);
        let server = self.clone();
        let request_id = req.id.clone();
        
        tokio::spawn(async move {
            let stream_count = 10; // Number of responses to send
            for i in 0..stream_count {
                let response_payload = generate_payload(server.config.payload_size, &PayloadPattern::Random);
                let response = SimpleResponse {
                    id: format!("{}-{}", request_id, i),
                    timestamp: get_current_timestamp_us(),
                    payload: response_payload,
                    status: "streaming".to_string(),
                    metrics: Some(ServerMetrics {
                        concurrent_connections: server.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                        total_messages: server.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                        cpu_usage: server.get_cpu_usage(),
                        memory_usage: server.get_memory_usage(),
                    }),
                };
                
                if tx.send(Ok(response)).await.is_err() {
                    warn!("Client disconnected from server stream");
                    break;
                }
                
                // Small delay between stream messages
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        
        // Metrics would be recorded here in a full implementation
        Ok(ReceiverStream::new(rx))
    }

    type BidirectionalStreamingStream = ReceiverStream<Result<SimpleResponse, Status>>;

    async fn bidirectional_streaming(
        &self,
        request: Request<Streaming<SimpleRequest>>,
    ) -> Result<Self::BidirectionalStreamingStream, Status> {
        info!("Received bidirectional streaming request");
        
        let (tx, rx) = mpsc::channel(128);
        let server = self.clone();
        let mut stream = request.into_inner();
        
        tokio::spawn(async move {
            while let Some(req_result) = stream.next().await {
                match req_result {
                    Ok(req) => {
                        let response = server.process_request(&req);
                        if tx.send(Ok(response)).await.is_err() {
                            warn!("Client disconnected from bidirectional stream");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error in bidirectional stream: {}", e);
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });
        
        // Metrics would be recorded here in a full implementation
        Ok(ReceiverStream::new(rx))
    }

    async fn latency_test(&self, request: Request<BenchmarkRequest>) -> Result<Response<BenchmarkResponse>, Status> {
        let req = request.into_inner();
        info!("Received latency test request: {}", req.id);
        
        let response = BenchmarkResponse {
            id: req.id,
            timestamp: get_current_timestamp_us(),
            payload: generate_payload(self.config.payload_size, &PayloadPattern::Random),
            status: "success".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                cpu_usage: self.get_cpu_usage(),
                memory_usage: self.get_memory_usage(),
            }),
        };
        
        Ok(Response::new(response))
    }

    type FloodTestStream = Response<BenchmarkResponse>;

    async fn flood_test(
        &self,
        request: Request<Streaming<BenchmarkRequest>>,
    ) -> Result<Self::FloodTestStream, Status> {
        info!("Received flood test request");
        
        let mut stream = request.into_inner();
        let mut message_count = 0u64;
        
        while let Some(req_result) = stream.next().await {
            match req_result {
                Ok(_) => {
                    message_count += 1;
                }
                Err(e) => {
                    error!("Error in flood test: {}", e);
                    return Err(e);
                }
            }
        }
        
        let response = BenchmarkResponse {
            id: "flood-test-response".to_string(),
            timestamp: get_current_timestamp_us(),
            payload: generate_payload(self.config.payload_size, &PayloadPattern::Random),
            status: "success".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                cpu_usage: self.get_cpu_usage(),
                memory_usage: self.get_memory_usage(),
            }),
        };
        
        Ok(Response::new(response))
    }

    type StreamTestStream = ReceiverStream<Result<BenchmarkResponse, Status>>;

    async fn stream_test(
        &self,
        request: Request<Streaming<BenchmarkRequest>>,
    ) -> Result<Self::StreamTestStream, Status> {
        info!("Received stream test request");
        
        let (tx, rx) = mpsc::channel(128);
        let server = self.clone();
        let mut stream = request.into_inner();
        
        tokio::spawn(async move {
            while let Some(req_result) = stream.next().await {
                match req_result {
                    Ok(req) => {
                        let response = BenchmarkResponse {
                            id: req.id,
                            timestamp: get_current_timestamp_us(),
                            payload: generate_payload(server.config.payload_size, &PayloadPattern::Random),
                            status: "streaming".to_string(),
                            metrics: Some(ServerMetrics {
                                concurrent_connections: server.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                                total_messages: server.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                                cpu_usage: server.get_cpu_usage(),
                                memory_usage: server.get_memory_usage(),
                            }),
                        };
                        
                        if tx.send(Ok(response)).await.is_err() {
                            warn!("Client disconnected from stream test");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error in stream test: {}", e);
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
        });
        
        Ok(ReceiverStream::new(rx))
    }
}

pub async fn run_server(address: String) -> Result<(), Box<dyn std::error::Error>> {
    let addr = address.parse()?;
    let server = BenchmarkServer::new();
    
    info!("Starting gRPC server on {}", address);
    
    // Metrics setup would go here in a full implementation
    
    Server::builder()
        .add_service(
            crate::proto::benchmark::benchmark_service_server::BenchmarkServiceServer::new(server)
        )
        .serve(addr)
        .await?;
    
    Ok(())
}