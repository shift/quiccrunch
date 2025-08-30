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

//! gRPC server implementation for all streaming patterns

#[cfg(feature = "grpc-comparison")]
pub mod grpc_impl {
    use std::pin::Pin;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use futures::Stream;
    use tokio::sync::{oneshot, RwLock};
    use tokio_stream::{wrappers::ReceiverStream, StreamExt};
    use tonic::transport::Server;
    use tonic::{Request, Response, Status, Streaming};

    // Include generated protobuf code
    pub mod proto {
        tonic::include_proto!("benchmark");
    }

    use proto::{
        benchmark_service_server::{BenchmarkService, BenchmarkServiceServer},
        SimpleRequest, SimpleResponse,
    };

    /// Server statistics tracking
    #[derive(Debug, Default, Clone)]
    struct ServerStats {
        total_requests: u64,
        total_responses: u64,
        active_connections: u64,
        error_count: u64,
    }

    /// gRPC benchmark service implementation
    #[derive(Clone)]
    pub struct GrpcBenchmarkService {
        stats: Arc<RwLock<ServerStats>>,
    }

    impl GrpcBenchmarkService {
        pub fn new() -> Self {
            Self {
                stats: Arc::new(RwLock::new(ServerStats::default())),
            }
        }

        pub async fn get_stats(&self) -> ServerStats {
            self.stats.read().await.clone()
        }
    }

    #[tonic::async_trait]
    impl BenchmarkService for GrpcBenchmarkService {
        /// Unary RPC - single request, single response
        async fn unary_call(
            &self,
            request: Request<SimpleRequest>,
        ) -> Result<Response<SimpleResponse>, Status> {
            let req = request.into_inner();
            
            {
                let mut stats = self.stats.write().await;
                stats.total_requests += 1;
            }
            
            let response = SimpleResponse {
                id: format!("resp-{}", req.id),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: req.payload, // Echo back payload
                status: "ok".to_string(),
                metrics: None,
            };
            
            {
                let mut stats = self.stats.write().await;
                stats.total_responses += 1;
            }

            Ok(Response::new(response))
        }

        /// Client streaming RPC - multiple requests, single response
        async fn client_streaming(
            &self,
            request: Request<Streaming<SimpleRequest>>,
        ) -> Result<Response<SimpleResponse>, Status> {
            let mut stream = request.into_inner();
            let mut message_count = 0;
            let mut total_bytes = 0;
            
            // Receive all messages from client
            while let Some(request_result) = stream.next().await {
                match request_result {
                    Ok(req) => {
                        message_count += 1;
                        total_bytes += req.payload.len();
                        
                        let mut stats = self.stats.write().await;
                        stats.total_requests += 1;
                    }
                    Err(e) => {
                        let mut stats = self.stats.write().await;
                        stats.error_count += 1;
                        return Err(Status::internal(format!("Stream error: {}", e)));
                    }
                }
            }
            
            // Send aggregated response
            let response = SimpleResponse {
                id: format!("client_stream_resp_{}", message_count),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: vec![0u8; 100], // Small response payload
                status: format!("received {} messages, {} bytes", message_count, total_bytes),
                metrics: None,
            };
            
            {
                let mut stats = self.stats.write().await;
                stats.total_responses += 1;
            }

            Ok(Response::new(response))
        }

        /// Server streaming RPC - single request, multiple responses
        async fn server_streaming(
            &self,
            request: Request<SimpleRequest>,
        ) -> Result<Response<Self::ServerStreamingStream>, Status> {
            let req = request.into_inner();
            
            {
                let mut stats = self.stats.write().await;
                stats.total_requests += 1;
            }
            
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let stats = self.stats.clone();
            
            tokio::spawn(async move {
                // Send multiple responses (default 10)
                let response_count = 10;
                
                for i in 0..response_count {
                    let response = SimpleResponse {
                        id: format!("server_resp_{}_{}", req.id, i),
                        timestamp: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_micros() as u64,
                        payload: req.payload.clone(),
                        status: format!("response {}/{}", i + 1, response_count),
                        metrics: None,
                    };
                    
                    if tx.send(Ok(response)).await.is_err() {
                        break;
                    }
                    
                    // Small delay between responses
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                
                {
                    let mut stats_lock = stats.write().await;
                    stats_lock.total_responses += response_count;
                }
            });

            let output_stream = ReceiverStream::new(rx);
            Ok(Response::new(Box::pin(output_stream) as Self::ServerStreamingStream))
        }

        /// Bidirectional streaming RPC - multiple requests, multiple responses
        async fn bidirectional_streaming(
            &self,
            request: Request<Streaming<SimpleRequest>>,
        ) -> Result<Response<Self::BidirectionalStreamingStream>, Status> {
            let mut input_stream = request.into_inner();
            let (tx, rx) = tokio::sync::mpsc::channel(16);
            let stats = self.stats.clone();
            
            tokio::spawn(async move {
                let mut message_count = 0;
                
                // Process each incoming message and send responses
                while let Some(request_result) = input_stream.next().await {
                    match request_result {
                        Ok(req) => {
                            message_count += 1;
                            
                            {
                                let mut stats_lock = stats.write().await;
                                stats_lock.total_requests += 1;
                            }
                            
                            // Send response for each request
                            let response = SimpleResponse {
                                id: format!("bidir_resp_{}", req.id),
                                timestamp: SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap()
                                    .as_micros() as u64,
                                payload: vec![42u8; 100], // Small response payload
                                status: format!("echo for request {}", req.sequence_number),
                                metrics: None,
                            };
                            
                            if tx.send(Ok(response)).await.is_err() {
                                break;
                            }
                            
                            {
                                let mut stats_lock = stats.write().await;
                                stats_lock.total_responses += 1;
                            }
                            
                            // Small delay to avoid overwhelming
                            tokio::time::sleep(Duration::from_millis(1)).await;
                        }
                        Err(e) => {
                            let mut stats_lock = stats.write().await;
                            stats_lock.error_count += 1;
                            
                            let _ = tx.send(Err(Status::internal(format!("Stream error: {}", e)))).await;
                            break;
                        }
                    }
                }
            });

            let output_stream = ReceiverStream::new(rx);
            Ok(Response::new(Box::pin(output_stream) as Self::BidirectionalStreamingStream))
        }

        type ServerStreamingStream = Pin<Box<dyn Stream<Item = Result<SimpleResponse, Status>> + Send>>;
        type BidirectionalStreamingStream = Pin<Box<dyn Stream<Item = Result<SimpleResponse, Status>> + Send>>;
        
        // Legacy methods for backward compatibility
        async fn latency_test(
            &self,
            _request: Request<proto::BenchmarkRequest>,
        ) -> Result<Response<proto::BenchmarkResponse>, Status> {
            Err(Status::unimplemented("Use unary_call instead"))
        }

        async fn flood_test(
            &self,
            _request: Request<Streaming<proto::BenchmarkRequest>>,
        ) -> Result<Response<proto::BenchmarkResponse>, Status> {
            Err(Status::unimplemented("Use client_streaming instead"))
        }

        async fn stream_test(
            &self,
            _request: Request<Streaming<proto::BenchmarkRequest>>,
        ) -> Result<Response<Self::StreamTestStream>, Status> {
            Err(Status::unimplemented("Use bidirectional_streaming instead"))
        }

        type StreamTestStream = Pin<Box<dyn Stream<Item = Result<proto::BenchmarkResponse, Status>> + Send>>;
    }

    /// gRPC benchmark server
    pub struct GrpcServer {
        service: GrpcBenchmarkService,
        shutdown_tx: Option<oneshot::Sender<()>>,
    }

    impl GrpcServer {
        /// Create a new gRPC server
        pub fn new() -> Self {
            Self {
                service: GrpcBenchmarkService::new(),
                shutdown_tx: None,
            }
        }

        /// Start the server
        pub async fn start(
            &mut self,
            bind_addr: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            println!("Starting gRPC server on {}", bind_addr);
            
            let addr = bind_addr.parse()?;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            self.shutdown_tx = Some(shutdown_tx);
            
            let service = self.service.clone();
            
            tokio::spawn(async move {
                let server_result = Server::builder()
                    .add_service(BenchmarkServiceServer::new(service))
                    .serve_with_shutdown(addr, async {
                        shutdown_rx.await.ok();
                    })
                    .await;
                    
                if let Err(e) = server_result {
                    println!("gRPC server error: {}", e);
                }
            });
            
            // Wait for server to start
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("gRPC server started successfully");
            Ok(())
        }

        /// Stop the server gracefully
        pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
                
                // Wait for graceful shutdown
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                let stats = self.service.get_stats().await;
                println!("gRPC server stopped. Stats: requests={}, responses={}, errors={}", 
                    stats.total_requests, stats.total_responses, stats.error_count);
            }
            Ok(())
        }

        /// Get current server statistics
        pub async fn get_stats(&self) -> ServerStats {
            self.service.get_stats().await
        }
    }

    impl Drop for GrpcServer {
        fn drop(&mut self) {
            if let Some(shutdown_tx) = self.shutdown_tx.take() {
                let _ = shutdown_tx.send(());
            }
        }
    }
}

// Re-export for when gRPC feature is not enabled
#[cfg(not(feature = "grpc-comparison"))]
pub mod grpc_impl {
    pub struct GrpcServer;
    
    impl GrpcServer {
        pub fn new() -> Self {
            Self
        }
        
        pub async fn start(
            &mut self,
            _bind_addr: &str,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Err("gRPC comparison feature not enabled".into())
        }
        
        pub async fn stop(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            Ok(())
        }
    }
}

pub use grpc_impl::*;