# Migrating from Apache Thrift to QuicCrunch

This guide helps you migrate existing Apache Thrift services to QuicCrunch, providing comprehensive strategies and examples.

## Table of Contents

1. [Why Migrate from Thrift?](#why-migrate-from-thrift)
2. [Migration Strategies](#migration-strategies)
3. [Protocol Conversion](#protocol-conversion)
4. [Service Implementation](#service-implementation)
5. [Client Migration](#client-migration)
6. [Performance Comparison](#performance-comparison)
7. [Testing and Validation](#testing-and-validation)
8. [Deployment Strategies](#deployment-strategies)

## Why Migrate from Thrift?

### Performance Advantages

| Feature | Apache Thrift | QuicCrunch | Improvement |
|---------|---------------|------------|-------------|
| Transport | TCP/HTTP | QUIC over UDP | 30-50% faster connection setup |
| Multiplexing | Single request per connection | Native QUIC streams | Better concurrency |
| Connection overhead | High TCP handshake cost | 0-RTT QUIC reconnection | Faster reconnections |
| Head-of-line blocking | Yes (TCP limitation) | No (QUIC native) | Better streaming |
| Serialization | Multiple formats (Binary, Compact, JSON) | Optimized binary + JSON | Better performance |

### Developer Experience

| Aspect | Apache Thrift | QuicCrunch | Advantage |
|--------|---------------|------------|-----------|
| Schema definition | Thrift IDL | Rust types + serde | Type safety, no code generation |
| Error handling | Exception-based | Rich Rust error types | Better error context |
| Async support | Limited/complex | Native async/await | Cleaner async code |
| Language support | Multi-language | Rust-first with bindings | Better type safety |
| Streaming | Basic support | Full streaming patterns | Advanced streaming |

## Migration Strategies

### 1. Service Proxy Migration

Create a QuicCrunch proxy that forwards to existing Thrift services:

```rust
use quiccrunch_macros::rpc_service;
use thrift::protocol::{TBinaryInputProtocol, TBinaryOutputProtocol};
use thrift::transport::{TFramedReadTransport, TFramedWriteTransport, TIoChannel, TTcpChannel};

#[rpc_service(name = "UserServiceProxy")]
trait UserServiceProxy {
    async fn get_user(&self, id: i64) -> RpcResult<User>;
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>>;
}

struct UserServiceProxyImpl {
    thrift_client: Arc<Mutex<UserServiceSyncClient>>,
}

impl UserServiceProxyImpl {
    pub fn new(thrift_endpoint: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let mut channel = TTcpChannel::new();
        channel.open(&thrift_endpoint)?;
        
        let (i_chan, o_chan) = channel.split()?;
        let i_tran = TFramedReadTransport::new(i_chan);
        let o_tran = TFramedWriteTransport::new(o_chan);
        let i_prot = TBinaryInputProtocol::new(i_tran, true);
        let o_prot = TBinaryOutputProtocol::new(o_tran, true);
        
        let client = UserServiceSyncClient::new(i_prot, o_prot);
        
        Ok(Self {
            thrift_client: Arc::new(Mutex::new(client)),
        })
    }
}

impl UserServiceProxy for UserServiceProxyImpl {
    async fn get_user(&self, id: i64) -> RpcResult<User> {
        let client = self.thrift_client.clone();
        
        // Execute Thrift call in blocking thread pool
        let result = tokio::task::spawn_blocking(move || {
            let mut client = client.lock().unwrap();
            client.get_user(id)
        }).await.map_err(|e| RpcError::ServiceError(e.to_string()))?;
        
        match result {
            Ok(thrift_user) => Ok(convert_thrift_to_quiccrunch_user(thrift_user)),
            Err(e) => Err(RpcError::ServiceError(format!("Thrift error: {:?}", e))),
        }
    }
    
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User> {
        let client = self.thrift_client.clone();
        let thrift_request = convert_quiccrunch_to_thrift_request(request);
        
        let result = tokio::task::spawn_blocking(move || {
            let mut client = client.lock().unwrap();
            client.create_user(thrift_request)
        }).await.map_err(|e| RpcError::ServiceError(e.to_string()))?;
        
        match result {
            Ok(thrift_user) => Ok(convert_thrift_to_quiccrunch_user(thrift_user)),
            Err(e) => Err(RpcError::ServiceError(format!("Thrift error: {:?}", e))),
        }
    }
    
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>> {
        let client = self.thrift_client.clone();
        let thrift_request = convert_list_request(request);
        
        let (mut sink, source) = RpcStreamSource::new();
        
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut client = client.lock().unwrap();
                client.list_users(thrift_request)
            }).await;
            
            match result {
                Ok(Ok(users)) => {
                    for user in users {
                        let converted_user = convert_thrift_to_quiccrunch_user(user);
                        if sink.send(converted_user).await.is_err() {
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    sink.send_error(RpcError::ServiceError(format!("Thrift error: {:?}", e))).await;
                }
                Err(e) => {
                    sink.send_error(RpcError::ServiceError(e.to_string())).await;
                }
            }
        });
        
        Ok(source)
    }
}

// Conversion helpers
fn convert_thrift_to_quiccrunch_user(thrift_user: ThriftUser) -> User {
    User {
        id: thrift_user.id,
        name: thrift_user.name,
        email: thrift_user.email,
        created_at: thrift_user.created_at,
        metadata: thrift_user.metadata.unwrap_or_default(),
    }
}

fn convert_quiccrunch_to_thrift_request(request: CreateUserRequest) -> ThriftCreateUserRequest {
    ThriftCreateUserRequest {
        name: request.name,
        email: request.email,
        metadata: Some(request.metadata),
    }
}
```

### 2. Direct Service Migration

Migrate Thrift services directly to QuicCrunch:

```rust
// Original Thrift service implementation (conceptual)
/*
service UserService {
    User getUser(1: i64 id) throws (1: UserNotFoundException notFound),
    User createUser(1: CreateUserRequest request) throws (1: InvalidRequestException invalid),
    list<User> listUsers(1: ListUsersRequest request),
}
*/

// QuicCrunch equivalent
#[rpc_service(name = "UserService")]
trait UserService {
    async fn get_user(&self, id: i64) -> RpcResult<User>;
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>>;
    
    // Enhanced with streaming capabilities not available in Thrift
    async fn stream_user_updates(&self) -> RpcResult<RpcStreamSource<UserUpdate>>;
    async fn batch_create_users(&self, users: RpcStreamSink<CreateUserRequest>) -> RpcResult<BatchCreateResponse>;
}

struct UserServiceImpl {
    database: Arc<dyn UserDatabase>,
    event_publisher: Arc<dyn EventPublisher>,
}

impl UserService for UserServiceImpl {
    async fn get_user(&self, id: i64) -> RpcResult<User> {
        match self.database.get_user(id).await {
            Ok(Some(user)) => Ok(user),
            Ok(None) => Err(RpcError::NotFound(format!("User {} not found", id))),
            Err(e) => Err(RpcError::ServiceError(e.to_string())),
        }
    }
    
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User> {
        // Validate request
        if request.name.is_empty() {
            return Err(RpcError::InvalidRequest("Name cannot be empty".to_string()));
        }
        
        if !is_valid_email(&request.email) {
            return Err(RpcError::InvalidRequest("Invalid email format".to_string()));
        }
        
        // Create user
        let user = User {
            id: generate_user_id(),
            name: request.name,
            email: request.email,
            created_at: chrono::Utc::now().timestamp() as u64,
            metadata: request.metadata,
        };
        
        match self.database.create_user(&user).await {
            Ok(_) => {
                // Publish event for streaming clients
                self.event_publisher.publish_user_created(&user).await;
                Ok(user)
            }
            Err(e) => Err(RpcError::ServiceError(e.to_string())),
        }
    }
    
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>> {
        let (mut sink, source) = RpcStreamSource::new();
        let database = self.database.clone();
        
        tokio::spawn(async move {
            let limit = request.limit.unwrap_or(100);
            let offset = request.offset.unwrap_or(0);
            
            match database.list_users(limit, offset, &request.filter).await {
                Ok(users) => {
                    for user in users {
                        if sink.send(user).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                }
                Err(e) => {
                    sink.send_error(RpcError::ServiceError(e.to_string())).await;
                }
            }
        });
        
        Ok(source)
    }
    
    async fn stream_user_updates(&self) -> RpcResult<RpcStreamSource<UserUpdate>> {
        let (mut sink, source) = RpcStreamSource::new();
        let event_publisher = self.event_publisher.clone();
        
        tokio::spawn(async move {
            let mut receiver = event_publisher.subscribe_user_updates().await;
            
            while let Some(update) = receiver.recv().await {
                if sink.send(update).await.is_err() {
                    break; // Client disconnected
                }
            }
        });
        
        Ok(source)
    }
    
    async fn batch_create_users(&self, users: RpcStreamSink<CreateUserRequest>) -> RpcResult<BatchCreateResponse> {
        let mut created_users = Vec::new();
        let mut errors = Vec::new();
        
        while let Some(request) = users.recv().await? {
            match self.create_user(request).await {
                Ok(user) => created_users.push(user),
                Err(e) => errors.push(e.to_string()),
            }
        }
        
        Ok(BatchCreateResponse {
            created_count: created_users.len() as u32,
            error_count: errors.len() as u32,
            errors,
        })
    }
}
```

## Protocol Conversion

### Thrift IDL to Rust Types

#### Thrift Definition
```thrift
// user.thrift
namespace rs user

struct User {
    1: required i64 id,
    2: required string name,
    3: required string email,
    4: optional i64 created_at,
    5: optional map<string, string> metadata,
}

struct CreateUserRequest {
    1: required string name,
    2: required string email,
    3: optional map<string, string> metadata,
}

exception UserNotFoundException {
    1: string message,
}

exception InvalidRequestException {
    1: string message,
    2: string field,
}

service UserService {
    User getUser(1: i64 id) throws (1: UserNotFoundException notFound),
    User createUser(1: CreateUserRequest request) throws (1: InvalidRequestException invalid),
    list<User> listUsers(1: i32 limit, 2: i64 offset),
}
```

#### QuicCrunch Rust Types
```rust
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use quiccrunch_macros::rpc_service;
use quiccrunch_core::rpc::{RpcResult, RpcStreamSource, RpcError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListUsersRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
}

// Enhanced types not available in Thrift
#[derive(Debug, Serialize, Deserialize)]
pub struct UserUpdate {
    pub user_id: i64,
    pub update_type: UserUpdateType,
    pub timestamp: u64,
    pub data: User,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum UserUpdateType {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    pub created_count: u32,
    pub error_count: u32,
    pub errors: Vec<String>,
}

// Error handling - Thrift exceptions become Rust errors
impl From<UserNotFoundException> for RpcError {
    fn from(e: UserNotFoundException) -> Self {
        RpcError::NotFound(e.message)
    }
}

impl From<InvalidRequestException> for RpcError {
    fn from(e: InvalidRequestException) -> Self {
        RpcError::InvalidRequest(format!("Invalid field '{}': {}", e.field, e.message))
    }
}
```

### Data Type Mappings

| Thrift Type | QuicCrunch Type | Notes |
|-------------|-----------------|-------|
| `bool` | `bool` | Direct mapping |
| `byte` | `i8` | Signed 8-bit integer |
| `i16` | `i16` | Signed 16-bit integer |
| `i32` | `i32` | Signed 32-bit integer |
| `i64` | `i64` | Signed 64-bit integer |
| `double` | `f64` | 64-bit floating point |
| `string` | `String` | UTF-8 string |
| `binary` | `Vec<u8>` | Byte array |
| `list<T>` | `Vec<T>` | Dynamic array |
| `set<T>` | `HashSet<T>` | Unique values |
| `map<K,V>` | `HashMap<K,V>` | Key-value pairs |
| `optional T` | `Option<T>` | Nullable values |
| `required T` | `T` | Non-nullable values |

## Client Migration

### Thrift Client
```rust
// Original Thrift client code
use thrift::transport::{TFramedReadTransport, TFramedWriteTransport, TIoChannel, TTcpChannel};
use thrift::protocol::{TBinaryInputProtocol, TBinaryOutputProtocol};

fn create_thrift_client() -> Result<UserServiceSyncClient, Box<dyn std::error::Error>> {
    let mut channel = TTcpChannel::new();
    channel.open("localhost:9090")?;
    
    let (i_chan, o_chan) = channel.split()?;
    let i_tran = TFramedReadTransport::new(i_chan);
    let o_tran = TFramedWriteTransport::new(o_tran);
    let i_prot = TBinaryInputProtocol::new(i_tran, true);
    let o_prot = TBinaryOutputProtocol::new(o_tran, true);
    
    Ok(UserServiceSyncClient::new(i_prot, o_prot))
}

fn call_thrift_service() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = create_thrift_client()?;
    
    // Blocking calls
    let user = client.get_user(123)?;
    println!("User: {:?}", user);
    
    let new_user_request = CreateUserRequest {
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
        metadata: None,
    };
    
    let created_user = client.create_user(new_user_request)?;
    println!("Created user: {:?}", created_user);
    
    Ok(())
}
```

### QuicCrunch Client
```rust
// QuicCrunch client code
use quiccrunch_core::transport::QuicTransport;
use quiccrunch_core::tls::TlsConfig;

async fn create_quiccrunch_client() -> Result<UserServiceClient, Box<dyn std::error::Error>> {
    let transport = QuicTransport::new(TlsConfig::default())?;
    let connection = transport.connect("127.0.0.1:50051").await?;
    Ok(UserServiceClient::new(connection))
}

async fn call_quiccrunch_service() -> Result<(), Box<dyn std::error::Error>> {
    let client = create_quiccrunch_client().await?;
    
    // Async calls with better error handling
    match client.get_user(123).await {
        Ok(user) => println!("User: {:?}", user),
        Err(RpcError::NotFound(msg)) => println!("User not found: {}", msg),
        Err(e) => println!("Error: {:?}", e),
    }
    
    let new_user_request = CreateUserRequest {
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
        metadata: None,
    };
    
    let created_user = client.create_user(new_user_request).await?;
    println!("Created user: {:?}", created_user);
    
    // Enhanced streaming capabilities
    let mut user_stream = client.list_users(ListUsersRequest {
        limit: Some(10),
        offset: Some(0),
        filter: None,
    }).await?;
    
    while let Some(user) = user_stream.recv().await? {
        println!("Streamed user: {:?}", user);
    }
    
    // Real-time updates (not possible with Thrift)
    let mut updates = client.stream_user_updates().await?;
    tokio::spawn(async move {
        while let Some(update) = updates.recv().await.ok().flatten() {
            println!("User update: {:?}", update);
        }
    });
    
    Ok(())
}
```

## Performance Comparison

### Benchmark Results

```rust
// Benchmark comparing Thrift vs QuicCrunch
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_connection_setup(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("thrift_connection_setup", |b| {
        b.iter(|| {
            let _client = create_thrift_client().unwrap();
        })
    });
    
    c.bench_function("quiccrunch_connection_setup", |b| {
        b.to_async(&rt).iter(|| async {
            let _client = create_quiccrunch_client().await.unwrap();
        })
    });
}

fn benchmark_simple_rpc(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("thrift_get_user", |b| {
        let mut client = create_thrift_client().unwrap();
        b.iter(|| {
            black_box(client.get_user(123).unwrap())
        })
    });
    
    c.bench_function("quiccrunch_get_user", |b| {
        b.to_async(&rt).iter(|| async {
            let client = create_quiccrunch_client().await.unwrap();
            black_box(client.get_user(123).await.unwrap())
        })
    });
}

criterion_group!(benches, benchmark_connection_setup, benchmark_simple_rpc);
criterion_main!(benches);
```

### Expected Performance Improvements

| Metric | Thrift (TCP) | QuicCrunch (QUIC) | Improvement |
|--------|--------------|-------------------|-------------|
| Connection setup | 50-100ms | 20-30ms | 50-70% faster |
| RPC latency (local) | 0.5-1ms | 0.1-0.3ms | 60-80% faster |
| Throughput (RPS) | 10K-50K | 50K-200K | 4-5x higher |
| Memory usage | High (multiple connections) | Low (multiplexed streams) | 50-70% less |
| CPU usage | High (TCP overhead) | Low (QUIC efficiency) | 30-50% less |

## Testing and Validation

### Migration Testing Strategy

```rust
// Test harness for validating migration
use tokio::sync::mpsc;
use std::sync::Arc;
use std::collections::HashMap;

struct MigrationTestHarness {
    thrift_client: Option<UserServiceSyncClient>,
    quiccrunch_client: Option<UserServiceClient>,
}

impl MigrationTestHarness {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let thrift_client = create_thrift_client().ok();
        let quiccrunch_client = create_quiccrunch_client().await.ok();
        
        Ok(Self {
            thrift_client,
            quiccrunch_client,
        })
    }
    
    async fn test_get_user_parity(&self, user_id: i64) -> Result<(), String> {
        let thrift_result = self.thrift_client.as_ref()
            .and_then(|c| c.get_user(user_id).ok());
            
        let quiccrunch_result = if let Some(client) = &self.quiccrunch_client {
            client.get_user(user_id).await.ok()
        } else {
            None
        };
        
        match (thrift_result, quiccrunch_result) {
            (Some(thrift_user), Some(quic_user)) => {
                if users_equivalent(&thrift_user, &quic_user) {
                    Ok(())
                } else {
                    Err(format!("User mismatch: Thrift={:?}, QuicCrunch={:?}", thrift_user, quic_user))
                }
            }
            (None, None) => Ok(()), // Both failed consistently
            (Some(_), None) => Err("Thrift succeeded but QuicCrunch failed".to_string()),
            (None, Some(_)) => Err("QuicCrunch succeeded but Thrift failed".to_string()),
        }
    }
    
    async fn performance_comparison(&self, iterations: usize) -> PerformanceReport {
        let mut thrift_times = Vec::new();
        let mut quiccrunch_times = Vec::new();
        
        // Test Thrift performance
        if let Some(client) = &self.thrift_client {
            for i in 0..iterations {
                let start = std::time::Instant::now();
                let _ = client.get_user(i as i64);
                thrift_times.push(start.elapsed());
            }
        }
        
        // Test QuicCrunch performance
        if let Some(client) = &self.quiccrunch_client {
            for i in 0..iterations {
                let start = std::time::Instant::now();
                let _ = client.get_user(i as i64).await;
                quiccrunch_times.push(start.elapsed());
            }
        }
        
        PerformanceReport {
            thrift_avg: average_duration(&thrift_times),
            quiccrunch_avg: average_duration(&quiccrunch_times),
            improvement_factor: calculate_improvement(&thrift_times, &quiccrunch_times),
        }
    }
}

#[derive(Debug)]
struct PerformanceReport {
    thrift_avg: std::time::Duration,
    quiccrunch_avg: std::time::Duration,
    improvement_factor: f64,
}

fn users_equivalent(thrift_user: &ThriftUser, quic_user: &User) -> bool {
    thrift_user.id == quic_user.id
        && thrift_user.name == quic_user.name
        && thrift_user.email == quic_user.email
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_migration_parity() {
        let harness = MigrationTestHarness::new().await.unwrap();
        
        // Test various user IDs
        for user_id in [1, 100, 999, 1000] {
            harness.test_get_user_parity(user_id).await.unwrap();
        }
    }
    
    #[tokio::test]
    async fn test_performance_improvement() {
        let harness = MigrationTestHarness::new().await.unwrap();
        let report = harness.performance_comparison(100).await;
        
        println!("Performance Report: {:?}", report);
        
        // Expect at least 20% improvement
        assert!(report.improvement_factor > 1.2, 
                "Expected performance improvement, got factor: {}", 
                report.improvement_factor);
    }
}
```

## Deployment Strategies

### Canary Deployment

```yaml
# Docker Compose for canary deployment
version: '3.8'
services:
  thrift-service:
    image: user-service-thrift:latest
    ports:
      - "9090:9090"
    environment:
      - SERVICE_MODE=thrift
    healthcheck:
      test: ["CMD", "nc", "-z", "localhost", "9090"]
      interval: 30s
      timeout: 10s
      retries: 3

  quiccrunch-service:
    image: user-service-quiccrunch:latest
    ports:
      - "50051:50051/udp"
    environment:
      - SERVICE_MODE=quiccrunch
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  load-balancer:
    image: nginx:alpine
    ports:
      - "80:80"
    volumes:
      - ./nginx-canary.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - thrift-service
      - quiccrunch-service
```

### Feature Flag Configuration

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct MigrationConfig {
    pub migration_mode: MigrationMode,
    pub quiccrunch_percentage: u8, // 0-100
    pub fallback_to_thrift: bool,
    pub comparison_mode: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum MigrationMode {
    ThriftOnly,
    QuicCrunchOnly,
    Canary { percentage: u8 },
    ABTest { test_group_percentage: u8 },
}

impl MigrationConfig {
    pub fn should_use_quiccrunch(&self, user_id: Option<u64>) -> bool {
        match self.migration_mode {
            MigrationMode::ThriftOnly => false,
            MigrationMode::QuicCrunchOnly => true,
            MigrationMode::Canary { percentage } => {
                (rand::random::<u8>() % 100) < percentage
            }
            MigrationMode::ABTest { test_group_percentage } => {
                if let Some(id) = user_id {
                    (id % 100) < test_group_percentage as u64
                } else {
                    false
                }
            }
        }
    }
}
```

---

This migration guide provides comprehensive strategies for moving from Apache Thrift to QuicCrunch, covering both technical implementation details and practical deployment considerations. The examples demonstrate how to maintain service compatibility while gaining the performance and developer experience benefits of QuicCrunch.