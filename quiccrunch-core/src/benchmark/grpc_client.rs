// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! gRPC client implementation for all streaming patterns

#[cfg(feature = "grpc-comparison")]
pub mod grpc_impl {
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tokio::time::sleep;
    use tokio_stream::{wrappers::ReceiverStream, StreamExt};
    use tonic::transport::Channel;
    use tonic::{Request, Response, Status, Streaming};
    use crate::benchmark::{
        BenchmarkConfig, StreamingPattern, PerformanceMetrics,
        generate_test_message, generate_message_id
    };
    use crate::benchmark::metrics::{MetricsCollector, LatencyTimer, ErrorType};

    // Include generated protobuf code
    pub mod proto {
        tonic::include_proto!("benchmark");
    }

    use proto::{
        benchmark_service_client::BenchmarkServiceClient,
        SimpleRequest, SimpleResponse,
    };

    /// gRPC benchmark client
    pub struct GrpcClient {
        client: BenchmarkServiceClient<Channel>,
        metrics: Arc<MetricsCollector>,
    }

    impl GrpcClient {
        /// Create a new gRPC client
        pub async fn new(server_addr: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let channel = Channel::from_shared(format!("http://{}", server_addr))?
                .connect()
                .await?;
            
            let client = BenchmarkServiceClient::new(channel);
            let metrics = Arc::new(MetricsCollector::new()?);
            
            Ok(Self { client, metrics })
        }
        
        /// Run benchmark for a specific streaming pattern
        pub async fn run_benchmark(
            &mut self,
            config: &BenchmarkConfig,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            // Reset metrics for new test
            self.metrics.reset()?;
            
            // Warmup phase
            self.run_warmup(config).await?;
            
            // Main benchmark phase
            match &config.streaming_pattern {
                StreamingPattern::Unary => {
                    self.run_unary_benchmark(config).await
                }
                StreamingPattern::ClientStreaming { message_count } => {
                    self.run_client_streaming_benchmark(config, *message_count).await
                }
                StreamingPattern::ServerStreaming { message_count } => {
                    self.run_server_streaming_benchmark(config, *message_count).await
                }
                StreamingPattern::Bidirectional { message_count } => {
                    self.run_bidirectional_benchmark(config, *message_count).await
                }
            }
        }
        
        /// Warmup phase
        async fn run_warmup(
            &mut self,
            config: &BenchmarkConfig,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            println!("Running gRPC warmup phase...");
            
            // Send a few simple requests to warm up the connection
            for i in 0..5 {
                let request = SimpleRequest {
                    id: format!("warmup-{}", i),
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                    payload: vec![0u8; config.message_size],
                    test_type: "warmup".to_string(),
                    sequence_number: i,
                };
                
                if let Err(e) = self.client.unary_call(Request::new(request)).await {
                    println!("Warmup request failed: {}", e);
                    continue;
                }
            }
            
            sleep(Duration::from_millis(100)).await;
            println!("gRPC warmup completed");
            Ok(())
        }
        
        /// Run unary RPC benchmark
        async fn run_unary_benchmark(
            &mut self,
            config: &BenchmarkConfig,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            println!("Running gRPC unary benchmark...");
            
            let requests_per_connection = 100;
            let mut handles = Vec::new();
            
            for client_id in 0..config.concurrent_connections {
                let server_addr = "127.0.0.1:50053".to_string(); // Different port from QuicCrunch
                let message_size = config.message_size;
                let target_qps = config.target_qps;
                let metrics = self.metrics.clone();
                
                let handle = tokio::spawn(async move {
                    let channel = Channel::from_shared(format!("http://{}", server_addr))?
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    
                    for i in 0..requests_per_connection {
                        let request = SimpleRequest {
                            id: generate_message_id("grpc-unary", (client_id * requests_per_connection + i) as u32),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros() as u64,
                            payload: generate_test_message(message_size),
                            test_type: "unary".to_string(),
                            sequence_number: (client_id * requests_per_connection + i) as u32,
                        };
                        
                        let timer = LatencyTimer::new();
                        
                        match client.unary_call(Request::new(request)).await {
                            Ok(response) => {
                                let latency_micros = timer.elapsed_micros();
                                let _ = metrics.record_latency(latency_micros);
                                let response_size = response.into_inner().payload.len();
                                metrics.record_throughput(
                                    (message_size + response_size) as u64, 
                                    Duration::from_millis(1)
                                );
                            }
                            Err(_) => {
                                metrics.record_error(ErrorType::MessageFailure);
                            }
                        }
                        
                        // Rate limiting if specified
                        if let Some(target_qps) = target_qps {
                            let delay = Duration::from_millis(1000 / target_qps);
                            sleep(delay).await;
                        }
                    }
                    
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                });
                
                handles.push(handle);
            }
            
            // Wait for all clients to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    println!("gRPC client error: {:?}", e);
                }
            }
            
            self.metrics.update_resource_usage();
            Ok(self.metrics.generate_metrics())
        }
        
        /// Run client streaming benchmark
        async fn run_client_streaming_benchmark(
            &mut self,
            config: &BenchmarkConfig,
            message_count: usize,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            println!("Running gRPC client streaming benchmark with {} messages per stream...", message_count);
            
            let mut handles = Vec::new();
            
            for client_id in 0..config.concurrent_connections {
                let server_addr = "127.0.0.1:50053".to_string();
                let message_size = config.message_size;
                let metrics = self.metrics.clone();
                
                let handle = tokio::spawn(async move {
                    let channel = Channel::from_shared(format!("http://{}", server_addr))?
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    
                    let (tx, rx) = tokio::sync::mpsc::channel(message_count);
                    let request_stream = ReceiverStream::new(rx);
                    
                    let timer = LatencyTimer::new();
                    
                    // Start the streaming request
                    let response_future = client.client_streaming(Request::new(request_stream));
                    
                    // Send messages
                    for i in 0..message_count {
                        let request = SimpleRequest {
                            id: generate_message_id("grpc-client-stream", (client_id * message_count + i) as u32),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros() as u64,
                            payload: generate_test_message(message_size),
                            test_type: "client_streaming".to_string(),
                            sequence_number: i as u32,
                        };
                        
                        if tx.send(request).await.is_err() {
                            metrics.record_error(ErrorType::MessageFailure);
                            break;
                        }
                        
                        metrics.record_throughput(message_size as u64, Duration::from_millis(1));
                    }
                    
                    // Close the stream
                    drop(tx);
                    
                    // Wait for response
                    match response_future.await {
                        Ok(_response) => {
                            let latency_micros = timer.elapsed_micros();
                            let _ = metrics.record_latency(latency_micros);
                        }
                        Err(_) => {
                            metrics.record_error(ErrorType::MessageFailure);
                        }
                    }
                    
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                });
                
                handles.push(handle);
            }
            
            // Wait for all clients to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    println!("gRPC client error: {:?}", e);
                }
            }
            
            self.metrics.update_resource_usage();
            Ok(self.metrics.generate_metrics())
        }
        
        /// Run server streaming benchmark
        async fn run_server_streaming_benchmark(
            &mut self,
            config: &BenchmarkConfig,
            expected_message_count: usize,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            println!("Running gRPC server streaming benchmark expecting {} messages per stream...", expected_message_count);
            
            let mut handles = Vec::new();
            
            for client_id in 0..config.concurrent_connections {
                let server_addr = "127.0.0.1:50053".to_string();
                let message_size = config.message_size;
                let metrics = self.metrics.clone();
                
                let handle = tokio::spawn(async move {
                    let channel = Channel::from_shared(format!("http://{}", server_addr))?
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    
                    let request = SimpleRequest {
                        id: generate_message_id("grpc-server-stream", client_id as u32),
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_micros() as u64,
                        payload: generate_test_message(message_size),
                        test_type: "server_streaming".to_string(),
                        sequence_number: client_id as u32,
                    };
                    
                    let timer = LatencyTimer::new();
                    
                    match client.server_streaming(Request::new(request)).await {
                        Ok(response) => {
                            let mut stream = response.into_inner();
                            let mut received_count = 0;
                            
                            while let Some(response_result) = stream.next().await {
                                match response_result {
                                    Ok(response) => {
                                        received_count += 1;
                                        metrics.record_throughput(
                                            response.payload.len() as u64, 
                                            Duration::from_millis(1)
                                        );
                                    }
                                    Err(_) => {
                                        metrics.record_error(ErrorType::MessageFailure);
                                        break;
                                    }
                                }
                            }
                            
                            let latency_micros = timer.elapsed_micros();
                            let _ = metrics.record_latency(latency_micros);
                        }
                        Err(_) => {
                            metrics.record_error(ErrorType::MessageFailure);
                        }
                    }
                    
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                });
                
                handles.push(handle);
            }
            
            // Wait for all clients to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    println!("gRPC client error: {:?}", e);
                }
            }
            
            self.metrics.update_resource_usage();
            Ok(self.metrics.generate_metrics())
        }
        
        /// Run bidirectional streaming benchmark
        async fn run_bidirectional_benchmark(
            &mut self,
            config: &BenchmarkConfig,
            message_count: usize,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            println!("Running gRPC bidirectional streaming benchmark with {} messages each way...", message_count);
            
            let mut handles = Vec::new();
            
            for client_id in 0..config.concurrent_connections {
                let server_addr = "127.0.0.1:50053".to_string();
                let message_size = config.message_size;
                let metrics = self.metrics.clone();
                
                let handle = tokio::spawn(async move {
                    let channel = Channel::from_shared(format!("http://{}", server_addr))?
                        .connect()
                        .await?;
                    let mut client = BenchmarkServiceClient::new(channel);
                    
                    let (tx, rx) = tokio::sync::mpsc::channel(message_count);
                    let request_stream = ReceiverStream::new(rx);
                    
                    let timer = LatencyTimer::new();
                    
                    // Separate the response future and sending
                    let (send_tx, send_rx) = tokio::sync::oneshot::channel();
                    
                    // Start the request in a separate task to handle lifetime issues
                    let client_clone = client.clone();
                    let response_task = tokio::spawn(async move {
                        client_clone.bidirectional_streaming(Request::new(request_stream)).await
                    });
                    
                    // Send messages
                    for i in 0..message_count {
                        let request = SimpleRequest {
                            id: generate_message_id("grpc-bidir", (client_id * message_count + i) as u32),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_micros() as u64,
                            payload: generate_test_message(message_size),
                            test_type: "bidirectional".to_string(),
                            sequence_number: i as u32,
                        };
                        
                        if tx.send(request).await.is_err() {
                            metrics.record_error(ErrorType::MessageFailure);
                            break;
                        }
                        
                        metrics.record_throughput(message_size as u64, Duration::from_millis(1));
                        sleep(Duration::from_millis(1)).await;
                    }
                    drop(tx);
                    
                    // Wait for response and process stream
                    match response_task.await {
                        Ok(Ok(response)) => {
                            let mut stream = response.into_inner();
                            
                            while let Some(response_result) = stream.next().await {
                                match response_result {
                                    Ok(response) => {
                                        metrics.record_throughput(
                                            response.payload.len() as u64, 
                                            Duration::from_millis(1)
                                        );
                                    }
                                    Err(_) => {
                                        metrics.record_error(ErrorType::MessageFailure);
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {
                            metrics.record_error(ErrorType::MessageFailure);
                        }
                    }
                    
                    let latency_micros = timer.elapsed_micros();
                    let _ = metrics.record_latency(latency_micros);
                    
                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
                });
                
                handles.push(handle);
            }
            
            // Wait for all clients to complete
            for handle in handles {
                if let Err(e) = handle.await {
                    println!("gRPC client error: {:?}", e);
                }
            }
            
            self.metrics.update_resource_usage();
            Ok(self.metrics.generate_metrics())
        }
    }
}

// Re-export for when gRPC feature is not enabled
#[cfg(not(feature = "grpc-comparison"))]
pub mod grpc_impl {
    use crate::benchmark::{BenchmarkConfig, PerformanceMetrics};
    
    pub struct GrpcClient;
    
    impl GrpcClient {
        pub async fn new(_server_addr: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            Err("gRPC comparison feature not enabled".into())
        }
        
        pub async fn run_benchmark(
            &mut self,
            _config: &BenchmarkConfig,
        ) -> Result<PerformanceMetrics, Box<dyn std::error::Error + Send + Sync>> {
            Err("gRPC comparison feature not enabled".into())
        }
    }
}

pub use grpc_impl::*;