# Migrating from gRPC to QuicCrunch

This guide helps you migrate existing gRPC services to QuicCrunch, highlighting the benefits and providing step-by-step migration strategies.

## Why Migrate to QuicCrunch?

### Performance Advantages

| Feature | gRPC | QuicCrunch | Improvement |
|---------|------|------------|-------------|
| Transport | HTTP/2 over TCP | QUIC over UDP | 30-50% faster connection setup |
| Head-of-line blocking | Yes (HTTP/2 limitation) | No (QUIC native) | Better streaming performance |
| Connection migration | No | Yes | Mobile/WiFi handoff support |
| 0-RTT data | Limited | Full support | Faster reconnections |
| Multiplexing overhead | HTTP/2 frames | Native QUIC streams | Lower CPU usage |

### Developer Experience

| Aspect | gRPC | QuicCrunch | Advantage |
|--------|------|------------|-----------|
| Schema definition | Protobuf only | Rust types + serde | Type safety, no code generation |
| Error handling | Status codes | Rich Rust error types | Better error context |
| Async integration | Complex generators | Native async/await | Cleaner code |
| Streaming | Complex state management | Split streams | Easier ownership |

## Migration Strategies

### 1. Gradual Migration (Recommended)

Migrate services incrementally while maintaining compatibility:

```rust
// Start with a proxy service that forwards to existing gRPC
#[rpc_service(name = "UserServiceProxy")]
trait UserServiceProxy {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
}

struct UserServiceProxyImpl {
    grpc_client: UserServiceGrpcClient,
}

impl UserServiceProxy for UserServiceProxyImpl {
    async fn get_user(&self, id: u64) -> RpcResult<User> {
        // Forward to existing gRPC service during migration
        let grpc_response = self.grpc_client.get_user(GetUserRequest { id }).await
            .map_err(|e| RpcError::ServiceError(e.to_string()))?;
        
        // Convert gRPC response to QuicCrunch format
        Ok(User {
            id: grpc_response.id,
            name: grpc_response.name,
            email: grpc_response.email,
            created_at: grpc_response.created_at,
        })
    }
    
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User> {
        // Gradually migrate methods one by one
        // This method could be implemented natively in QuicCrunch
        let user = self.create_user_native(request).await?;
        Ok(user)
    }
}
```

### 2. Parallel Deployment

Run both gRPC and QuicCrunch services simultaneously:

```rust
// Deploy QuicCrunch alongside existing gRPC
async fn deploy_parallel_services() -> Result<(), Box<dyn std::error::Error>> {
    // Start QuicCrunch server on new port
    let quiccrunch_server = RpcServer::new();
    quiccrunch_server.register_service("UserService".to_string(), Arc::new(UserServiceImpl::new())).await;
    
    let transport = QuicTransport::new(TlsConfig::generate_self_signed("localhost")?)?;
    let quiccrunch_handle = tokio::spawn(async move {
        quiccrunch_server.serve(&transport, "0.0.0.0:50052").await
    });
    
    // Keep existing gRPC server running on original port
    let grpc_handle = tokio::spawn(async move {
        // Your existing gRPC server code
        start_grpc_server("0.0.0.0:50051").await
    });
    
    // Gradually switch traffic from gRPC to QuicCrunch
    tokio::try_join!(quiccrunch_handle, grpc_handle)?;
    Ok(())
}
```

### 3. Feature Flag Migration

Use feature flags to control which transport to use:

```rust
use config::Config;

enum TransportMode {
    Grpc,
    QuicCrunch,
    Both, // A/B testing
}

async fn call_user_service(transport: TransportMode, user_id: u64) -> Result<User, Box<dyn std::error::Error>> {
    match transport {
        TransportMode::Grpc => {
            let grpc_client = UserServiceGrpcClient::connect("http://localhost:50051").await?;
            let response = grpc_client.get_user(GetUserRequest { id: user_id }).await?;
            Ok(convert_grpc_user(response.into_inner()))
        }
        TransportMode::QuicCrunch => {
            let transport = QuicTransport::new(TlsConfig::default())?;
            let connection = transport.connect("127.0.0.1:50052").await?;
            let client = UserServiceClient::new(connection);
            client.get_user(user_id).await.map_err(|e| e.into())
        }
        TransportMode::Both => {
            // Call both and compare results for validation
            let (grpc_result, quic_result) = tokio::join!(
                call_user_service(TransportMode::Grpc, user_id),
                call_user_service(TransportMode::QuicCrunch, user_id)
            );
            
            // Log differences for debugging
            if let (Ok(grpc_user), Ok(quic_user)) = (&grpc_result, &quic_result) {
                if grpc_user != quic_user {
                    log::warn!("gRPC/QuicCrunch mismatch for user {}: {:?} vs {:?}", user_id, grpc_user, quic_user);
                }
            }
            
            // Return QuicCrunch result, fallback to gRPC
            quic_result.or(grpc_result)
        }
    }
}
```

## Step-by-Step Migration

### Step 1: Convert Protocol Definitions

#### gRPC (Protobuf)
```protobuf
syntax = "proto3";

service UserService {
  rpc GetUser(GetUserRequest) returns (User);
  rpc CreateUser(CreateUserRequest) returns (User);
  rpc ListUsers(ListUsersRequest) returns (stream User);
  rpc BatchCreateUsers(stream CreateUserRequest) returns (BatchCreateResponse);
  rpc UserChat(stream ChatMessage) returns (stream ChatMessage);
}

message User {
  uint64 id = 1;
  string name = 2;
  string email = 3;
  uint64 created_at = 4;
}

message GetUserRequest {
  uint64 id = 1;
}

message CreateUserRequest {
  string name = 1;
  string email = 2;
}
```

#### QuicCrunch (Rust Types)
```rust
use serde::{Serialize, Deserialize};
use quiccrunch_macros::rpc_service;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub created_at: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListUsersRequest {
    pub limit: Option<u32>,
    pub offset: Option<u64>,
    pub filter: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BatchCreateResponse {
    pub created_users: Vec<User>,
    pub failed_requests: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub user_id: u64,
    pub message: String,
    pub timestamp: u64,
}

#[rpc_service(name = "UserService")]
trait UserService {
    // Unary RPC
    async fn get_user(&self, id: u64) -> RpcResult<Option<User>>;
    
    // Unary RPC
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
    
    // Server streaming
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>>;
    
    // Client streaming
    async fn batch_create_users(&self, requests: RpcStreamSource<CreateUserRequest>) -> RpcResult<BatchCreateResponse>;
    
    // Bidirectional streaming
    async fn user_chat(&self, input: RpcStreamSource<ChatMessage>) -> RpcResult<RpcStreamSource<ChatMessage>>;
}
```

### Step 2: Migrate Service Implementation

#### gRPC Implementation
```rust
use tonic::{Request, Response, Status};
use tokio_stream::wrappers::ReceiverStream;

#[tonic::async_trait]
impl UserService for UserServiceImpl {
    async fn get_user(&self, request: Request<GetUserRequest>) -> Result<Response<User>, Status> {
        let req = request.into_inner();
        
        match self.database.get_user(req.id).await {
            Ok(Some(user)) => Ok(Response::new(user.into())),
            Ok(None) => Err(Status::not_found("User not found")),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }
    
    type ListUsersStream = ReceiverStream<Result<User, Status>>;
    
    async fn list_users(&self, request: Request<ListUsersRequest>) -> Result<Response<Self::ListUsersStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        
        // Complex async streaming setup
        tokio::spawn(async move {
            // Streaming implementation...
        });
        
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
```

#### QuicCrunch Implementation
```rust
impl UserService for UserServiceImpl {
    async fn get_user(&self, id: u64) -> RpcResult<Option<User>> {
        self.database.get_user(id).await
            .map_err(|e| RpcError::ServiceError(e.to_string()))
    }
    
    async fn list_users(&self, request: ListUsersRequest) -> RpcResult<RpcStreamSource<User>> {
        let (mut sink, source) = RpcStreamSink::create();
        let database = self.database.clone();
        
        tokio::spawn(async move {
            let mut offset = request.offset.unwrap_or(0);
            let limit = request.limit.unwrap_or(100).min(1000);
            
            loop {
                match database.get_users(offset, limit).await {
                    Ok(users) => {
                        if users.is_empty() {
                            break;
                        }
                        
                        for user in users {
                            if sink.send(user).await.is_err() {
                                return; // Client disconnected
                            }
                        }
                        
                        offset += limit as u64;
                    }
                    Err(e) => {
                        log::error!("Database error in list_users: {}", e);
                        break;
                    }
                }
            }
            
            let _ = sink.finish().await;
        });
        
        Ok(source)
    }
    
    async fn batch_create_users(&self, mut requests: RpcStreamSource<CreateUserRequest>) -> RpcResult<BatchCreateResponse> {
        let mut created_users = Vec::new();
        let mut failed_requests = Vec::new();
        
        while let Some(request_result) = requests.next().await {
            match request_result {
                Ok(request) => {
                    match self.database.create_user(request.clone()).await {
                        Ok(user) => created_users.push(user),
                        Err(e) => {
                            failed_requests.push(format!("Failed to create user '{}': {}", request.name, e));
                        }
                    }
                }
                Err(e) => {
                    failed_requests.push(format!("Request parsing error: {}", e));
                }
            }
        }
        
        Ok(BatchCreateResponse {
            created_users,
            failed_requests,
        })
    }
    
    async fn user_chat(&self, mut input: RpcStreamSource<ChatMessage>) -> RpcResult<RpcStreamSource<ChatMessage>> {
        let (mut output_sink, output_source) = RpcStreamSink::create();
        let chat_service = self.chat_service.clone();
        
        tokio::spawn(async move {
            while let Some(message_result) = input.next().await {
                match message_result {
                    Ok(message) => {
                        // Process chat message
                        match chat_service.process_message(message).await {
                            Ok(response) => {
                                if output_sink.send(response).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("Chat processing error: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Chat message error: {}", e);
                        break;
                    }
                }
            }
            
            let _ = output_sink.finish().await;
        });
        
        Ok(output_source)
    }
}
```

### Step 3: Migrate Client Code

#### gRPC Client
```rust
use tonic::transport::Channel;

pub struct UserServiceGrpcClient {
    inner: user_service_client::UserServiceClient<Channel>,
}

impl UserServiceGrpcClient {
    pub async fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let channel = Channel::from_static(addr).connect().await?;
        Ok(Self {
            inner: user_service_client::UserServiceClient::new(channel),
        })
    }
    
    pub async fn get_user(&mut self, id: u64) -> Result<User, Box<dyn std::error::Error>> {
        let request = GetUserRequest { id };
        let response = self.inner.get_user(request).await?.into_inner();
        Ok(response)
    }
    
    pub async fn list_users(&mut self) -> Result<Vec<User>, Box<dyn std::error::Error>> {
        let request = ListUsersRequest { limit: Some(100), offset: None, filter: None };
        let mut stream = self.inner.list_users(request).await?.into_inner();
        
        let mut users = Vec::new();
        while let Some(user) = stream.message().await? {
            users.push(user);
        }
        
        Ok(users)
    }
}
```

#### QuicCrunch Client
```rust
use quiccrunch_core::{QuicTransport, TlsConfig};

pub struct UserServiceQuicClient {
    inner: UserServiceClient,
}

impl UserServiceQuicClient {
    pub async fn connect(addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let tls_config = TlsConfig::default();
        let transport = QuicTransport::new(tls_config)?;
        let connection = transport.connect(addr).await?;
        
        Ok(Self {
            inner: UserServiceClient::new(connection),
        })
    }
    
    pub async fn get_user(&self, id: u64) -> Result<Option<User>, Box<dyn std::error::Error>> {
        self.inner.get_user(id).await.map_err(|e| e.into())
    }
    
    pub async fn list_users(&self) -> Result<Vec<User>, Box<dyn std::error::Error>> {
        let request = ListUsersRequest { 
            limit: Some(100), 
            offset: None, 
            filter: None 
        };
        
        let mut stream = self.inner.list_users(request).await?;
        let mut users = Vec::new();
        
        while let Some(user_result) = stream.next().await {
            users.push(user_result?);
        }
        
        Ok(users)
    }
    
    pub async fn batch_create_users(&self, requests: Vec<CreateUserRequest>) -> Result<BatchCreateResponse, Box<dyn std::error::Error>> {
        let (sink, response_future) = self.inner.batch_create_users().await?;
        
        // Send all requests
        for request in requests {
            sink.send(request).await?;
        }
        sink.finish().await?;
        
        // Wait for response
        let response = response_future.await?;
        Ok(response)
    }
}
```

### Step 4: Update Build System

#### Replace gRPC Dependencies

**Before (Cargo.toml)**
```toml
[dependencies]
tonic = "0.10"
prost = "0.12"
tokio = { version = "1.0", features = ["macros", "rt-multi-thread"] }
tokio-stream = "0.1"

[build-dependencies]
tonic-build = "0.10"
```

**After (Cargo.toml)**
```toml
[dependencies]
quiccrunch-core = "0.1.0"
quiccrunch-macros = "0.1.0"
tokio = { version = "1.0", features = ["macros", "rt-multi-thread"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

#### Remove build.rs

**Before (build.rs)**
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/user_service.proto")?;
    Ok(())
}
```

**After**
```rust
// No build.rs needed! Everything is handled by Rust's type system and macros
```

### Step 5: Testing Migration

#### Integration Tests

```rust
#[cfg(test)]
mod migration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_grpc_quiccrunch_compatibility() {
        // Start both servers
        let grpc_server = start_grpc_test_server().await;
        let quiccrunch_server = start_quiccrunch_test_server().await;
        
        // Test same operations on both
        let test_user = CreateUserRequest {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
        };
        
        // Create user via gRPC
        let grpc_client = UserServiceGrpcClient::connect("http://localhost:50051").await.unwrap();
        let grpc_user = grpc_client.create_user(test_user.clone()).await.unwrap();
        
        // Create user via QuicCrunch
        let quic_client = UserServiceQuicClient::connect("127.0.0.1:50052").await.unwrap();
        let quic_user = quic_client.create_user(test_user).await.unwrap();
        
        // Verify both users have same structure (different IDs expected)
        assert_eq!(grpc_user.name, quic_user.name);
        assert_eq!(grpc_user.email, quic_user.email);
    }
    
    #[tokio::test]
    async fn test_streaming_compatibility() {
        // Test that streaming works the same way
        let grpc_users = grpc_client.list_users().await.unwrap();
        let quic_users = quic_client.list_users().await.unwrap();
        
        // Should return equivalent data
        assert_eq!(grpc_users.len(), quic_users.len());
    }
}
```

#### Performance Comparison

```rust
#[tokio::test]
async fn benchmark_migration() {
    use std::time::Instant;
    
    let grpc_client = UserServiceGrpcClient::connect("http://localhost:50051").await.unwrap();
    let quic_client = UserServiceQuicClient::connect("127.0.0.1:50052").await.unwrap();
    
    const NUM_REQUESTS: usize = 1000;
    
    // Benchmark gRPC
    let start = Instant::now();
    for i in 0..NUM_REQUESTS {
        let _ = grpc_client.get_user(i as u64).await;
    }
    let grpc_duration = start.elapsed();
    
    // Benchmark QuicCrunch
    let start = Instant::now();
    for i in 0..NUM_REQUESTS {
        let _ = quic_client.get_user(i as u64).await;
    }
    let quic_duration = start.elapsed();
    
    println!("gRPC: {} requests in {:?} ({:.2} req/sec)", 
        NUM_REQUESTS, grpc_duration, NUM_REQUESTS as f64 / grpc_duration.as_secs_f64());
    println!("QuicCrunch: {} requests in {:?} ({:.2} req/sec)", 
        NUM_REQUESTS, quic_duration, NUM_REQUESTS as f64 / quic_duration.as_secs_f64());
    
    // QuicCrunch should be faster
    assert!(quic_duration < grpc_duration);
}
```

## Common Migration Challenges

### 1. Error Handling Differences

#### gRPC Status Codes
```rust
// gRPC uses status codes
match grpc_result {
    Ok(response) => { /* handle success */ }
    Err(status) => {
        match status.code() {
            tonic::Code::NotFound => { /* handle not found */ }
            tonic::Code::InvalidArgument => { /* handle invalid args */ }
            _ => { /* handle other errors */ }
        }
    }
}
```

#### QuicCrunch Rich Errors
```rust
// QuicCrunch uses rich error types
match quiccrunch_result {
    Ok(response) => { /* handle success */ }
    Err(RpcError::ServiceError(msg)) => { /* handle service errors */ }
    Err(RpcError::InvalidParameters(msg)) => { /* handle validation errors */ }
    Err(RpcError::Transport(msg)) => { /* handle network errors */ }
    _ => { /* handle other errors */ }
}
```

**Solution: Create conversion utilities**
```rust
fn convert_grpc_status_to_rpc_error(status: tonic::Status) -> RpcError {
    match status.code() {
        tonic::Code::NotFound => RpcError::ServiceError("Not found".to_string()),
        tonic::Code::InvalidArgument => RpcError::InvalidParameters(status.message().to_string()),
        tonic::Code::Unavailable => RpcError::ServiceUnavailable(status.message().to_string()),
        _ => RpcError::ServiceError(status.message().to_string()),
    }
}
```

### 2. Streaming State Management

#### gRPC Complex State
```rust
// gRPC requires complex state management
pub struct GrpcStreamState {
    stream: Option<Streaming<Response>>,
    buffer: Vec<Item>,
    is_complete: bool,
}

impl GrpcStreamState {
    async fn next_item(&mut self) -> Option<Result<Item, Status>> {
        // Complex state management code...
    }
}
```

#### QuicCrunch Simple Streams
```rust
// QuicCrunch has simple stream handling
let mut stream = client.get_stream().await?;
while let Some(item) = stream.next().await {
    // Simple iteration, no state management needed
    process_item(item?);
}
```

### 3. Connection Management

#### gRPC Channel Management
```rust
// gRPC requires careful channel management
use tonic::transport::{Channel, Endpoint};
use tower::discover::Change;

let channel = Endpoint::from_static("http://localhost:50051")
    .connect_timeout(Duration::from_secs(5))
    .tcp_keepalive(Some(Duration::from_secs(30)))
    .connect()
    .await?;
```

#### QuicCrunch Automatic Management
```rust
// QuicCrunch handles connection management automatically
let transport = QuicTransport::new(TlsConfig::default())?;
let connection = transport.connect("127.0.0.1:50051").await?;
let client = UserServiceClient::new(connection);

// Connection is automatically managed, including:
// - Keep-alive
// - Reconnection
// - Multiplexing
// - Resource cleanup
```

## Migration Checklist

### Pre-Migration
- [ ] Inventory all gRPC services and their usage
- [ ] Identify critical vs. non-critical services
- [ ] Set up performance baselines
- [ ] Plan rollback strategy
- [ ] Set up monitoring for both protocols

### During Migration
- [ ] Convert Protobuf definitions to Rust types
- [ ] Implement QuicCrunch services
- [ ] Create compatibility tests
- [ ] Deploy in parallel mode
- [ ] Gradual traffic switching
- [ ] Monitor performance and errors

### Post-Migration
- [ ] Remove gRPC dependencies
- [ ] Clean up Protobuf files
- [ ] Update documentation
- [ ] Train team on QuicCrunch patterns
- [ ] Optimize performance based on QuicCrunch features

## Performance Optimization After Migration

### 1. Connection Reuse
```rust
// Use connection pooling for high-throughput scenarios
let pool = RpcConnectionPool::new(vec![
    transport.connect("server1:50051").await?,
    transport.connect("server2:50051").await?,
    transport.connect("server3:50051").await?,
]);

let client = UserServiceClient::new(pool.get_client()?);
```

### 2. Streaming Optimization
```rust
// Use batching for better throughput
async fn batch_process_users(users: Vec<User>) -> RpcResult<()> {
    const BATCH_SIZE: usize = 100;
    
    for chunk in users.chunks(BATCH_SIZE) {
        let (sink, _) = client.batch_create_users().await?;
        
        for user in chunk {
            sink.send(user.clone()).await?;
        }
        sink.finish().await?;
    }
    
    Ok(())
}
```

### 3. Error Recovery
```rust
// Implement circuit breaker pattern
struct CircuitBreaker {
    failure_count: AtomicU32,
    is_open: AtomicBool,
    last_failure: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    async fn call_with_breaker<T>(&self, operation: impl Future<Output = RpcResult<T>>) -> RpcResult<T> {
        if self.is_open() {
            return Err(RpcError::ServiceUnavailable("Circuit breaker open".to_string()));
        }
        
        match operation.await {
            Ok(result) => {
                self.reset();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(e)
            }
        }
    }
}
```

## Conclusion

Migrating from gRPC to QuicCrunch provides significant benefits:

1. **Better Performance**: QUIC's advantages over HTTP/2
2. **Simpler Code**: No protobuf, better Rust integration
3. **Better Streaming**: Native support, easier ownership
4. **Production Ready**: Built-in monitoring, health checks

The migration can be done gradually with minimal risk by using the patterns outlined in this guide.