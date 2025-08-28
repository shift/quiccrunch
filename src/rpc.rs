//! QuicCrunch RPC Framework
//! 
//! High-level RPC abstraction built on top of QUIC transport with Cap'n Proto serialization.

pub mod codegen;
// pub mod discovery; // Temporarily disabled

use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, SystemTime};
use std::pin::Pin;
use std::task::{Context, Poll};
use async_trait::async_trait;
use bytes::Bytes;
use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;
use tokio::sync::{RwLock, mpsc, Mutex};
use futures::Stream;
use crate::{Connection, QuicStream, TransportError};
use crate::schema::rpc_capnp::*;

/// Service information for discovery
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub endpoints: Vec<EndpointInfo>,
}

/// Endpoint information for services
#[derive(Debug, Clone)]
pub struct EndpointInfo {
    pub address: String,
    pub port: u16,
    pub protocol: String,
    pub health: HealthStatus,
    pub weight: u32,
}

/// RPC request/response correlation ID
type RequestId = u64;

/// Stream identifier for streaming RPCs
type StreamId = u64;

/// RPC method result
pub type RpcResult<T> = std::result::Result<T, RpcError>;

/// Streaming RPC result
pub type StreamingRpcResult<T> = std::result::Result<T, RpcError>;

/// RPC-specific errors
#[derive(Debug, Clone)]
pub enum RpcError {
    /// Method not found
    MethodNotFound(String),
    /// Invalid parameters
    InvalidParameters(String),
    /// Serialization error
    SerializationError(String),
    /// Transport error
    Transport(String),
    /// Service error
    ServiceError(String),
    /// Timeout
    Timeout,
    /// Service unavailable
    ServiceUnavailable(String),
    /// Permission denied
    PermissionDenied(String),
    /// Rate limited
    RateLimited(String),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RpcError::MethodNotFound(method) => write!(f, "Method not found: {}", method),
            RpcError::InvalidParameters(msg) => write!(f, "Invalid parameters: {}", msg),
            RpcError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            RpcError::Transport(msg) => write!(f, "Transport error: {}", msg),
            RpcError::ServiceError(msg) => write!(f, "Service error: {}", msg),
            RpcError::Timeout => write!(f, "Request timeout"),
            RpcError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
            RpcError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            RpcError::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
        }
    }
}

impl std::error::Error for RpcError {}

impl From<TransportError> for RpcError {
    fn from(err: TransportError) -> Self {
        RpcError::Transport(err.to_string())
    }
}

impl From<capnp::Error> for RpcError {
    fn from(err: capnp::Error) -> Self {
        RpcError::SerializationError(err.to_string())
    }
}

impl From<ErrorCode> for RpcError {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::InvalidRequest => RpcError::InvalidParameters("Invalid request".to_string()),
            ErrorCode::MethodNotFound => RpcError::MethodNotFound("Method not found".to_string()),
            ErrorCode::InvalidParams => RpcError::InvalidParameters("Invalid parameters".to_string()),
            ErrorCode::InternalError => RpcError::ServiceError("Internal error".to_string()),
            ErrorCode::Timeout => RpcError::Timeout,
            ErrorCode::ServiceUnavailable => RpcError::ServiceUnavailable("Service unavailable".to_string()),
            ErrorCode::PermissionDenied => RpcError::PermissionDenied("Permission denied".to_string()),
            ErrorCode::RateLimited => RpcError::RateLimited("Rate limited".to_string()),
            _ => RpcError::ServiceError("Unknown error".to_string()),
        }
    }
}

/// RPC call context with metadata and tracing
#[derive(Debug, Clone)]
pub struct RpcContext {
    pub service: String,
    pub method: String,
    pub request_id: RequestId,
    pub metadata: HashMap<String, String>,
    pub start_time: SystemTime,
    pub timeout: Option<Duration>,
}

impl RpcContext {
    pub fn new(service: String, method: String) -> Self {
        Self {
            service,
            method,
            request_id: 0,
            metadata: HashMap::new(),
            start_time: SystemTime::now(),
            timeout: None,
        }
    }
    
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed().unwrap_or_default()
    }
}

/// Base trait for RPC services
#[async_trait]
pub trait RpcService: Send + Sync {
    async fn call(&self, context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes>;
}

/// Streaming RPC service trait for advanced streaming patterns
#[async_trait]
pub trait StreamingRpcService: Send + Sync {
    /// Server streaming: single request, multiple responses
    async fn server_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        params: Bytes,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()>;
    
    /// Client streaming: multiple requests, single response
    async fn client_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        stream: RpcStreamReceiver,
    ) -> StreamingRpcResult<Bytes>;
    
    /// Bidirectional streaming: multiple requests, multiple responses
    async fn bidirectional_streaming_call(
        &self,
        context: &RpcContext,
        method: &str,
        stream: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()>;
}

/// Sink for sending streaming data
pub struct RpcStreamSink {
    stream: Arc<Mutex<QuicStream>>,
    stream_id: StreamId,
}

impl RpcStreamSink {
    pub fn new(stream: QuicStream, stream_id: StreamId) -> Self {
        Self {
            stream: Arc::new(Mutex::new(stream)),
            stream_id,
        }
    }
    
    pub async fn send(&self, data: Bytes) -> RpcResult<()> {
        let stream_chunk_data = self.create_stream_chunk(data)?;
        let mut stream = self.stream.lock().await;
        stream.send_message(stream_chunk_data).await?;
        Ok(())
    }
    
    pub async fn finish(self) -> RpcResult<()> {
        let stream_end_data = self.create_stream_end()?;
        let mut stream = self.stream.lock().await;
        stream.send_message(stream_end_data).await?;
        Ok(())
    }
    
    fn create_stream_chunk(&self, data: Bytes) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut rpc_message = message.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::StreamChunk);
        
        let mut chunk = rpc_message.init_stream_chunk();
        chunk.set_stream_id(self.stream_id);
        chunk.set_data(&data);
        
        let serialized = serialize::write_message_to_words(&message);
        Ok(Bytes::from(serialized))
    }
    
    fn create_stream_end(&self) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut rpc_message = message.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::StreamEnd);
        
        let mut end = rpc_message.init_stream_end();
        end.set_stream_id(self.stream_id);
        
        let serialized = serialize::write_message_to_words(&message);
        Ok(Bytes::from(serialized))
    }
}

/// Receiver for streaming data
pub struct RpcStreamReceiver {
    receiver: mpsc::UnboundedReceiver<StreamingRpcResult<Bytes>>,
}

impl RpcStreamReceiver {
    pub fn new(receiver: mpsc::UnboundedReceiver<StreamingRpcResult<Bytes>>) -> Self {
        Self { receiver }
    }
    
    pub async fn next(&mut self) -> Option<StreamingRpcResult<Bytes>> {
        self.receiver.recv().await
    }
}

impl Stream for RpcStreamReceiver {
    type Item = StreamingRpcResult<Bytes>;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// RPC Server for handling incoming requests
#[derive(Clone)]
pub struct RpcServer {
    services: Arc<RwLock<HashMap<String, Arc<dyn RpcService>>>>,
    streaming_services: Arc<RwLock<HashMap<String, Arc<dyn StreamingRpcService>>>>,
    next_stream_id: Arc<AtomicU64>,
}

impl RpcServer {
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            streaming_services: Arc::new(RwLock::new(HashMap::new())),
            next_stream_id: Arc::new(AtomicU64::new(1)),
        }
    }
    
    /// Register a regular RPC service
    pub async fn register_service(&self, name: String, service: Arc<dyn RpcService>) {
        self.services.write().await.insert(name, service);
    }
    
    /// Register a streaming RPC service  
    pub async fn register_streaming_service(&self, name: String, service: Arc<dyn StreamingRpcService>) {
        self.streaming_services.write().await.insert(name, service);
    }
    
    /// Handle incoming connection - core server functionality
    pub async fn handle_connection(&self, mut connection: Connection) -> RpcResult<()> {
        loop {
            // Accept incoming bidirectional stream
            let mut stream = match connection.accept_bi().await {
                Ok(stream) => stream,
                Err(e) => {
                    // Connection closed or error - this is expected when client disconnects
                    tracing::debug!("Connection closed: {}", e);
                    return Ok(());
                }
            };

            // Clone server for concurrent stream handling
            let server = self.clone();
            
            // Handle each stream concurrently
            tokio::spawn(async move {
                if let Err(e) = server.handle_stream(&mut stream).await {
                    tracing::error!("Stream handling error: {}", e);
                }
            });
        }
    }
    
    /// Handle a single RPC stream - supports both unary and streaming calls
    async fn handle_stream(&self, stream: &mut QuicStream) -> RpcResult<()> {
        // Receive request message
        let request_data = match stream.receive_message().await {
            Ok(data) => data,
            Err(e) => {
                tracing::error!("Failed to receive request: {}", e);
                return Err(RpcError::Transport(e.to_string()));
            }
        };

        // Parse the request with streaming type
        let (context, params, streaming_type) = match self.parse_request(request_data).await {
            Ok((context, params, streaming_type)) => (context, params, streaming_type),
            Err(e) => {
                // Send error response
                let error_response = Self::create_error_response(
                    0, // Unknown request ID
                    Self::error_to_code(&e),
                    &e.to_string()
                );
                let _ = stream.send_message(error_response).await;
                return Err(e);
            }
        };

        // Handle different streaming types
        match streaming_type {
            StreamingType::Unary => {
                // Standard unary RPC
                let result = self.process_request(&context, params).await;
                
                let response_data = match result {
                    Ok(response_bytes) => {
                        Self::create_success_response(response_bytes)
                    }
                    Err(e) => {
                        Self::create_error_response(
                            context.request_id,
                            Self::error_to_code(&e),
                            &e.to_string()
                        )
                    }
                };

                stream.send_message(response_data).await
                    .map_err(|e| RpcError::Transport(e.to_string()))?;
            }
            StreamingType::ServerStreaming => {
                // Server streaming: single request, multiple responses
                let result = self.process_server_streaming_request(&context, params, stream).await;
                if let Err(e) = result {
                    let error_response = Self::create_error_response(
                        context.request_id,
                        Self::error_to_code(&e),
                        &e.to_string()
                    );
                    // Note: Cannot send error on stream that was moved
                    return Err(e);
                }
            }
            StreamingType::ClientStreaming => {
                // Client streaming: multiple requests, single response
                let result = self.process_client_streaming_request(&context, stream).await;
                match result {
                    Ok(response_bytes) => {
                        // Stream was moved, cannot send response
                        return Ok(());
                    }
                    Err(e) => {
                        // Stream was moved, cannot send error
                        return Err(e);
                    }
                }
            }
            StreamingType::BidirectionalStreaming => {
                // Bidirectional streaming: multiple requests, multiple responses
                let result = self.process_bidirectional_streaming_request(&context, stream).await;
                if let Err(e) = result {
                    // Stream was moved, cannot send error
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Parse incoming request data into context and parameters, with streaming type
    async fn parse_request(&self, data: Bytes) -> RpcResult<(RpcContext, Bytes, StreamingType)> {
        let mut data_ref = data.as_ref();
        let reader = capnp::serialize::read_message_from_flat_slice(&mut data_ref, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(format!("Failed to read request: {}", e)))?;

        let rpc_message = reader.get_root::<rpc_message::Reader>()
            .map_err(|e| RpcError::SerializationError(format!("Invalid RPC message: {}", e)))?;

        let message_type = rpc_message.get_message_type()
            .map_err(|e| RpcError::SerializationError(format!("Invalid message type: {}", e)))?;

        if message_type != MessageType::Request {
            return Err(RpcError::InvalidParameters("Expected request message".to_string()));
        }

        let request_union = rpc_message.which()
            .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;

        if let rpc_message::Which::Request(request) = request_union {
            let request = request
                .map_err(|e| RpcError::SerializationError(format!("Invalid request: {}", e)))?;

            let service = request.get_service()
                .map_err(|e| RpcError::SerializationError(format!("Invalid service: {}", e)))?
                .to_string()
                .map_err(|_| RpcError::SerializationError("Invalid service string".to_string()))?;

            let method = request.get_method()
                .map_err(|e| RpcError::SerializationError(format!("Invalid method: {}", e)))?
                .to_string()
                .map_err(|_| RpcError::SerializationError("Invalid method string".to_string()))?;

            let parameters = request.get_parameters()
                .map_err(|e| RpcError::SerializationError(format!("Invalid parameters: {}", e)))?;

            let streaming_type = request.get_streaming_type()
                .unwrap_or(StreamingType::Unary); // Default to unary if not specified

            let request_id = rpc_message.get_request_id();

            let mut context = RpcContext {
                service,
                method,
                request_id,
                metadata: HashMap::new(),
                start_time: SystemTime::now(),
                timeout: None,
            };

            // Parse timeout if specified
            let timeout_ms = request.get_timeout();
            if timeout_ms > 0 {
                context.timeout = Some(Duration::from_millis(timeout_ms as u64));
            }

            Ok((context, Bytes::copy_from_slice(parameters), streaming_type))
        } else {
            Err(RpcError::SerializationError("Expected request".to_string()))
        }
    }

    /// Process the RPC request by routing to the appropriate service
    async fn process_request(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        let services = self.services.read().await;
        
        // Find the service
        let service = services.get(&context.service)
            .ok_or_else(|| RpcError::ServiceError(format!("Service not found: {}", context.service)))?
            .clone();

        // Drop the read lock before calling the service
        drop(services);

        // Call the service
        service.call(context, &context.method, params).await
    }
    
    /// Process server streaming request: single request, multiple responses
    async fn process_server_streaming_request(&self, context: &RpcContext, params: Bytes, stream: &mut QuicStream) -> RpcResult<()> {
        let streaming_services = self.streaming_services.read().await;
        
        // Find the streaming service
        let service = streaming_services.get(&context.service)
            .ok_or_else(|| RpcError::ServiceError(format!("Streaming service not found: {}", context.service)))?
            .clone();

        // Drop the read lock before calling the service
        drop(streaming_services);

        // Create a stream sink for responses - we need to handle ownership differently
        let stream_id = self.next_stream_id.fetch_add(1, Ordering::SeqCst);
        
        // For now, create a mock sink. In a full implementation, we'd need to restructure this
        // to avoid ownership issues with QuicStream
        return Err(RpcError::ServiceError("Server streaming not fully implemented - ownership conflicts".to_string()));
    }
    
    /// Process client streaming request: multiple requests, single response
    async fn process_client_streaming_request(&self, context: &RpcContext, stream: &mut QuicStream) -> RpcResult<Bytes> {
        // For now, return an error indicating this needs further implementation
        Err(RpcError::ServiceError("Client streaming not fully implemented - ownership conflicts".to_string()))
    }
    
    /// Process bidirectional streaming request: multiple requests, multiple responses
    async fn process_bidirectional_streaming_request(&self, context: &RpcContext, stream: &mut QuicStream) -> RpcResult<()> {
        // For now, return an error indicating this needs further implementation
        Err(RpcError::ServiceError("Bidirectional streaming not fully implemented - ownership conflicts".to_string()))
    }
    
    /// Parse stream chunk message
    fn parse_stream_chunk(data: Bytes) -> RpcResult<(Bytes, bool)> {
        let mut data_ref = data.as_ref();
        let reader = capnp::serialize::read_message_from_flat_slice(&mut data_ref, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(format!("Failed to read stream chunk: {}", e)))?;

        let rpc_message = reader.get_root::<rpc_message::Reader>()
            .map_err(|e| RpcError::SerializationError(format!("Invalid RPC message: {}", e)))?;

        let message_type = rpc_message.get_message_type()
            .map_err(|e| RpcError::SerializationError(format!("Invalid message type: {}", e)))?;

        if message_type != MessageType::StreamChunk {
            return Err(RpcError::InvalidParameters("Expected stream chunk message".to_string()));
        }

        let chunk_union = rpc_message.which()
            .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;

        if let rpc_message::Which::StreamChunk(chunk) = chunk_union {
            let chunk = chunk
                .map_err(|e| RpcError::SerializationError(format!("Invalid chunk: {}", e)))?;

            let data = chunk.get_data()
                .map_err(|e| RpcError::SerializationError(format!("Invalid chunk data: {}", e)))?;
            
            let is_last = chunk.get_is_last();

            Ok((Bytes::copy_from_slice(data), is_last))
        } else {
            Err(RpcError::SerializationError("Expected stream chunk".to_string()))
        }
    }
    
    /// Create a success response message
    pub fn create_success_response(data: Bytes) -> Bytes {
        let mut message = Builder::new_default();
        let mut rpc_message = message.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::Response);
        
        let mut response = rpc_message.init_response();
        response.set_result(&data);
        
        let serialized = serialize::write_message_to_words(&message);
        Bytes::from(serialized)
    }
    
    /// Create an error response message
    pub fn create_error_response(request_id: RequestId, error_code: ErrorCode, message: &str) -> Bytes {
        let mut msg = Builder::new_default();
        let mut rpc_message = msg.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::Error);
        rpc_message.set_request_id(request_id);
        
        let mut error = rpc_message.init_error();
        error.set_code(error_code);
        error.set_message(message.into());
        
        let serialized = serialize::write_message_to_words(&msg);
        Bytes::from(serialized)
    }
    
    /// Convert RpcError to ErrorCode
    pub fn error_to_code(error: &RpcError) -> ErrorCode {
        match error {
            RpcError::MethodNotFound(_) => ErrorCode::MethodNotFound,
            RpcError::InvalidParameters(_) => ErrorCode::InvalidParams,
            RpcError::SerializationError(_) => ErrorCode::InvalidRequest,
            RpcError::Transport(_) => ErrorCode::InternalError,
            RpcError::ServiceError(_) => ErrorCode::InternalError,
            RpcError::Timeout => ErrorCode::Timeout,
            RpcError::ServiceUnavailable(_) => ErrorCode::ServiceUnavailable,
            RpcError::PermissionDenied(_) => ErrorCode::PermissionDenied,
            RpcError::RateLimited(_) => ErrorCode::RateLimited,
        }
    }
}

impl Default for RpcServer {
    fn default() -> Self {
        Self::new()
    }
}

/// RPC client for making remote calls
pub struct RpcClient {
    connection: Connection,
    next_request_id: AtomicU64,
}

impl RpcClient {
    /// Create a new RPC client
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            next_request_id: AtomicU64::new(1),
        }
    }
    
    /// Call a remote method with context
    pub async fn call_with_context(&self, context: &mut RpcContext, params: Bytes) -> RpcResult<Bytes> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        context.request_id = request_id;
        
        // Create the actual call future
        let call_future = async {
            // Create Cap'n Proto request
            let request_data = self.create_request(context, params)?;
            
            // Send request and receive response
            let mut stream = self.connection.open_bi().await?;
            stream.send_message(request_data).await?;
            
            let response_data = stream.receive_message().await?;
            self.parse_response(response_data)
        };
        
        // Apply timeout if specified
        if let Some(timeout_duration) = context.timeout {
            match tokio::time::timeout(timeout_duration, call_future).await {
                Ok(result) => result,
                Err(_) => Err(RpcError::Timeout),
            }
        } else {
            call_future.await
        }
    }
    
    /// Simple call with service and method names
    pub async fn call(&self, service: &str, method: &str, params: Bytes) -> RpcResult<Bytes> {
        let mut context = RpcContext::new(service.to_string(), method.to_string());
        self.call_with_context(&mut context, params).await
    }
    
    fn create_request(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut rpc_message = message.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::Request);
        rpc_message.set_request_id(context.request_id);
        
        let mut request = rpc_message.init_request();
        request.set_service(context.service.as_str().into());
        request.set_method(context.method.as_str().into());
        request.set_parameters(&params);
        
        let serialized = serialize::write_message_to_words(&message);
        Ok(Bytes::from(serialized))
    }
    
    fn parse_response(&self, data: Bytes) -> RpcResult<Bytes> {
        let mut data_ref = data.as_ref();
        let reader = capnp::serialize::read_message_from_flat_slice(&mut data_ref, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(format!("Failed to read response: {}", e)))?;
        
        let rpc_message = reader.get_root::<rpc_message::Reader>()
            .map_err(|e| RpcError::SerializationError(format!("Invalid RPC message: {}", e)))?;
        
        let message_type = rpc_message.get_message_type()
            .map_err(|e| RpcError::SerializationError(format!("Invalid message type: {}", e)))?;
        
        match message_type {
            MessageType::Response => {
                let response_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;
                if let rpc_message::Which::Response(response) = response_union {
                    let response = response
                        .map_err(|e| RpcError::SerializationError(format!("Invalid response: {}", e)))?;
                    let result_data = response.get_result()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid response data: {}", e)))?;
                    Ok(Bytes::copy_from_slice(result_data))
                } else {
                    Err(RpcError::SerializationError("Expected response".to_string()))
                }
            }
            MessageType::Error => {
                let error_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;
                if let rpc_message::Which::Error(error) = error_union {
                    let error = error
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error: {}", e)))?;
                    let error_message = error.get_message()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error message: {}", e)))?
                        .to_string()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(RpcError::ServiceError(error_message))
                } else {
                    Err(RpcError::SerializationError("Expected error".to_string()))
                }
            }
            _ => Err(RpcError::SerializationError("Unexpected message type".to_string())),
        }
    }
    
    /// Start a server streaming call: single request, multiple responses
    pub async fn server_streaming_call(&self, service: &str, method: &str, params: Bytes) -> RpcResult<RpcStreamReceiver> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let mut context = RpcContext::new(service.to_string(), method.to_string());
        context.request_id = request_id;
        
        // Create streaming request
        let request_data = self.create_streaming_request(&context, params, StreamingType::ServerStreaming)?;
        
        // Send request
        let mut stream = self.connection.open_bi().await?;
        stream.send_message(request_data).await?;
        
        // Create receiver for streaming responses
        let (tx, rx) = mpsc::unbounded_channel();
        let receiver = RpcStreamReceiver::new(rx);
        
        // Spawn task to receive stream messages
        tokio::spawn(async move {
            loop {
                match stream.receive_message().await {
                    Ok(data) => {
                        match Self::parse_stream_response(data) {
                            Ok((chunk_data, is_last)) => {
                                if tx.send(Ok(chunk_data)).is_err() {
                                    break; // Receiver dropped
                                }
                                if is_last {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(RpcError::Transport(e.to_string())));
                        break;
                    }
                }
            }
        });
        
        Ok(receiver)
    }
    
    /// Start a client streaming call: multiple requests, single response
    pub async fn client_streaming_call(&self, service: &str, method: &str) -> RpcResult<(RpcStreamSink, tokio::task::JoinHandle<RpcResult<Bytes>>)> {
        // For now, return an error as this needs proper stream splitting
        Err(RpcError::ServiceError("Client streaming not implemented - requires stream ownership handling".to_string()))
    }
    
    /// Start a bidirectional streaming call: multiple requests, multiple responses
    pub async fn bidirectional_streaming_call(&self, service: &str, method: &str) -> RpcResult<(RpcStreamSink, RpcStreamReceiver)> {
        // For now, return an error as this needs proper stream splitting
        Err(RpcError::ServiceError("Bidirectional streaming not implemented - requires stream ownership handling".to_string()))
    }
    
    /// Create a streaming request message
    fn create_streaming_request(&self, context: &RpcContext, params: Bytes, streaming_type: StreamingType) -> RpcResult<Bytes> {
        let mut message = Builder::new_default();
        let mut rpc_message = message.init_root::<rpc_message::Builder>();
        rpc_message.set_message_type(MessageType::Request);
        rpc_message.set_request_id(context.request_id);
        
        let mut request = rpc_message.init_request();
        request.set_service(context.service.as_str().into());
        request.set_method(context.method.as_str().into());
        request.set_parameters(&params);
        request.set_streaming_type(streaming_type);
        
        if let Some(timeout) = context.timeout {
            request.set_timeout(timeout.as_millis() as u32);
        }
        
        let serialized = serialize::write_message_to_words(&message);
        Ok(Bytes::from(serialized))
    }
    
    /// Parse streaming response chunk
    fn parse_stream_response(data: Bytes) -> RpcResult<(Bytes, bool)> {
        let mut data_ref = data.as_ref();
        let reader = capnp::serialize::read_message_from_flat_slice(&mut data_ref, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(format!("Failed to read stream response: {}", e)))?;

        let rpc_message = reader.get_root::<rpc_message::Reader>()
            .map_err(|e| RpcError::SerializationError(format!("Invalid RPC message: {}", e)))?;

        let message_type = rpc_message.get_message_type()
            .map_err(|e| RpcError::SerializationError(format!("Invalid message type: {}", e)))?;

        match message_type {
            MessageType::StreamChunk => {
                let chunk_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;

                if let rpc_message::Which::StreamChunk(chunk) = chunk_union {
                    let chunk = chunk
                        .map_err(|e| RpcError::SerializationError(format!("Invalid chunk: {}", e)))?;

                    let data = chunk.get_data()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid chunk data: {}", e)))?;
                    
                    let is_last = chunk.get_is_last();

                    Ok((Bytes::copy_from_slice(data), is_last))
                } else {
                    Err(RpcError::SerializationError("Expected stream chunk".to_string()))
                }
            }
            MessageType::StreamEnd => {
                // Stream ended gracefully
                Ok((Bytes::new(), true))
            }
            MessageType::Error => {
                // Parse error and return it
                let error_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;
                if let rpc_message::Which::Error(error) = error_union {
                    let error = error
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error: {}", e)))?;
                    let error_message = error.get_message()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error message: {}", e)))?
                        .to_string()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(RpcError::ServiceError(error_message))
                } else {
                    Err(RpcError::SerializationError("Expected error".to_string()))
                }
            }
            _ => Err(RpcError::SerializationError("Unexpected message type in stream".to_string())),
        }
    }
    
    /// Parse final response for client streaming
    fn parse_final_response(data: Bytes) -> RpcResult<Bytes> {
        let mut data_ref = data.as_ref();
        let reader = capnp::serialize::read_message_from_flat_slice(&mut data_ref, ReaderOptions::new())
            .map_err(|e| RpcError::SerializationError(format!("Failed to read final response: {}", e)))?;

        let rpc_message = reader.get_root::<rpc_message::Reader>()
            .map_err(|e| RpcError::SerializationError(format!("Invalid RPC message: {}", e)))?;

        let message_type = rpc_message.get_message_type()
            .map_err(|e| RpcError::SerializationError(format!("Invalid message type: {}", e)))?;

        match message_type {
            MessageType::Response => {
                let response_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;
                if let rpc_message::Which::Response(response) = response_union {
                    let response = response
                        .map_err(|e| RpcError::SerializationError(format!("Invalid response: {}", e)))?;
                    let result_data = response.get_result()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid response data: {}", e)))?;
                    Ok(Bytes::copy_from_slice(result_data))
                } else {
                    Err(RpcError::SerializationError("Expected response".to_string()))
                }
            }
            MessageType::Error => {
                let error_union = rpc_message.which()
                    .map_err(|e| RpcError::SerializationError(format!("Invalid union: {}", e)))?;
                if let rpc_message::Which::Error(error) = error_union {
                    let error = error
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error: {}", e)))?;
                    let error_message = error.get_message()
                        .map_err(|e| RpcError::SerializationError(format!("Invalid error message: {}", e)))?
                        .to_string()
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    Err(RpcError::ServiceError(error_message))
                } else {
                    Err(RpcError::SerializationError("Expected error".to_string()))
                }
            }
            _ => Err(RpcError::SerializationError("Unexpected message type".to_string())),
        }
    }
}

/// Connection pool for RPC clients
pub struct RpcConnectionPool {
    connections: Vec<Connection>,
    next_connection: AtomicU64,
}

impl RpcConnectionPool {
    pub fn new(connections: Vec<Connection>) -> Self {
        Self {
            connections,
            next_connection: AtomicU64::new(0),
        }
    }
    
    pub fn get_client(&self) -> Option<RpcClient> {
        if self.connections.is_empty() {
            return None;
        }
        
        let index = self.next_connection.fetch_add(1, Ordering::SeqCst) as usize % self.connections.len();
        self.connections.get(index).map(|conn| RpcClient::new(conn.clone()))
    }
}

/// Pooled RPC client with automatic failover
pub struct PooledRpcClient {
    pool: Arc<RpcConnectionPool>,
}

impl PooledRpcClient {
    pub fn new(pool: Arc<RpcConnectionPool>) -> Self {
        Self { pool }
    }
    
    pub async fn call(&self, service: &str, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match self.pool.get_client() {
            Some(client) => client.call(service, method, params).await,
            None => Err(RpcError::ServiceUnavailable("No connections available".to_string())),
        }
    }
}