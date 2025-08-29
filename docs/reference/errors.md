# Error Reference Guide

This comprehensive reference documents all QuicCrunch error types, codes, and resolution strategies for developers and operators.

## Table of Contents

1. [Error Categories](#error-categories)
2. [Core RPC Errors](#core-rpc-errors)
3. [Transport Errors](#transport-errors)
4. [TLS/Security Errors](#tlssecurity-errors)
5. [Serialization Errors](#serialization-errors)
6. [Resource Errors](#resource-errors)
7. [Configuration Errors](#configuration-errors)
8. [Client Errors](#client-errors)
9. [Server Errors](#server-errors)
10. [Error Handling Patterns](#error-handling-patterns)

## Error Categories

QuicCrunch errors are organized into logical categories for easier diagnosis and handling:

```rust
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum RpcError {
    // Core RPC Errors
    #[error("Method not found: {method}")]
    MethodNotFound { method: String },
    
    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },
    
    #[error("Service error: {message}")]
    ServiceError { message: String },
    
    #[error("Request timeout after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    
    // Transport Errors
    #[error("Connection failed: {reason}")]
    ConnectionFailed { reason: String },
    
    #[error("Transport error: {message}")]
    TransportError { message: String },
    
    // Security Errors
    #[error("Authentication failed: {reason}")]
    AuthenticationFailed { reason: String },
    
    #[error("Authorization denied: {resource}")]
    AuthorizationDenied { resource: String },
    
    // Resource Errors
    #[error("Resource not found: {resource}")]
    NotFound { resource: String },
    
    #[error("Resource limit exceeded: {limit}")]
    ResourceLimitExceeded { limit: String },
    
    // Stream Errors
    #[error("Stream closed unexpectedly")]
    StreamClosed,
    
    #[error("Stream overflow: {details}")]
    StreamOverflow { details: String },
}
```

## Core RPC Errors

### RPC_METHOD_NOT_FOUND

**Error Code**: `RPC_METHOD_NOT_FOUND`  
**HTTP Status**: 404  
**Description**: The requested RPC method is not implemented by the service.

```rust
// Example occurrence
#[rpc_service(name = "Calculator")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
    // subtract method not implemented
}

// Client calls non-existent method
client.subtract(5, 3).await; // Returns MethodNotFound error
```

**Resolution**:
- Verify method name spelling and casing
- Check service version compatibility
- Ensure service implements the requested method
- Review API documentation for available methods

**Prevention**:
```rust
// Use code generation to prevent typos
use quiccrunch_macros::rpc_client;

#[rpc_client(service = "Calculator")]
struct CalculatorClient;

// Compile-time method validation
let result = client.add(5, 3).await?; // ✅ Validated at compile time
// client.subtract(5, 3).await?; // ❌ Compile error if method doesn't exist
```

### RPC_INVALID_REQUEST

**Error Code**: `RPC_INVALID_REQUEST`  
**HTTP Status**: 400  
**Description**: Request parameters are invalid or malformed.

```rust
// Common causes
#[derive(Serialize, Deserialize)]
struct CreateUserRequest {
    #[serde(validate = "email")]
    email: String,
    #[serde(validate = "length(min = 1, max = 100)")]
    name: String,
}

// Invalid email format
let request = CreateUserRequest {
    email: "invalid-email".to_string(), // Missing @ and domain
    name: "".to_string(), // Empty name
};
```

**Resolution**:
- Validate input parameters before sending
- Check parameter types and formats
- Review API documentation for parameter requirements
- Use schema validation libraries

**Prevention**:
```rust
use validator::{Validate, ValidationError};

#[derive(Serialize, Deserialize, Validate)]
struct CreateUserRequest {
    #[validate(email)]
    email: String,
    #[validate(length(min = 1, max = 100))]
    name: String,
}

impl CreateUserRequest {
    pub fn new(email: String, name: String) -> Result<Self, ValidationError> {
        let request = Self { email, name };
        request.validate()?;
        Ok(request)
    }
}
```

### RPC_TIMEOUT

**Error Code**: `RPC_TIMEOUT`  
**HTTP Status**: 408  
**Description**: Request exceeded the configured timeout duration.

```rust
// Configuration
let client = CalculatorClient::builder()
    .timeout(Duration::from_secs(30))
    .build();

// Long-running operation
let result = client.complex_calculation(huge_dataset).await;
// Returns Timeout error if takes longer than 30 seconds
```

**Resolution**:
- Increase timeout for long-running operations
- Optimize service implementation for better performance
- Use streaming for large data processing
- Implement progress reporting for long operations

**Prevention**:
```rust
// Configure appropriate timeouts per operation
#[rpc_service(name = "DataProcessor")]
trait DataProcessor {
    #[rpc(timeout = "5s")]
    async fn quick_lookup(&self, id: u64) -> RpcResult<Data>;
    
    #[rpc(timeout = "300s")]
    async fn batch_process(&self, data: Vec<Input>) -> RpcResult<ProcessResult>;
    
    #[rpc(streaming = true)] // No timeout for streaming
    async fn process_stream(&self, input: RpcStreamSink<Data>) -> RpcResult<RpcStreamSource<Result>>;
}
```

## Transport Errors

### QUIC_CONNECTION_FAILED

**Error Code**: `QUIC_CONNECTION_FAILED`  
**HTTP Status**: 503  
**Description**: Failed to establish QUIC connection to the server.

```rust
// Common causes
let transport = QuicTransport::new(TlsConfig::default())?;

// Server not running
let connection = transport.connect("127.0.0.1:50051").await;
// Returns ConnectionFailed error

// Wrong port
let connection = transport.connect("127.0.0.1:50052").await;
// Returns ConnectionFailed error

// Network unreachable
let connection = transport.connect("192.168.999.999:50051").await;
// Returns ConnectionFailed error
```

**Resolution**:
- Verify server is running and reachable
- Check network connectivity and firewall rules
- Validate server address and port
- Ensure UDP traffic is allowed

**Diagnostic Commands**:
```bash
# Check if server is listening
netstat -ulnp | grep :50051

# Test UDP connectivity
nc -u -v hostname 50051

# Check firewall rules
iptables -L -n | grep 50051

# Test network reachability
ping hostname
traceroute hostname
```

### QUIC_STREAM_RESET

**Error Code**: `QUIC_STREAM_RESET`  
**HTTP Status**: 502  
**Description**: QUIC stream was reset by the peer.

```rust
// Server-side stream reset
async fn process_data(&self, mut stream: RpcStreamSink<Data>) -> RpcResult<()> {
    while let Some(data) = stream.recv().await? {
        if data.is_invalid() {
            // Server resets stream due to invalid data
            return Err(RpcError::InvalidRequest { 
                message: "Invalid data format".to_string() 
            });
        }
        process(data).await?;
    }
    Ok(())
}
```

**Resolution**:
- Check for application-level errors on the server
- Verify data format and validation
- Implement proper error handling in stream processing
- Add retry logic for transient failures

**Prevention**:
```rust
// Robust stream handling
async fn handle_stream_with_retry(
    client: &Client,
    data: Vec<Data>,
    max_retries: usize,
) -> Result<(), RpcError> {
    for attempt in 0..=max_retries {
        match client.send_stream(data.clone()).await {
            Ok(result) => return Ok(result),
            Err(RpcError::StreamReset) if attempt < max_retries => {
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(RpcError::Timeout { timeout_ms: 0 })
}
```

## TLS/Security Errors

### TLS_CERTIFICATE_INVALID

**Error Code**: `TLS_CERTIFICATE_INVALID`  
**HTTP Status**: 495  
**Description**: Server certificate validation failed.

```rust
// Common causes
let tls_config = TlsConfig {
    ca_file: Some("/path/to/ca.crt".to_string()),
    verify_hostname: true,
    ..Default::default()
};

// Expired certificate
// Self-signed certificate without proper CA
// Hostname mismatch
let transport = QuicTransport::new(tls_config)?;
let connection = transport.connect("mismatch.example.com:50051").await;
// Returns TlsCertificateInvalid error
```

**Resolution**:
- Verify certificate validity and expiration
- Check certificate chain is complete
- Ensure hostname matches certificate CN/SAN
- Add custom CA to trust store if using self-signed certificates

**Certificate Validation**:
```bash
# Check certificate details
openssl x509 -in server.crt -text -noout

# Verify certificate chain
openssl verify -CAfile ca.crt server.crt

# Check certificate expiration
echo | openssl s_client -connect hostname:50051 2>/dev/null | openssl x509 -noout -dates

# Test certificate with specific hostname
echo | openssl s_client -connect hostname:50051 -servername hostname
```

### TLS_HANDSHAKE_FAILED

**Error Code**: `TLS_HANDSHAKE_FAILED`  
**HTTP Status**: 525  
**Description**: TLS handshake process failed.

```rust
// Version mismatch
let tls_config = TlsConfig {
    min_version: TlsVersion::V1_3,
    max_version: TlsVersion::V1_3,
    ..Default::default()
};

// Cipher suite mismatch
let tls_config = TlsConfig {
    cipher_suites: vec![CipherSuite::AES_128_GCM_SHA256],
    ..Default::default()
};
```

**Resolution**:
- Check TLS version compatibility
- Verify cipher suite support
- Ensure both client and server support compatible TLS settings
- Review TLS configuration on both sides

## Serialization Errors

### SERDE_SERIALIZATION_FAILED

**Error Code**: `SERDE_SERIALIZATION_FAILED`  
**HTTP Status**: 422  
**Description**: Failed to serialize request or response data.

```rust
#[derive(Serialize, Deserialize)]
struct InvalidStruct {
    // This will fail serialization with circular references
    #[serde(skip)]
    self_ref: Option<Box<InvalidStruct>>,
}

// Problematic data
let mut data = InvalidStruct { self_ref: None };
data.self_ref = Some(Box::new(data)); // Circular reference
```

**Resolution**:
- Fix circular references in data structures
- Implement custom serialization for complex types
- Use appropriate serde attributes
- Validate data structure design

**Prevention**:
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SafeStruct {
    pub id: u64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_field: Option<String>,
    #[serde(default)]
    pub default_field: Vec<String>,
}

// Test serialization in development
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_serialization() {
        let data = SafeStruct {
            id: 1,
            name: "test".to_string(),
            optional_field: None,
            default_field: vec!["item1".to_string()],
        };
        
        let serialized = serde_json::to_string(&data).unwrap();
        let deserialized: SafeStruct = serde_json::from_str(&serialized).unwrap();
        assert_eq!(data.id, deserialized.id);
    }
}
```

## Resource Errors

### MEMORY_LIMIT_EXCEEDED

**Error Code**: `MEMORY_LIMIT_EXCEEDED`  
**HTTP Status**: 507  
**Description**: Operation exceeded memory limits.

```rust
// Large message handling
const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB

async fn send_large_data(&self, data: Vec<u8>) -> RpcResult<()> {
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(RpcError::ResourceLimitExceeded {
            limit: format!("Message size {} exceeds limit {}", data.len(), MAX_MESSAGE_SIZE)
        });
    }
    // Process data
    Ok(())
}
```

**Resolution**:
- Increase memory limits if appropriate
- Implement data chunking for large transfers
- Use streaming for large datasets
- Optimize data structures for memory efficiency

**Prevention**:
```rust
// Streaming large data
#[rpc_service(name = "DataService")]
trait DataService {
    // Instead of large single message
    // async fn upload_file(&self, data: Vec<u8>) -> RpcResult<()>;
    
    // Use streaming for large data
    async fn upload_file_stream(&self, data: RpcStreamSink<FileChunk>) -> RpcResult<UploadResult>;
}

#[derive(Serialize, Deserialize)]
struct FileChunk {
    chunk_id: u32,
    data: Vec<u8>, // Small chunks (e.g., 64KB)
    is_final: bool,
}
```

### CONNECTION_LIMIT_EXCEEDED

**Error Code**: `CONNECTION_LIMIT_EXCEEDED`  
**HTTP Status**: 503  
**Description**: Server reached maximum connection limit.

```rust
// Server configuration
let server = RpcServer::builder()
    .max_connections(1000)
    .connection_timeout(Duration::from_secs(60))
    .build();
```

**Resolution**:
- Increase connection limits if resources allow
- Implement connection pooling on client side
- Use connection sharing for multiple requests
- Monitor and optimize connection lifecycle

**Prevention**:
```rust
// Client-side connection pooling
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ConnectionPool {
    connections: Arc<Mutex<Vec<Connection>>>,
    max_size: usize,
}

impl ConnectionPool {
    pub async fn get_connection(&self) -> Result<Connection, RpcError> {
        let mut connections = self.connections.lock().await;
        
        if let Some(conn) = connections.pop() {
            if conn.is_healthy().await {
                return Ok(conn);
            }
        }
        
        // Create new connection if under limit
        if connections.len() < self.max_size {
            let transport = QuicTransport::new(TlsConfig::default())?;
            transport.connect("127.0.0.1:50051").await
        } else {
            Err(RpcError::ResourceLimitExceeded {
                limit: "Connection pool exhausted".to_string()
            })
        }
    }
    
    pub async fn return_connection(&self, conn: Connection) {
        let mut connections = self.connections.lock().await;
        if connections.len() < self.max_size {
            connections.push(conn);
        }
    }
}
```

## Error Handling Patterns

### Structured Error Handling

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Validation error: {field}: {message}")]
    Validation { field: String, message: String },
    
    #[error("Business logic error: {0}")]
    BusinessLogic(String),
    
    #[error("External service error: {service}: {error}")]
    ExternalService { service: String, error: String },
}

impl From<ServiceError> for RpcError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Validation { field, message } => {
                RpcError::InvalidRequest { 
                    message: format!("Invalid field '{}': {}", field, message) 
                }
            }
            ServiceError::Database(_) => {
                RpcError::ServiceError { 
                    message: "Internal database error".to_string() 
                }
            }
            ServiceError::BusinessLogic(msg) => {
                RpcError::ServiceError { message: msg }
            }
            ServiceError::ExternalService { service, error } => {
                RpcError::ServiceError { 
                    message: format!("External service '{}' error: {}", service, error) 
                }
            }
        }
    }
}
```

### Retry Logic with Exponential Backoff

```rust
use tokio::time::{sleep, Duration};

pub struct RetryConfig {
    pub max_retries: usize,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
        }
    }
}

pub async fn retry_with_backoff<F, T, E>(
    operation: F,
    config: RetryConfig,
) -> Result<T, E>
where
    F: Fn() -> futures::future::BoxFuture<'_, Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut delay = config.base_delay;
    
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt == config.max_retries => return Err(e),
            Err(e) => {
                log::warn!("Attempt {} failed: {:?}, retrying in {:?}", attempt + 1, e, delay);
                sleep(delay).await;
                delay = std::cmp::min(
                    Duration::from_millis((delay.as_millis() as f64 * config.multiplier) as u64),
                    config.max_delay,
                );
            }
        }
    }
    
    unreachable!()
}

// Usage
async fn resilient_rpc_call(client: &Client, request: Request) -> Result<Response, RpcError> {
    retry_with_backoff(
        || Box::pin(client.call_service(request.clone())),
        RetryConfig::default(),
    ).await
}
```

### Circuit Breaker Pattern

```rust
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub enum CircuitState {
    Closed = 0,    // Normal operation
    Open = 1,      // Failing, reject requests
    HalfOpen = 2,  // Testing if service recovered
}

pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU64,
    last_failure_time: AtomicU64,
    failure_threshold: u64,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u64, recovery_timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            last_failure_time: AtomicU64::new(0),
            failure_threshold,
            recovery_timeout,
        }
    }
    
    pub async fn call<F, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> futures::future::BoxFuture<'_, Result<T, E>>,
    {
        let state = self.get_state();
        
        match state {
            CircuitState::Open => {
                let now = Instant::now().elapsed().as_secs();
                let last_failure = self.last_failure_time.load(Ordering::Relaxed);
                
                if now - last_failure > self.recovery_timeout.as_secs() {
                    self.state.store(CircuitState::HalfOpen as u8, Ordering::Relaxed);
                } else {
                    return Err(CircuitBreakerError::CircuitOpen);
                }
            }
            CircuitState::HalfOpen | CircuitState::Closed => {}
        }
        
        match operation().await {
            Ok(result) => {
                self.on_success();
                Ok(result)
            }
            Err(e) => {
                self.on_failure();
                Err(CircuitBreakerError::CallFailed(e))
            }
        }
    }
    
    fn get_state(&self) -> CircuitState {
        match self.state.load(Ordering::Relaxed) {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
    
    fn on_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        self.state.store(CircuitState::Closed as u8, Ordering::Relaxed);
    }
    
    fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_failure_time.store(
            Instant::now().elapsed().as_secs(),
            Ordering::Relaxed,
        );
        
        if failures >= self.failure_threshold {
            self.state.store(CircuitState::Open as u8, Ordering::Relaxed);
        }
    }
}

#[derive(Error, Debug)]
pub enum CircuitBreakerError<E> {
    #[error("Circuit breaker is open")]
    CircuitOpen,
    #[error("Call failed: {0}")]
    CallFailed(E),
}
```

## Monitoring and Alerting

### Error Metrics

```rust
use prometheus::{Counter, Histogram, IntCounter, IntGauge, Registry};

pub struct ErrorMetrics {
    pub error_total: IntCounter,
    pub error_by_type: Counter,
    pub active_connections: IntGauge,
    pub request_duration: Histogram,
}

impl ErrorMetrics {
    pub fn new(registry: &Registry) -> Result<Self, prometheus::Error> {
        let error_total = IntCounter::new(
            "quiccrunch_errors_total",
            "Total number of errors",
        )?;
        
        let error_by_type = Counter::new(
            "quiccrunch_errors_by_type",
            "Errors grouped by type",
        )?;
        
        let active_connections = IntGauge::new(
            "quiccrunch_active_connections",
            "Number of active connections",
        )?;
        
        let request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "quiccrunch_request_duration_seconds",
                "Request duration in seconds",
            ).buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]),
        )?;
        
        registry.register(Box::new(error_total.clone()))?;
        registry.register(Box::new(error_by_type.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;
        
        Ok(Self {
            error_total,
            error_by_type,
            active_connections,
            request_duration,
        })
    }
    
    pub fn record_error(&self, error_type: &str) {
        self.error_total.inc();
        self.error_by_type.with_label_values(&[error_type]).inc();
    }
}
```

### Error Logging

```rust
use tracing::{error, warn, info, span, Level};
use serde_json::json;

pub fn log_rpc_error(error: &RpcError, context: &RpcContext) {
    let span = span!(Level::ERROR, "rpc_error", 
        method = %context.method,
        service = %context.service,
        request_id = %context.request_id,
    );
    
    let _enter = span.enter();
    
    match error {
        RpcError::MethodNotFound { method } => {
            warn!(
                error_type = "method_not_found",
                method = %method,
                "RPC method not found"
            );
        }
        RpcError::Timeout { timeout_ms } => {
            warn!(
                error_type = "timeout",
                timeout_ms = %timeout_ms,
                "RPC request timed out"
            );
        }
        RpcError::ServiceError { message } => {
            error!(
                error_type = "service_error",
                error_message = %message,
                "Internal service error"
            );
        }
        _ => {
            error!(
                error_type = "unknown",
                error = %error,
                "Unhandled RPC error"
            );
        }
    }
}

pub struct RpcContext {
    pub method: String,
    pub service: String,
    pub request_id: String,
    pub client_addr: std::net::SocketAddr,
    pub start_time: std::time::Instant,
}
```

---

This error reference provides comprehensive documentation for diagnosing and resolving QuicCrunch errors. Use the error codes and resolution strategies to quickly identify and fix issues in development and production environments.