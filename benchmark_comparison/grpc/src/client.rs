use tonic::{transport::Channel, Request, Response, Status, Streaming};
use tokio_stream::{StreamExt, Stream};
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};

use crate::proto::benchmark::{
    benchmark_service_client::BenchmarkServiceClient,
    SimpleRequest, SimpleResponse, BenchmarkRequest, BenchmarkResponse,
};
use crate::common::{
    BenchmarkConfig, MetricsCollector, ErrorType, 
    generate_payload, get_current_timestamp_us, PayloadPattern
};

pub struct BenchmarkClient {
    client: BenchmarkServiceClient<Channel>,
    config: BenchmarkConfig,
    metrics: MetricsCollector,
}

impl BenchmarkClient {
    pub async fn new(config: BenchmarkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let client = BenchmarkServiceClient::connect(config.server_address.clone()).await?;
        
        Ok(Self {
            client,
            config,
            metrics: MetricsCollector::new(),
        })
    }

    pub async fn run_unary_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting unary test with {} messages", self.config.num_messages);
        
        for i in 0..self.config.num_messages {
            let request_id = format!("unary-{}", i);
            let payload = generate_payload(self.config.message_size, &self.config.payload_pattern);
            
            let request = SimpleRequest {
                id: request_id,
                timestamp: get_current_timestamp_us(),
                payload,
                test_type: "unary".to_string(),
                sequence_number: i as u32,
            };
            
            let start_time = Instant::now();
            
            match self.client.unary_call(Request::new(request)).await {
                Ok(response) => {
                    let latency = start_time.elapsed();
                    let response = response.into_inner();
                    
                    self.metrics.record_sent(self.config.message_size as u64);
                    self.metrics.record_received(response.payload.len() as u64, latency);
                    
                    debug!("Unary response received: {} in {:?}", response.id, latency);
                }
                Err(e) => {
                    self.metrics.record_error(ErrorType::Protocol);
                    error!("Unary call failed: {}", e);
                }
            }
        }
        
        let metrics = self.metrics.calculate_metrics("unary".to_string());
        self.print_metrics(&metrics);
        
        Ok(())
    }

    pub async fn run_client_streaming_test(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting client streaming test with {} messages", self.config.num_messages);
        
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        
        // Spawn task to send messages
        let sender_task = tokio::spawn(async move {
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
                
                if tx.send(Ok(request)).await.is_err() {
                    warn!("Failed to send message to client stream");
                    break;
                }
                
                // Small delay between messages
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
        
        let start_time = Instant::now();
        let payload_size = self.config.message_size as u64;
        
        match self.client.client_streaming(Request::new(tokio_stream::wrappers::ReceiverStream::new(rx))).await {
            Ok(response) => {
                let latency = start_time.elapsed();
                let response = response.into_inner();
                
                self.metrics.record_sent(payload_size * self.config.num_messages as u64);
                self.metrics.record_received(response.payload.len() as u64, latency);
                
                info!("Client streaming completed in {:?}", latency);
            }
            Err(e) => {
                self.metrics.record_error(ErrorType::Protocol);
                error!("Client streaming failed: {}", e);
            }
        }
        
        sender_task.await?;
        
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
        
        match self.client.server_streaming(Request::new(request)).await {
            Ok(mut response) => {
                while let Some(result) = response.message().await {
                    match result {
                        Ok(response_msg) => {
                            message_count += 1;
                            total_bytes_received += response_msg.payload.len() as u64;
                            debug!("Server stream message received: {}", response_msg.id);
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
        info!("Starting bidirectional streaming test with {} messages", self.config.num_messages);
        
        let (tx, rx) = tokio::sync::mpsc::channel(128);
        
        // Spawn task to send messages and collect responses
        let client = self.client.clone();
        let message_count = self.config.num_messages;
        let message_size = self.config.message_size;
        let payload_pattern = self.config.payload_pattern.clone();
        
        let stream_task = tokio::spawn(async move {
            let response_stream = match client.bidirectional_streaming(Request::new(tokio_stream::wrappers::ReceiverStream::new(rx))).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Failed to start bidirectional stream: {}", e);
                    return None;
                }
            };
            
            Some(response_stream)
        });
        
        // Send messages
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
            
            if tx.send(Ok(request)).await.is_err() {
                warn!("Failed to send message to bidirectional stream");
                break;
            }
            
            // Small delay between messages
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        drop(tx); // Close the sender to signal end of stream
        
        // Collect responses
        if let Some(Ok(mut response_stream)) = stream_task.await? {
            let mut message_count = 0u64;
            let mut total_bytes_received = 0u64;
            
            while let Some(result) = response_stream.message().await {
                match result {
                    Ok(response_msg) => {
                        message_count += 1;
                        total_bytes_received += response_msg.payload.len() as u64;
                        debug!("Bidirectional stream message received: {}", response_msg.id);
                    }
                    Err(e) => {
                        self.metrics.record_error(ErrorType::Protocol);
                        error!("Error in bidirectional stream: {}", e);
                        break;
                    }
                }
            }
            
            self.metrics.record_sent((message_size * message_count) as u64);
            self.metrics.record_received(total_bytes_received, Duration::from_millis(100)); // Approximate latency
            
            info!("Bidirectional streaming completed: {} messages", message_count);
        }
        
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
            
            match self.client.latency_test(Request::new(request)).await {
                Ok(response) => {
                    let latency = start_time.elapsed();
                    let response = response.into_inner();
                    
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
        
        let (tx, rx) = tokio::sync::mpsc::channel(512);
        
        // Spawn task to send messages rapidly
        let sender_task = tokio::spawn(async move {
            for i in 0..self.config.num_messages {
                let request_id = format!("flood-{}", i);
                let payload = generate_payload(self.config.message_size, &PayloadPattern::Random);
                
                let request = BenchmarkRequest {
                    id: request_id,
                    timestamp: get_current_timestamp_us(),
                    payload,
                    test_type: "flood".to_string(),
                    sequence_number: i as u32,
                };
                
                if tx.send(Ok(request)).await.is_err() {
                    warn!("Failed to send message to flood stream");
                    break;
                }
            }
        });
        
        let start_time = Instant::now();
        
        match self.client.flood_test(Request::new(tokio_stream::wrappers::ReceiverStream::new(rx))).await {
            Ok(response) => {
                let latency = start_time.elapsed();
                let response = response.into_inner();
                
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