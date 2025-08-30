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

//! gRPC transport implementation for benchmarking comparison with QUIC

use crate::{TransportError, Result};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, Mutex};
use tonic::{transport::{Server, Channel}, Request, Response, Status, Streaming};

// Include generated protobuf code
pub mod proto {
    tonic::include_proto!("benchmark");
}

use proto::{
    benchmark_service_server::{BenchmarkService, BenchmarkServiceServer},
    benchmark_service_client::BenchmarkServiceClient,
    BenchmarkRequest, BenchmarkResponse, SimpleRequest, SimpleResponse, ServerMetrics,
};

/// gRPC-based transport for performance comparison  
pub struct GrpcTransport {
    _client: Option<BenchmarkServiceClient<Channel>>,
    _server_handle: Option<tokio::task::JoinHandle<std::result::Result<(), tonic::transport::Error>>>,
}

impl GrpcTransport {
    pub fn new(_tls_config: crate::TlsConfig) -> Result<Self> {
        Ok(Self {
            _client: None,
            _server_handle: None,
        })
    }
    
    pub async fn listen(&self, addr: &str) -> Result<GrpcIncoming> {
        let addr = addr.parse().map_err(|e| TransportError::ConnectionFailed(format!("Invalid address: {}", e)))?;
        let service = BenchmarkServiceImpl::new();
        
        let (tx, rx) = mpsc::channel(1024);
        let incoming = GrpcIncoming::new(rx);
        
        // Start the gRPC server
        tokio::spawn(async move {
            Server::builder()
                .add_service(BenchmarkServiceServer::new(service))
                .serve(addr)
                .await
        });
        
        // Simulate incoming connections for benchmarking
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            let conn = GrpcConnection::new("grpc-connection".to_string());
            let _ = tx.send(conn).await;
        });
        
        Ok(incoming)
    }

    pub async fn connect(&self, addr: &str) -> Result<GrpcConnection> {
        let channel = Channel::from_shared(format!("http://{}", addr))
            .map_err(|e| TransportError::ConnectionFailed(format!("Invalid URI: {}", e)))?
            .connect()
            .await
            .map_err(|e| TransportError::ConnectionFailed(format!("Connection failed: {}", e)))?;
            
        let client = BenchmarkServiceClient::new(channel);
        Ok(GrpcConnection::with_client(client))
    }
}

#[async_trait]
impl crate::Transport for GrpcTransport {
    async fn connect(&self, addr: &str) -> Result<crate::Connection> {
        let grpc_conn = self.connect(addr).await?;
        // For now, we'll wrap the gRPC connection in a QUIC-compatible way
        // This is a simplified implementation for benchmarking purposes
        unimplemented!("gRPC transport should be used directly, not through the QUIC transport trait")
    }

    async fn listen(&self, addr: &str) -> Result<crate::Incoming> {
        let _grpc_incoming = self.listen(addr).await?;
        // For now, we'll wrap the gRPC incoming in a QUIC-compatible way
        // This is a simplified implementation for benchmarking purposes
        unimplemented!("gRPC transport should be used directly, not through the QUIC transport trait")
    }
}

/// gRPC connection implementation
pub struct GrpcConnection {
    id: String,
    client: Option<BenchmarkServiceClient<Channel>>,
    _sequence: Arc<Mutex<u32>>,
}

impl GrpcConnection {
    fn new(id: String) -> Self {
        Self {
            id,
            client: None,
            _sequence: Arc::new(Mutex::new(0)),
        }
    }
    
    fn with_client(client: BenchmarkServiceClient<Channel>) -> Self {
        Self {
            id: "client-connection".to_string(),
            client: Some(client),
            _sequence: Arc::new(Mutex::new(0)),
        }
    }
    
    pub async fn open_bi(&self) -> Result<GrpcStream> {
        Ok(GrpcStream::new(
            self.client.clone(),
            self.id.clone(),
        ))
    }
    
    pub async fn accept_bi(&self) -> Result<GrpcStream> {
        // For gRPC, accept_bi is the same as open_bi since it's request-response
        self.open_bi().await
    }
}

/// gRPC stream implementation
pub struct GrpcStream {
    client: Option<BenchmarkServiceClient<Channel>>,
    connection_id: String,
    sequence: Arc<Mutex<u32>>,
}

impl GrpcStream {
    fn new(client: Option<BenchmarkServiceClient<Channel>>, connection_id: String) -> Self {
        Self {
            client,
            connection_id,
            sequence: Arc::new(Mutex::new(0)),
        }
    }
    
    async fn next_sequence(&self) -> u32 {
        let mut seq = self.sequence.lock().await;
        *seq += 1;
        *seq
    }
    
    pub async fn send_message(&mut self, data: Bytes) -> Result<()> {
        if let Some(mut client) = self.client.clone() {
            let request = BenchmarkRequest {
                id: format!("{}-{}", self.connection_id, self.next_sequence().await),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64,
                payload: data.to_vec(),
                test_type: "latency".to_string(),
                sequence_number: self.next_sequence().await,
            };
            
            client.latency_test(Request::new(request))
                .await
                .map_err(|e| TransportError::StreamError(format!("gRPC call failed: {}", e)))?;
        } else {
            return Err(TransportError::ConnectionFailed("No client available".to_string()));
        }
        
        Ok(())
    }

    pub async fn receive_message(&mut self) -> Result<Bytes> {
        // For latency tests, we don't actually receive separate messages
        // The response is handled in send_message
        Ok(Bytes::from("grpc-response"))
    }
}

/// gRPC incoming connections handler
pub struct GrpcIncoming {
    receiver: mpsc::Receiver<GrpcConnection>,
}

impl GrpcIncoming {
    fn new(receiver: mpsc::Receiver<GrpcConnection>) -> Self {
        Self { receiver }
    }
    
    pub async fn accept(&mut self) -> Result<GrpcConnection> {
        self.receiver
            .recv()
            .await
            .ok_or_else(|| TransportError::ConnectionFailed("No more connections".to_string()))
    }
    
    pub fn local_addr(&self) -> Result<String> {
        // For gRPC, return a placeholder address
        Ok("127.0.0.1:50051".to_string())
    }
}

/// gRPC service implementation for benchmarking
#[derive(Clone)]
struct BenchmarkServiceImpl {
    message_count: Arc<Mutex<u64>>,
    start_time: Arc<Mutex<Option<SystemTime>>>,
}

impl BenchmarkServiceImpl {
    fn new() -> Self {
        Self {
            message_count: Arc::new(Mutex::new(0)),
            start_time: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl BenchmarkService for BenchmarkServiceImpl {
    // New streaming methods for comprehensive benchmark
    async fn unary_call(
        &self,
        request: Request<SimpleRequest>,
    ) -> std::result::Result<Response<SimpleResponse>, Status> {
        let req = request.into_inner();
        
        // Update statistics  
        {
            let mut count = self.message_count.lock().await;
            *count += 1;
            
            let mut start = self.start_time.lock().await;
            if start.is_none() {
                *start = Some(SystemTime::now());
            }
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
        
        Ok(Response::new(response))
    }

    async fn client_streaming(
        &self,
        _request: Request<Streaming<SimpleRequest>>,
    ) -> std::result::Result<Response<SimpleResponse>, Status> {
        // Not implemented in this legacy version
        Err(Status::unimplemented("client_streaming not implemented in transport/grpc.rs"))
    }

    async fn server_streaming(
        &self,
        _request: Request<SimpleRequest>,
    ) -> std::result::Result<Response<Self::ServerStreamingStream>, Status> {
        // Not implemented in this legacy version  
        Err(Status::unimplemented("server_streaming not implemented in transport/grpc.rs"))
    }

    async fn bidirectional_streaming(
        &self,
        _request: Request<Streaming<SimpleRequest>>,
    ) -> std::result::Result<Response<Self::BidirectionalStreamingStream>, Status> {
        // Not implemented in this legacy version
        Err(Status::unimplemented("bidirectional_streaming not implemented in transport/grpc.rs"))
    }

    type ServerStreamingStream = std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<SimpleResponse, Status>> + Send>>;
    type BidirectionalStreamingStream = std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<SimpleResponse, Status>> + Send>>;

    // Legacy methods
    async fn latency_test(
        &self,
        request: Request<BenchmarkRequest>,
    ) -> std::result::Result<Response<BenchmarkResponse>, Status> {
        let req = request.into_inner();
        
        // Update statistics
        {
            let mut count = self.message_count.lock().await;
            *count += 1;
            
            let mut start = self.start_time.lock().await;
            if start.is_none() {
                *start = Some(SystemTime::now());
            }
        }
        
        let response = BenchmarkResponse {
            id: req.id,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: vec![], // Minimal response for latency testing
            status: "ok".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: 1,
                total_messages: *self.message_count.lock().await,
                cpu_usage: 0.0, // Would need actual CPU monitoring
                memory_usage: 0, // Would need actual memory monitoring
            }),
        };
        
        Ok(Response::new(response))
    }

    async fn flood_test(
        &self,
        request: Request<Streaming<BenchmarkRequest>>,
    ) -> std::result::Result<Response<BenchmarkResponse>, Status> {
        let mut stream = request.into_inner();
        let _message_count = 0u64;
        
        // Process all incoming messages
        while let Some(_req) = stream.message().await? {
            // Update global statistics
            {
                let mut count = self.message_count.lock().await;
                *count += 1;
            }
        }
        
        let response = BenchmarkResponse {
            id: "flood-test-complete".to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            payload: vec![],
            status: "ok".to_string(),
            metrics: Some(ServerMetrics {
                concurrent_connections: 1,
                total_messages: *self.message_count.lock().await,
                cpu_usage: 0.0,
                memory_usage: 0,
            }),
        };
        
        Ok(Response::new(response))
    }

    async fn stream_test(
        &self,
        request: Request<Streaming<BenchmarkRequest>>,
    ) -> std::result::Result<Response<Self::StreamTestStream>, Status> {
        let stream = request.into_inner();
        let (tx, rx) = mpsc::channel(1024);
        let message_count = self.message_count.clone();
        
        // Process incoming stream and generate responses
        tokio::spawn(async move {
            let mut input_stream = stream;
            while let Ok(Some(req)) = input_stream.message().await {
                {
                    let mut count = message_count.lock().await;
                    *count += 1;
                }
                
                let response = BenchmarkResponse {
                    id: req.id,
                    timestamp: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_micros() as u64,
                    payload: req.payload, // Echo the payload back
                    status: "ok".to_string(),
                    metrics: None,
                };
                
                if tx.send(Ok(response)).await.is_err() {
                    break;
                }
            }
        });
        
        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::StreamTestStream))
    }
    
    type StreamTestStream = std::pin::Pin<Box<dyn futures::Stream<Item = std::result::Result<BenchmarkResponse, Status>> + Send>>;
}