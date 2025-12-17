use quiccrunch_core::{
    rpc::{RpcClient, RpcStreamReceiver},
    transport::tquic::{TquicClient, TquicConnection},
};
use std::time::{Duration, Instant};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{info, warn, error, debug};

use crate::common::{
    BenchmarkConfig, MetricsCollector, ErrorType, 
    SimpleRequest, SimpleResponse, BenchmarkRequest, BenchmarkResponse,
    generate_payload, get_current_timestamp_us, PayloadPattern
};
use crate::proto::{BenchmarkProto, BenchmarkMethods};

pub struct BenchmarkClient {
    clients: Vec<RpcClient>,
    config: BenchmarkConfig,
    metrics: MetricsCollector,
    semaphore: Arc<Semaphore>,
}

impl BenchmarkClient {
    pub async fn new(config: BenchmarkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let mut clients = Vec::new();
        
        // Create connection pool with multiple clients for parallelism
        for _ in 0..config.concurrent_connections {
            let tquic_client = TquicClient::new(Some(2)).await?;
            let connection = tquic_client.connect(&config.server_address).await?;
            let client = RpcClient::new(connection);
            clients.push(client);
        }
        
        // Create semaphore to limit concurrent operations
        let semaphore = Arc::new(Semaphore::new(config.concurrent_connections));
        
        Ok(Self {
            clients,
            config,
            metrics: MetricsCollector::new(),
            semaphore,
        })
    }

    pub async fn run_unary_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting parallel unary test with {} messages across {} connections", 
              self.config.num_messages, self.config.concurrent_connections);
        
        let start_time = Instant::now();
        let mut tasks = Vec::new();
        
        // Create parallel tasks for maximum throughput
        for i in 0..self.config.num_messages {
            let client_idx = i % self.clients.len();
            let client = &self.clients[client_idx];
            let semaphore = Arc::clone(&self.semaphore);
            let message_size = self.config.message_size;
            let payload_pattern = self.config.payload_pattern.clone();
            
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                
                let request_id = format!("unary-{}", i);
                let payload = generate_payload(message_size, &payload_pattern);
                
                let request = SimpleRequest {
                    id: request_id,
                    timestamp: get_current_timestamp_us(),
                    payload,
                    test_type: "unary".to_string(),
                    sequence_number: i as u32,
                };
                
                let request_start = Instant::now();
                let request_data = match BenchmarkProto::serialize_simple_request(&request) {
                    Ok(data) => data,
                    Err(e) => {
                        return (None, Some(ErrorType::Protocol), request_start.elapsed());
                    }
                };
                
                match client.call("BenchmarkService", BenchmarkMethods::UNARY_CALL, request_data).await {
                    Ok(response_data) => {
                        let latency = request_start.elapsed();
                        match BenchmarkProto::deserialize_simple_response(&response_data) {
                            Ok(response) => {
                                (Some(response.payload.len()), None, latency)
                            }
                            Err(_) => {
                                (None, Some(ErrorType::Protocol), latency)
                            }
                        }
                    }
                    Err(_) => {
                        (None, Some(ErrorType::Protocol), request_start.elapsed())
                    }
                }
            });
            
            tasks.push(task);
        }
        
        // Collect results with pipelining
        for task in tasks {
            match task.await {
                Ok((Some(bytes_received), None, latency)) => {
                    self.metrics.record_sent(self.config.message_size as u64);
                    self.metrics.record_received(bytes_received as u64, latency);
                }
                Ok((_, Some(error_type), latency)) => {
                    self.metrics.record_error(error_type);
                    // Still record sent bytes even if failed
                    self.metrics.record_sent(self.config.message_size as u64);
                }
                Err(e) => {
                    self.metrics.record_error(ErrorType::Other);
                    error!("Task failed: {}", e);
                }
            }
        }
        
        let total_time = start_time.elapsed();
        info!("Parallel unary test completed in {:?}", total_time);
        
        let metrics = self.metrics.calculate_metrics("unary".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_client_streaming_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting high-performance client streaming test with {} messages", self.config.num_messages);
        
        let start_time = Instant::now();
        let payload_size = self.config.message_size as u64;
        
        // Use first client for streaming
        let client = &self.clients[0];
        
        // Call client streaming method
        match client.client_streaming_call("BenchmarkService", BenchmarkMethods::CLIENT_STREAMING).await {
            Ok((sink, response_future)) => {
                // Batch send messages for maximum throughput
                const BATCH_SIZE: usize = 100;
                let mut batch = Vec::with_capacity(BATCH_SIZE);
                
                for i in 0..self.config.num_messages {
                    let request_id = format!("client-stream-{}", i);
                    let payload = generate_payload(self.config.message_size, &self.config.payload_pattern);
                    
                    let request = SimpleRequest {
                        id: request_id,
                        timestamp: get_current_timestamp_us(),
                        payload,
                        test_type: "client_streaming".to_string(),
                        sequence_number: i as u32,
                    };
                    
                    match BenchmarkProto::serialize_simple_request(&request) {
                        Ok(request_data) => batch.push(request_data),
                        Err(e) => {
                            warn!("Failed to serialize request {}: {}", i, e);
                            continue;
                        }
                    }
                    
                    // Send batch when full or at the end
                    if batch.len() == BATCH_SIZE || i == self.config.num_messages - 1 {
                        for request_data in batch.drain(..) {
                            if sink.send(request_data).await.is_err() {
                                warn!("Failed to send message to client stream");
                                break;
                            }
                        }
                    }
                }
                
                // Signal end of stream
                if let Err(e) = sink.finish().await {
                    warn!("Failed to finish client stream: {}", e);
                }
                
                // Wait for response
                match response_future.await {
                    Ok(response_data) => {
                        let latency = start_time.elapsed();
                        let response = BenchmarkProto::deserialize_simple_response(&response_data)?;
                        
                        self.metrics.record_sent(payload_size * self.config.num_messages as u64);
                        self.metrics.record_received(response.payload.len() as u64, latency);
                        
                        info!("High-performance client streaming completed in {:?}", latency);
                    }
                    Err(e) => {
                        self.metrics.record_error(ErrorType::Protocol);
                        error!("Client streaming failed: {}", e);
                    }
                }
            }
            Err(e) => {
                self.metrics.record_error(ErrorType::Protocol);
                error!("Client streaming failed: {}", e);
            }
        }
        
        let metrics = self.metrics.calculate_metrics("client_streaming".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_server_streaming_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting server streaming test");
        
        let request_id = "server-stream-test".to_string();
        let payload = generate_payload(self.config.message_size, &self.config.payload_pattern);
        
        let request = SimpleRequest {
            id: request_id,
            timestamp: get_current_timestamp_us(),
            payload,
            test_type: "server_streaming".to_string(),
            sequence_number: 0,
        };
        
        let start_time = Instant::now();
        let mut message_count = 0u64;
        let mut total_bytes_received = 0u64;
        
        let request_data = BenchmarkProto::serialize_simple_request(&request)?;
        match self.client.server_streaming_call("BenchmarkService", BenchmarkMethods::SERVER_STREAMING, request_data).await {
            Ok(mut stream) => {
                while let Some(data_result) = stream.next().await {
                    match data_result {
                        Ok(response_data) => {
                            let response = BenchmarkProto::deserialize_simple_response(&response_data)?;
                            message_count += 1;
                            total_bytes_received += response.payload.len() as u64;
                            debug!("Server stream message received: {}", response.id);
                        }
                        Err(e) => {
                            self.metrics.record_error(ErrorType::Protocol);
                            error!("Error in server stream: {}", e);
                            break;
                        }
                    }
                }
                
                let total_latency = start_time.elapsed();
                self.metrics.record_sent(self.config.message_size as u64);
                self.metrics.record_received(total_bytes_received, total_latency);
                
                info!("Server streaming completed: {} messages in {:?}", message_count, total_latency);
            }
            Err(e) => {
                self.metrics.record_error(ErrorType::Protocol);
                error!("Server streaming failed: {}", e);
            }
        }
        
        let metrics = self.metrics.calculate_metrics("server_streaming".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_bidirectional_streaming_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting high-performance bidirectional streaming test with {} messages", self.config.num_messages);
        
        let start_time = Instant::now();
        
        // Use first client for bidirectional streaming
        let client = &self.clients[0];
        
        // Start bidirectional streaming call
        let stream_result = client.bidirectional_streaming_call(
            "BenchmarkService", 
            BenchmarkMethods::BIDIRECTIONAL_STREAMING
        ).await;
        
        let (sink, mut stream) = match stream_result {
            Ok((sink, stream)) => (sink, stream),
            Err(e) => {
                self.metrics.record_error(ErrorType::Protocol);
                error!("Failed to start bidirectional stream: {}", e);
                return Ok(());
            }
        };
        
        // Spawn high-performance sender task with batching
        let message_count = self.config.num_messages;
        let message_size = self.config.message_size;
        let payload_pattern = self.config.payload_pattern.clone();
        
        let sender_task = tokio::spawn(async move {
            const BATCH_SIZE: usize = 50;
            let mut batch = Vec::with_capacity(BATCH_SIZE);
            
            for i in 0..message_count {
                let request_id = format!("bidi-stream-{}", i);
                let payload = generate_payload(message_size, &payload_pattern);
                
                let request = SimpleRequest {
                    id: request_id,
                    timestamp: get_current_timestamp_us(),
                    payload,
                    test_type: "bidirectional_streaming".to_string(),
                    sequence_number: i as u32,
                };
                
                match BenchmarkProto::serialize_simple_request(&request) {
                    Ok(data) => batch.push(data),
                    Err(e) => {
                        warn!("Failed to serialize request {}: {}", i, e);
                        continue;
                    }
                }
                
                // Send batch when full or at the end
                if batch.len() == BATCH_SIZE || i == message_count - 1 {
                    for request_data in batch.drain(..) {
                        if sink.send(request_data).await.is_err() {
                            warn!("Failed to send message to bidirectional stream");
                            return;
                        }
                    }
                }
            }
            
            if let Err(e) = sink.finish().await {
                warn!("Failed to finish bidirectional stream: {}", e);
            }
        });
        
        // Collect responses with pipelining
        let mut response_count = 0u64;
        let mut total_bytes_received = 0u64;
        let mut response_latencies = Vec::new();
        
        while let Some(data_result) = stream.next().await {
            match data_result {
                Ok(response_data) => {
                    let response_start = Instant::now();
                    match BenchmarkProto::deserialize_simple_response(&response_data) {
                        Ok(response) => {
                            response_count += 1;
                            total_bytes_received += response.payload.len() as u64;
                            response_latencies.push(response_start.elapsed());
                            debug!("Bidirectional stream message received: {}", response.id);
                        }
                        Err(e) => {
                            self.metrics.record_error(ErrorType::Protocol);
                            error!("Failed to deserialize response: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.metrics.record_error(ErrorType::Protocol);
                    error!("Error in bidirectional stream: {}", e);
                    break;
                }
            }
        }
        
        sender_task.await?;
        
        let total_latency = start_time.elapsed();
        
        // Record metrics with proper latency measurement
        self.metrics.record_sent((self.config.message_size * response_count) as u64);
        self.metrics.record_received(total_bytes_received, total_latency);
        
        // Add individual response latencies for more accurate metrics
        for latency in response_latencies {
            self.metrics.record_received(0, latency);
        }
        
        info!("High-performance bidirectional streaming completed: {} messages in {:?}", response_count, total_latency);
        
        let metrics = self.metrics.calculate_metrics("bidirectional_streaming".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_latency_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting latency test with {} messages", self.config.num_messages);
        
        for i in 0..self.config.num_messages {
            let request_id = format!("latency-{}", i);
            let payload = generate_payload(self.config.message_size, &self.config.payload_pattern);
            
            let request = BenchmarkRequest {
                id: request_id,
                timestamp: get_current_timestamp_us(),
                payload,
                test_type: "latency".to_string(),
                sequence_number: i as u32,
            };
            
            let start_time = Instant::now();
            
            let request_data = BenchmarkProto::serialize_benchmark_request(&request)?;
            match self.client.call("BenchmarkService", BenchmarkMethods::LATENCY_TEST, request_data).await {
                Ok(response_data) => {
                    let latency = start_time.elapsed();
                    let response = BenchmarkProto::deserialize_benchmark_response(&response_data)?;
                    
                    self.metrics.record_sent(self.config.message_size as u64);
                    self.metrics.record_received(response.payload.len() as u64, latency);
                    
                    debug!("Latency test response received: {} in {:?}", response.id, latency);
                }
                Err(e) => {
                    self.metrics.record_error(ErrorType::Protocol);
                    error!("Latency test failed: {}", e);
                }
            }
        }
        
        let metrics = self.metrics.calculate_metrics("latency".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_flood_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting flood test with {} messages", self.config.num_messages);
        
        // Call flood test method
        match self.client.client_streaming_call("BenchmarkService", BenchmarkMethods::FLOOD_TEST).await {
            Ok((sink, response_future)) => {
                // Spawn task to send messages rapidly
                let message_count = self.config.num_messages;
                let message_size = self.config.message_size;
                
                let sender_task = tokio::spawn(async move {
                    for i in 0..message_count {
                        let request_id = format!("flood-{}", i);
                        let payload = generate_payload(message_size, &PayloadPattern::Random);
                        
                        let request = BenchmarkRequest {
                            id: request_id,
                            timestamp: get_current_timestamp_us(),
                            payload,
                            test_type: "flood".to_string(),
                            sequence_number: i as u32,
                        };
                        
                        let request_data = match BenchmarkProto::serialize_benchmark_request(&request) {
                            Ok(data) => data,
                            Err(e) => {
                                warn!("Failed to serialize request: {}", e);
                                break;
                            }
                        };
                        
                        if sink.send(request_data).await.is_err() {
                            warn!("Failed to send message to flood stream");
                            break;
                        }
                    }
                    
                    if let Err(e) = sink.finish().await {
                        warn!("Failed to finish flood stream: {}", e);
                    }
                });
                
                let start_time = Instant::now();
                
                // Wait for response
                match response_future.await {
                    Ok(response_data) => {
                        let latency = start_time.elapsed();
                        let response = BenchmarkProto::deserialize_benchmark_response(&response_data)?;
                        
                        self.metrics.record_sent((self.config.message_size * self.config.num_messages) as u64);
                        self.metrics.record_received(response.payload.len() as u64, latency);
                        
                        info!("Flood test completed in {:?}", latency);
                    }
                    Err(e) => {
                        self.metrics.record_error(ErrorType::Protocol);
                        error!("Flood test failed: {}", e);
                    }
                }
                
                sender_task.await?;
            }
            Err(e) => {
                self.metrics.record_error(ErrorType::Protocol);
                error!("Flood test failed: {}", e);
            }
        }
        
        let metrics = self.metrics.calculate_metrics("flood".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    fn print_metrics(&self, metrics: &crate::common::BenchmarkMetrics) {
        println!("\n=== {} Test Results ===", metrics.test_type);
        println!("Test ID: {}", metrics.test_id);
        println!("Duration: {} seconds", metrics.end_time - metrics.start_time);
        println!("Messages: {} sent, {} received, {} failed", 
                metrics.total_messages, metrics.successful_messages, metrics.failed_messages);
        println!("Data: {} MB sent, {} MB received", 
                metrics.bytes_sent / (1024 * 1024), 
                metrics.bytes_received / (1024 * 1024));
        
        println!("\nLatency:");
        println!("  Min: {} μs", metrics.latency_stats.min_us);
        println!("  Max: {} μs", metrics.latency_stats.max_us);
        println!("  Mean: {:.2} μs", metrics.latency_stats.mean_us);
        println!("  P50: {} μs", metrics.latency_stats.p50_us);
        println!("  P95: {} μs", metrics.latency_stats.p95_us);
        println!("  P99: {} μs", metrics.latency_stats.p99_us);
        println!("  P99.9: {} μs", metrics.latency_stats.p999_us);
        
        println!("\nThroughput:");
        println!("  {:.2} messages/sec", metrics.throughput_stats.messages_per_second);
        println!("  {:.2} MB/sec", metrics.throughput_stats.bytes_per_second / (1024.0 * 1024.0));
        println!("  {:.2} Mbps", metrics.throughput_stats.megabits_per_second);
        
        if metrics.error_stats.connection_errors > 0 || 
           metrics.error_stats.timeout_errors > 0 || 
           metrics.error_stats.protocol_errors > 0 || 
           metrics.error_stats.other_errors > 0 {
            println!("\nErrors:");
            println!("  Connection: {}", metrics.error_stats.connection_errors);
            println!("  Timeout: {}", metrics.error_stats.timeout_errors);
            println!("  Protocol: {}", metrics.error_stats.protocol_errors);
            println!("  Other: {}", metrics.error_stats.other_errors);
        }
        println!();
    }
}