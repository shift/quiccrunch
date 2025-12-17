use quiccrunch_core::{
    rpc::{RpcServer, RpcService, StreamingRpcService, RpcContext, RpcResult, RpcStreamSink, RpcStreamReceiver, RpcError},
    transport::tquic::{TquicServer, TquicConnection},
    TlsConfig,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};
use tokio::sync::mpsc;

use crate::common::{
    SimpleRequest, SimpleResponse, BenchmarkRequest, BenchmarkResponse, ServerMetrics,
    get_current_timestamp_us, generate_payload, PayloadPattern
};
use crate::proto::{BenchmarkProto, BenchmarkMethods};

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

    async fn handle_unary_call(&self, _context: &RpcContext, params: bytes::Bytes) -> RpcResult<bytes::Bytes> {
        let request = BenchmarkProto::deserialize_simple_request(&params)?;
        info!("Received unary call: {}", request.id);
        
        let response = self.process_request(&request);
        BenchmarkProto::serialize_simple_response(&response)
    }

    async fn handle_client_streaming(
        &self,
        _context: &RpcContext,
        mut stream: RpcStreamReceiver,
    ) -> RpcResult<bytes::Bytes> {
        info!("Received client streaming request");
        
        let mut message_count = 0u64;
        let mut total_size = 0u64;
        
        while let Some(data_result) = stream.next().await {
            match data_result {
                Ok(data) => {
                    let request = BenchmarkProto::deserialize_simple_request(&data)?;
                    message_count += 1;
                    total_size += request.payload.len() as u64;
                    debug!("Processed client stream message: {}", request.id);
                }
                Err(e) => {
                    error!("Error in client stream: {}", e);
                    return Err(e);
                }
            }
        }
        
        let response = SimpleResponse {
            id: "client-streaming-response".to_string(),
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
        
        BenchmarkProto::serialize_simple_response(&response)
    }

    async fn handle_server_streaming(
        &self,
        _context: &RpcContext,
        params: bytes::Bytes,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        let request = BenchmarkProto::deserialize_simple_request(&params)?;
        info!("Received server streaming request: {}", request.id);
        
        let stream_count = 10; // Number of responses to send
        for i in 0..stream_count {
            let response_payload = generate_payload(self.config.payload_size, &PayloadPattern::Random);
            let response = SimpleResponse {
                id: format!("{}-{}", request.id, i),
                timestamp: get_current_timestamp_us(),
                payload: response_payload,
                status: "streaming".to_string(),
                metrics: Some(ServerMetrics {
                    concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                    total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                    cpu_usage: self.get_cpu_usage(),
                    memory_usage: self.get_memory_usage(),
                }),
            };
            
            let response_data = BenchmarkProto::serialize_simple_response(&response)?;
            if sink.send(response_data).await.is_err() {
                warn!("Client disconnected from server stream");
                break;
            }
            
            // Small delay between stream messages
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        sink.finish().await?;
        Ok(())
    }

    async fn handle_bidirectional_streaming(
        &self,
        _context: &RpcContext,
        mut stream: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        info!("Received bidirectional streaming request");
        
        while let Some(data_result) = stream.next().await {
            match data_result {
                Ok(data) => {
                    let request = BenchmarkProto::deserialize_simple_request(&data)?;
                    let response = self.process_request(&request);
                    let response_data = BenchmarkProto::serialize_simple_response(&response)?;
                    
                    if sink.send(response_data).await.is_err() {
                        warn!("Client disconnected from bidirectional stream");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error in bidirectional stream: {}", e);
                    break;
                }
            }
        }
        
        sink.finish().await?;
        Ok(())
    }

    async fn handle_latency_test(&self, _context: &RpcContext, params: bytes::Bytes) -> RpcResult<bytes::Bytes> {
        let request = BenchmarkProto::deserialize_benchmark_request(&params)?;
        info!("Received latency test request: {}", request.id);
        
        let response = BenchmarkResponse {
            id: request.id,
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
        
        BenchmarkProto::serialize_benchmark_response(&response)
    }

    async fn handle_flood_test(
        &self,
        _context: &RpcContext,
        mut stream: RpcStreamReceiver,
    ) -> RpcResult<bytes::Bytes> {
        info!("Received flood test request");
        
        let mut message_count = 0u64;
        
        while let Some(data_result) = stream.next().await {
            match data_result {
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
        
        BenchmarkProto::serialize_benchmark_response(&response)
    }

    async fn handle_stream_test(
        &self,
        _context: &RpcContext,
        mut stream: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        info!("Received stream test request");
        
        while let Some(data_result) = stream.next().await {
            match data_result {
                Ok(data) => {
                    let request = BenchmarkProto::deserialize_benchmark_request(&data)?;
                    let response = BenchmarkResponse {
                        id: request.id,
                        timestamp: get_current_timestamp_us(),
                        payload: generate_payload(self.config.payload_size, &PayloadPattern::Random),
                        status: "streaming".to_string(),
                        metrics: Some(ServerMetrics {
                            concurrent_connections: self.metrics.concurrent_connections.load(std::sync::atomic::Ordering::Relaxed),
                            total_messages: self.metrics.total_messages.load(std::sync::atomic::Ordering::Relaxed),
                            cpu_usage: self.get_cpu_usage(),
                            memory_usage: self.get_memory_usage(),
                        }),
                    };
                    
                    let response_data = BenchmarkProto::serialize_benchmark_response(&response)?;
                    if sink.send(response_data).await.is_err() {
                        warn!("Client disconnected from stream test");
                        break;
                    }
                }
                Err(e) => {
                    error!("Error in stream test: {}", e);
                    break;
                }
            }
        }
        
        sink.finish().await?;
        Ok(())
    }
}

// RPC Service implementation for unary calls
#[async_trait::async_trait]
impl RpcService for BenchmarkServer {
    async fn call(&self, context: &RpcContext, method: &str, params: bytes::Bytes) -> RpcResult<bytes::Bytes> {
        match method {
            BenchmarkMethods::UNARY_CALL => self.handle_unary_call(context, params).await,
            BenchmarkMethods::LATENCY_TEST => self.handle_latency_test(context, params).await,
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

// Streaming RPC Service implementation
#[async_trait::async_trait]
impl StreamingRpcService for BenchmarkServer {
    async fn server_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        params: bytes::Bytes,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            BenchmarkMethods::SERVER_STREAMING => self.handle_server_streaming(context, params, sink).await,
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }

    async fn client_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        stream: RpcStreamReceiver,
    ) -> RpcResult<bytes::Bytes> {
        match method {
            BenchmarkMethods::CLIENT_STREAMING => self.handle_client_streaming(context, stream).await,
            BenchmarkMethods::FLOOD_TEST => self.handle_flood_test(context, stream).await,
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }

    async fn bidirectional_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        stream: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            BenchmarkMethods::BIDIRECTIONAL_STREAMING => self.handle_bidirectional_streaming(context, stream, sink).await,
            BenchmarkMethods::STREAM_TEST => self.handle_stream_test(context, stream, sink).await,
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

pub async fn run_server(address: String) -> Result<(), Box<dyn std::error::Error>> {
    // Parse address to extract host and port
    let addr: std::net::SocketAddr = address.parse()?;
    let host = addr.ip().to_string();
    let port = addr.port();
    
    // Generate self-signed TLS certificate
    let tls_config = TlsConfig::generate_self_signed(&host)?;
    
    // Create QUICCRUNCH server
    let rpc_server = RpcServer::new();
    let benchmark_server = BenchmarkServer::new();
    
    // Register both regular and streaming services
    rpc_server.register_service("BenchmarkService".to_string(), Arc::new(benchmark_server.clone())).await;
    rpc_server.register_streaming_service("BenchmarkService".to_string(), Arc::new(benchmark_server)).await;
    
    info!("Starting QUICCRUNCH server on {}", address);
    
    // Create tQUIC server
    let tquic_server = TquicServer::new(addr, "cert.pem".to_string(), "key.pem".to_string(), Some(4)).await?;
    let mut listener = tquic_server.listen(&address).await?;
    
    info!("QUICCRUNCH server listening on {}", address);
    
    // Handle incoming connections
    while let Ok(connection) = listener.accept().await {
        let rpc_server = rpc_server.clone();
        tokio::spawn(async move {
            if let Err(e) = rpc_server.handle_connection(connection).await {
                error!("Connection handling error: {}", e);
            }
        });
    }
    
    Ok(())
}