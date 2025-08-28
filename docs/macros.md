# QuicCrunch Macros Guide

QuicCrunch provides powerful procedural macros that eliminate boilerplate and make RPC development ergonomic and type-safe. This guide covers everything you need to know about using QuicCrunch macros effectively.

## Table of Contents

1. [Overview](#overview)
2. [The #[rpc_service] Macro](#the-rpc_service-macro)
3. [Advanced Attributes](#advanced-attributes)
4. [Generated Code Structure](#generated-code-structure)
5. [Error Handling](#error-handling)
6. [Streaming Configuration](#streaming-configuration)
7. [Customization Options](#customization-options)
8. [Best Practices](#best-practices)
9. [Troubleshooting](#troubleshooting)

## Overview

QuicCrunch macros transform simple trait definitions into complete RPC implementations:

```rust
// Input: Simple trait definition
#[rpc_service(name = "Calculator")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
}

// Generated: Complete RPC service + client
// - CalculatorHandler (server-side)
// - CalculatorClient (client-side)
// - Serialization/deserialization
// - Method routing
// - Error handling
```

## The #[rpc_service] Macro

### Basic Usage

The `#[rpc_service]` macro is applied to trait definitions:

```rust
use quiccrunch_macros::rpc_service;
use quiccrunch_core::rpc::{RpcResult, RpcStreamSource};

#[rpc_service]
trait UserService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn create_user(&self, user: CreateUserRequest) -> RpcResult<User>;
    async fn list_users(&self) -> RpcResult<Vec<User>>;
    async fn delete_user(&self, id: u64) -> RpcResult<bool>;
}
```

### Service Naming

By default, the service name matches the trait name. Override it with the `name` attribute:

```rust
#[rpc_service(name = "UserManagementService")]
trait UserService {
    // methods...
}
```

This affects:
- RPC service registration name
- Generated handler name: `UserManagementServiceHandler`
- Generated client name: `UserManagementServiceClient`
- Method dispatch routing

### Supported Method Signatures

#### Unary Methods

```rust
#[rpc_service]
trait Calculator {
    // Simple parameters
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
    
    // Complex types
    async fn calculate(&self, request: CalculationRequest) -> RpcResult<CalculationResponse>;
    
    // Optional parameters
    async fn divide(&self, a: f64, b: f64) -> RpcResult<Option<f64>>;
    
    // Result with custom error
    async fn validate(&self, data: InputData) -> RpcResult<ValidationResult>;
}
```

#### Streaming Methods

```rust
#[rpc_service]
trait StreamingService {
    // Server streaming: one request, many responses
    async fn subscribe_events(&self, filter: EventFilter) -> RpcResult<RpcStreamSource<Event>>;
    
    // Client streaming: many requests, one response
    async fn upload_data(&self, stream: RpcStreamSource<DataChunk>) -> RpcResult<UploadResult>;
    
    // Bidirectional streaming: many requests, many responses
    async fn process_stream(&self, input: RpcStreamSource<Input>) -> RpcResult<RpcStreamSource<Output>>;
}
```

## Advanced Attributes

### Method-Level Attributes

Control individual method behavior with attributes:

```rust
#[rpc_service]
trait AdvancedService {
    // Custom timeout for this method
    #[rpc_method(timeout = "30s")]
    async fn slow_operation(&self, data: LargeData) -> RpcResult<ProcessedData>;
    
    // Custom serialization
    #[rpc_method(serializer = "bincode")]
    async fn binary_data(&self, data: Vec<u8>) -> RpcResult<Vec<u8>>;
    
    // Method aliases
    #[rpc_method(alias = "getUserById")]
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    
    // Deprecation warnings
    #[rpc_method(deprecated = "Use get_user_v2 instead")]
    async fn get_user_legacy(&self, id: u64) -> RpcResult<User>;
}
```

### Streaming Attributes

Configure streaming behavior:

```rust
#[rpc_service]
trait StreamingService {
    // Server streaming with backpressure configuration
    #[rpc_method(
        streaming = "server",
        buffer_size = 1000,
        flow_control = "adaptive"
    )]
    async fn data_feed(&self, request: FeedRequest) -> RpcResult<RpcStreamSource<DataPoint>>;
    
    // Client streaming with batch processing
    #[rpc_method(
        streaming = "client",
        batch_size = 100,
        timeout = "60s"
    )]
    async fn batch_upload(&self, stream: RpcStreamSource<Record>) -> RpcResult<BatchResult>;
    
    // Bidirectional with custom configuration
    #[rpc_method(
        streaming = "bidirectional",
        concurrent_streams = 10,
        heartbeat_interval = "5s"
    )]
    async fn live_processing(&self, input: RpcStreamSource<Request>) -> RpcResult<RpcStreamSource<Response>>;
}
```

### Service-Level Attributes

Configure the entire service:

```rust
#[rpc_service(
    name = "UserService",
    version = "1.2.0",
    namespace = "com.example.users",
    serializer = "json",
    compression = "gzip",
    middleware = ["auth", "metrics", "tracing"]
)]
trait UserService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
}
```

## Generated Code Structure

### Server-Side Generation

For a service trait, the macro generates:

```rust
// Original trait
#[rpc_service(name = "Calculator")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
}

// Generated handler
pub struct CalculatorHandler {
    service: Arc<dyn Calculator + Send + Sync>,
}

impl CalculatorHandler {
    pub fn new(service: impl Calculator + Send + Sync + 'static) -> Self {
        Self {
            service: Arc::new(service),
        }
    }
}

#[async_trait]
impl RpcService for CalculatorHandler {
    async fn call(&self, context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match method {
            "add" => {
                // Deserialize parameters
                let (a, b): (i32, i32) = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                // Call service method
                let result = self.service.add(a, b).await?;
                
                // Serialize response
                let response = serde_json::to_vec(&result)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            _ => Err(RpcError::MethodNotFound(method.to_string()))
        }
    }
}

// Registration helpers
impl CalculatorHandler {
    pub fn register_with_server(self, server: &RpcServer) {
        server.register_service("Calculator".to_string(), Arc::new(self));
    }
}
```

### Client-Side Generation

```rust
// Generated client
pub struct CalculatorClient {
    client: RpcClient,
}

impl CalculatorClient {
    pub fn new(connection: Connection) -> Self {
        Self {
            client: RpcClient::new(connection),
        }
    }
    
    pub fn with_client(client: RpcClient) -> Self {
        Self { client }
    }
    
    pub async fn add(&self, a: i32, b: i32) -> RpcResult<i32> {
        // Serialize parameters
        let params = serde_json::to_vec(&(a, b))
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        // Make RPC call
        let response = self.client.call("Calculator", "add", Bytes::from(params)).await?;
        
        // Deserialize response
        let result = serde_json::from_slice(&response)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(result)
    }
}

// Cloning support for connection pooling
impl Clone for CalculatorClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
        }
    }
}
```

### Streaming Method Generation

For streaming methods, the macro generates specialized implementations:

```rust
// Server streaming method
pub async fn subscribe_events(&self, filter: EventFilter) -> RpcResult<RpcStreamSource<Event>> {
    let params = serde_json::to_vec(&filter)?;
    let receiver = self.client.server_streaming_call("Service", "subscribe_events", Bytes::from(params)).await?;
    
    // Wrap receiver to handle deserialization
    Ok(RpcStreamSource::new(receiver))
}

// Client streaming method
pub async fn upload_data(&self, stream: RpcStreamSource<DataChunk>) -> (RpcStreamSink<DataChunk>, impl Future<Output = RpcResult<UploadResult>>) {
    let (sink, response_future) = self.client.client_streaming_call("Service", "upload_data").await?;
    
    let response_future = async move {
        let response = response_future.await?;
        let result = serde_json::from_slice(&response)?;
        Ok(result)
    };
    
    (RpcStreamSink::new(sink), response_future)
}
```

## Error Handling

### Custom Error Types

You can use custom error types in your service methods:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UserServiceError {
    UserNotFound(u64),
    InvalidEmail(String),
    DatabaseError(String),
}

impl std::fmt::Display for UserServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            UserServiceError::UserNotFound(id) => write!(f, "User {} not found", id),
            UserServiceError::InvalidEmail(email) => write!(f, "Invalid email: {}", email),
            UserServiceError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for UserServiceError {}

// The macro automatically converts your error to RpcError
impl From<UserServiceError> for RpcError {
    fn from(err: UserServiceError) -> Self {
        RpcError::ServiceError(err.to_string())
    }
}

#[rpc_service]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User, UserServiceError>;
}
```

### Error Propagation

The macro handles automatic error conversion:

```rust
impl UserService for UserServiceImpl {
    async fn get_user(&self, id: u64) -> Result<User, UserServiceError> {
        let user = self.database.find_user(id).await
            .map_err(|e| UserServiceError::DatabaseError(e.to_string()))?;
        
        match user {
            Some(user) => Ok(user),
            None => Err(UserServiceError::UserNotFound(id)),
        }
    }
}

// Generated code automatically converts the error
// Result<User, UserServiceError> -> RpcResult<User>
```

## Streaming Configuration

### Server Streaming Configuration

```rust
#[rpc_service]
trait DataService {
    #[rpc_method(
        streaming = "server",
        buffer_size = 1000,          // Internal buffer size
        flow_control = "adaptive",    // Adaptive flow control
        batch_timeout = "100ms",      // Batch timeout
        max_message_size = "1MB"      // Max message size
    )]
    async fn stream_data(&self, request: DataRequest) -> RpcResult<RpcStreamSource<DataPoint>>;
}
```

### Client Streaming Configuration

```rust
#[rpc_service]
trait UploadService {
    #[rpc_method(
        streaming = "client",
        batch_size = 100,             // Batch multiple items
        timeout = "60s",              // Overall timeout
        max_items = 10000,            // Maximum items to accept
        validate_items = true         // Validate each item
    )]
    async fn upload_batch(&self, stream: RpcStreamSource<Item>) -> RpcResult<BatchResult>;
}
```

### Bidirectional Streaming Configuration

```rust
#[rpc_service]
trait ProcessingService {
    #[rpc_method(
        streaming = "bidirectional",
        concurrent_streams = 10,      // Max concurrent processing
        heartbeat_interval = "5s",    // Heartbeat interval
        idle_timeout = "30s",         // Idle timeout
        backpressure_strategy = "block" // Backpressure handling
    )]
    async fn process_stream(&self, input: RpcStreamSource<Input>) -> RpcResult<RpcStreamSource<Output>>;
}
```

## Customization Options

### Serialization

Choose different serializers for different methods:

```rust
#[rpc_service(serializer = "json")] // Default for all methods
trait MixedService {
    // Uses service default (JSON)
    async fn text_data(&self, data: TextData) -> RpcResult<ProcessedText>;
    
    // Override to use binary serialization
    #[rpc_method(serializer = "bincode")]
    async fn binary_data(&self, data: Vec<u8>) -> RpcResult<Vec<u8>>;
    
    // Use MessagePack for structured data
    #[rpc_method(serializer = "msgpack")]
    async fn structured_data(&self, data: StructuredData) -> RpcResult<ProcessedData>;
}
```

### Middleware Integration

Apply middleware to specific methods or entire services:

```rust
#[rpc_service(middleware = ["auth", "metrics"])] // Applied to all methods
trait SecureService {
    // Uses service middleware
    async fn public_method(&self) -> RpcResult<PublicData>;
    
    // Additional middleware for this method
    #[rpc_method(middleware = ["admin_auth", "audit_log"])]
    async fn admin_method(&self, action: AdminAction) -> RpcResult<AdminResult>;
    
    // Skip middleware for this method
    #[rpc_method(middleware = [])]
    async fn health_check(&self) -> RpcResult<HealthStatus>;
}
```

### Custom Naming

Control the names of generated types:

```rust
#[rpc_service(
    name = "UserService",
    handler_name = "UserRpcHandler",
    client_name = "UserRpcClient"
)]
trait UserService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
}

// Generates:
// - UserRpcHandler (instead of UserServiceHandler)
// - UserRpcClient (instead of UserServiceClient)
```

### Documentation Generation

The macro preserves and enhances documentation:

```rust
/// User management service
/// 
/// Provides CRUD operations for user data with authentication
/// and authorization support.
#[rpc_service(name = "UserService")]
trait UserService {
    /// Retrieve a user by ID
    /// 
    /// # Arguments
    /// * `id` - The unique user identifier
    /// 
    /// # Returns
    /// The user data if found, or UserNotFound error
    /// 
    /// # Example
    /// ```
    /// let user = service.get_user(123).await?;
    /// println!("User: {}", user.name);
    /// ```
    async fn get_user(&self, id: u64) -> RpcResult<User>;
}

// Generated client preserves documentation:
impl UserServiceClient {
    /// Retrieve a user by ID
    /// 
    /// # Arguments
    /// * `id` - The unique user identifier
    /// 
    /// # Returns
    /// The user data if found, or UserNotFound error
    pub async fn get_user(&self, id: u64) -> RpcResult<User> {
        // Generated implementation...
    }
}
```

## Best Practices

### 1. Service Design

```rust
// ✅ Good: Focused, cohesive service
#[rpc_service]
trait UserService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn create_user(&self, user: CreateUserRequest) -> RpcResult<User>;
    async fn update_user(&self, id: u64, updates: UpdateUserRequest) -> RpcResult<User>;
    async fn delete_user(&self, id: u64) -> RpcResult<bool>;
}

// ❌ Bad: Too broad, mixed concerns
#[rpc_service]
trait EverythingService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn send_email(&self, email: Email) -> RpcResult<bool>;
    async fn process_payment(&self, payment: Payment) -> RpcResult<Receipt>;
    async fn generate_report(&self, params: ReportParams) -> RpcResult<Report>;
}
```

### 2. Method Signatures

```rust
// ✅ Good: Clear, typed parameters
#[rpc_service]
trait OrderService {
    async fn create_order(&self, request: CreateOrderRequest) -> RpcResult<Order>;
    async fn get_order(&self, order_id: OrderId) -> RpcResult<Option<Order>>;
}

// ❌ Bad: Ambiguous parameters
#[rpc_service]
trait OrderService {
    async fn create_order(&self, data: String) -> RpcResult<String>;
    async fn get_order(&self, id: u64) -> RpcResult<String>;
}
```

### 3. Error Handling

```rust
// ✅ Good: Specific error types
#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
pub enum OrderError {
    #[error("Order {0} not found")]
    NotFound(OrderId),
    #[error("Insufficient inventory: {0}")]
    InsufficientInventory(String),
    #[error("Payment failed: {0}")]
    PaymentFailed(String),
}

#[rpc_service]
trait OrderService {
    async fn create_order(&self, request: CreateOrderRequest) -> Result<Order, OrderError>;
}

// ❌ Bad: Generic error handling
#[rpc_service]
trait OrderService {
    async fn create_order(&self, request: CreateOrderRequest) -> RpcResult<Order>; // Too generic
}
```

### 4. Streaming Design

```rust
// ✅ Good: Appropriate streaming usage
#[rpc_service]
trait LogService {
    // Good: Server streaming for continuous data
    async fn tail_logs(&self, filter: LogFilter) -> RpcResult<RpcStreamSource<LogEntry>>;
    
    // Good: Client streaming for batch uploads
    async fn ingest_logs(&self, logs: RpcStreamSource<LogEntry>) -> RpcResult<IngestResult>;
}

// ❌ Bad: Unnecessary streaming
#[rpc_service]
trait UserService {
    // Bad: Streaming for single item
    async fn get_user(&self, id: u64) -> RpcResult<RpcStreamSource<User>>;
}
```

### 5. Documentation

```rust
// ✅ Good: Comprehensive documentation
/// User management service
/// 
/// Handles user lifecycle operations including creation, retrieval,
/// updates, and deletion. All operations require authentication.
#[rpc_service(name = "UserService")]
trait UserService {
    /// Create a new user account
    /// 
    /// # Arguments
    /// * `request` - User creation data including name, email, and initial settings
    /// 
    /// # Returns
    /// The created user with assigned ID and metadata
    /// 
    /// # Errors
    /// * `UserError::EmailAlreadyExists` - If email is already registered
    /// * `UserError::InvalidEmail` - If email format is invalid
    /// * `UserError::WeakPassword` - If password doesn't meet requirements
    async fn create_user(&self, request: CreateUserRequest) -> Result<User, UserError>;
}
```

## Troubleshooting

### Common Compilation Errors

#### "trait not in scope"

```rust
// Error: trait `Calculator` is not in scope
let handler = CalculatorHandler::new(service);

// Fix: Import the trait
use crate::Calculator;
let handler = CalculatorHandler::new(service);
```

#### "async trait method not found"

```rust
// Error: no method named `add` found for type `&CalculatorImpl`

// Cause: Missing #[async_trait] on implementation
impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32> {
        Ok(a + b)
    }
}

// Fix: Add #[async_trait] (usually auto-generated)
#[async_trait]
impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32> {
        Ok(a + b)
    }
}
```

#### "cannot serialize/deserialize"

```rust
// Error: the trait `Serialize` is not implemented for `CustomType`

// Fix: Derive Serialize and Deserialize
#[derive(Serialize, Deserialize)]
pub struct CustomType {
    // fields...
}
```

### Runtime Errors

#### Method Not Found

```
RpcError::MethodNotFound("unknown_method")
```

**Causes:**
- Method name mismatch between client and server
- Service not properly registered
- Wrong service name in client call

**Fixes:**
```rust
// Ensure method names match exactly
#[rpc_service]
trait MyService {
    async fn my_method(&self) -> RpcResult<String>; // Server
}

// Client call must match exactly
client.my_method().await?; // Not "myMethod" or "my_Method"
```

#### Serialization Errors

```
RpcError::SerializationError("missing field `id`")
```

**Causes:**
- Version mismatch between client and server data structures
- Optional fields not properly handled
- Enum variant changes

**Fixes:**
```rust
// Use versioned structures
#[derive(Serialize, Deserialize)]
pub struct UserV1 {
    pub id: u64,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct UserV2 {
    pub id: u64,
    pub name: String,
    #[serde(default)] // Handle missing fields
    pub email: Option<String>,
}
```

### Performance Issues

#### Generated Code Size

If generated code is too large:

```rust
// Split large services
#[rpc_service]
trait UserReadService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn list_users(&self, filter: UserFilter) -> RpcResult<Vec<User>>;
}

#[rpc_service]
trait UserWriteService {
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
    async fn update_user(&self, id: u64, updates: UpdateUserRequest) -> RpcResult<User>;
    async fn delete_user(&self, id: u64) -> RpcResult<bool>;
}
```

#### Compilation Time

For faster compilation:

```rust
// Use feature flags to conditionally compile large services
#[cfg(feature = "user-service")]
#[rpc_service]
trait UserService {
    // Large service definition...
}
```

### Debugging Generated Code

To see what the macro generates:

```bash
# Expand macros to see generated code
cargo expand --lib

# Or for a specific file
cargo expand --bin my_service
```

This will show the complete generated code for debugging purposes.

## Advanced Patterns

### Service Composition

```rust
// Compose multiple services
#[rpc_service]
trait CompositeService {
    async fn user_operation(&self, request: UserRequest) -> RpcResult<UserResponse>;
    async fn order_operation(&self, request: OrderRequest) -> RpcResult<OrderResponse>;
}

struct CompositeServiceImpl {
    user_service: Arc<dyn UserService + Send + Sync>,
    order_service: Arc<dyn OrderService + Send + Sync>,
}

impl CompositeService for CompositeServiceImpl {
    async fn user_operation(&self, request: UserRequest) -> RpcResult<UserResponse> {
        // Delegate to user service
        self.user_service.handle_request(request).await
    }
    
    async fn order_operation(&self, request: OrderRequest) -> RpcResult<OrderResponse> {
        // Delegate to order service
        self.order_service.handle_request(request).await
    }
}
```

### Generic Services

```rust
// Generic service traits
#[rpc_service]
trait CrudService<T, ID> 
where 
    T: Serialize + DeserializeOwned + Send + Sync,
    ID: Serialize + DeserializeOwned + Send + Sync,
{
    async fn get(&self, id: ID) -> RpcResult<Option<T>>;
    async fn create(&self, item: T) -> RpcResult<T>;
    async fn update(&self, id: ID, item: T) -> RpcResult<T>;
    async fn delete(&self, id: ID) -> RpcResult<bool>;
}

// Concrete implementations
type UserCrudService = CrudService<User, u64>;
type ProductCrudService = CrudService<Product, String>;
```

The QuicCrunch macro system is designed to be powerful yet simple, handling the complexity of RPC implementation while keeping your code clean and maintainable.