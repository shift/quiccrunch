# Getting Started with QuicCrunch

QuicCrunch is a high-performance RPC framework built on QUIC transport, offering native support for all streaming patterns with superior performance compared to traditional HTTP/2-based solutions.

## Why QuicCrunch?

- **🚀 High Performance**: Native QUIC multiplexing with zero head-of-line blocking
- **🌊 Streaming First**: Built-in support for server, client, and bidirectional streaming
- **🦀 Rust Native**: Memory-safe, zero-copy operations with async/await
- **⚡ Zero Configuration**: Sensible defaults, works out of the box
- **🔧 Developer Friendly**: Ergonomic macros and excellent error messages

## Installation

Add QuicCrunch to your `Cargo.toml`:

```toml
[dependencies]
quiccrunch-core = "0.1.0"
quiccrunch-macros = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

## Your First RPC Service

Let's create a simple calculator service that demonstrates all RPC patterns.

### Step 1: Define Your Service

```rust
use quiccrunch_macros::rpc_service;
use quiccrunch_core::rpc::{RpcResult, RpcStreamSource};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Calculation {
    pub operation: String,
    pub operands: Vec<f64>,
    pub result: f64,
    pub timestamp: u64,
}

#[rpc_service(name = "Calculator")]
trait Calculator {
    // Unary RPC: Simple calculation
    async fn add(&self, a: f64, b: f64) -> RpcResult<f64>;
    
    // Server streaming: Generate fibonacci sequence
    async fn fibonacci(&self, count: u32) -> RpcResult<RpcStreamSource<u64>>;
    
    // Client streaming: Sum a stream of numbers
    async fn sum_stream(&self, numbers: RpcStreamSource<f64>) -> RpcResult<f64>;
    
    // Bidirectional: Interactive calculations
    async fn calculate_stream(&self, input: RpcStreamSource<Calculation>) -> RpcResult<RpcStreamSource<Calculation>>;
}
```

### Step 2: Implement Your Service

```rust
use quiccrunch_core::rpc::{RpcStreamSink, RpcStreamSource};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: f64, b: f64) -> RpcResult<f64> {
        Ok(a + b)
    }
    
    async fn fibonacci(&self, count: u32) -> RpcResult<RpcStreamSource<u64>> {
        let (mut sink, source) = RpcStreamSink::create();
        
        tokio::spawn(async move {
            let (mut a, mut b) = (0u64, 1u64);
            
            for i in 0..count {
                let _ = sink.send(if i == 0 { a } else { b }).await;
                
                let temp = a + b;
                a = b;
                b = temp;
                
                // Small delay to demonstrate streaming
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            
            let _ = sink.finish().await;
        });
        
        Ok(source)
    }
    
    async fn sum_stream(&self, mut numbers: RpcStreamSource<f64>) -> RpcResult<f64> {
        let mut sum = 0.0;
        
        while let Some(number_result) = numbers.next().await {
            let number = number_result?;
            sum += number;
        }
        
        Ok(sum)
    }
    
    async fn calculate_stream(&self, mut input: RpcStreamSource<Calculation>) -> RpcResult<RpcStreamSource<Calculation>> {
        let (mut output_sink, output_source) = RpcStreamSink::create();
        
        tokio::spawn(async move {
            while let Some(calc_result) = input.next().await {
                match calc_result {
                    Ok(calc) => {
                        let result = match calc.operation.as_str() {
                            "add" => calc.operands.iter().sum(),
                            "multiply" => calc.operands.iter().product(),
                            "mean" => calc.operands.iter().sum::<f64>() / calc.operands.len() as f64,
                            _ => 0.0,
                        };
                        
                        let response = Calculation {
                            operation: calc.operation,
                            operands: calc.operands,
                            result,
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };
                        
                        if output_sink.send(response).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            
            let _ = output_sink.finish().await;
        });
        
        Ok(output_source)
    }
}
```

### Step 3: Create the Server

```rust
use quiccrunch_core::{QuicTransport, TlsConfig};
use quiccrunch_core::rpc::RpcServer;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Create TLS configuration
    let tls_config = TlsConfig::generate_self_signed("localhost")?;
    
    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Start listening
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    
    // Create RPC server and register service
    let server = RpcServer::new();
    let calculator = Arc::new(CalculatorImpl);
    server.register_service("Calculator".to_string(), calculator).await;
    
    println!("🚀 Calculator RPC server listening on 127.0.0.1:50051");
    
    // Handle incoming connections
    while let Ok(connection) = incoming.accept().await {
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server.handle_connection(connection).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
    
    Ok(())
}
```

### Step 4: Create a Client

```rust
use quiccrunch_core::{QuicTransport, TlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let tls_config = TlsConfig::load_ca_cert("ca.crt")?;
    let transport = QuicTransport::new(tls_config)?;
    let connection = transport.connect("127.0.0.1:50051").await?;
    
    // Create client (generated by #[rpc_service] macro)
    let client = CalculatorClient::new(connection);
    
    // Test unary RPC
    println!("=== Unary RPC ===");
    let result = client.add(5.5, 3.2).await?;
    println!("5.5 + 3.2 = {}", result);
    
    // Test server streaming
    println!("\n=== Server Streaming ===");
    let mut fib_stream = client.fibonacci(10).await?;
    print!("Fibonacci(10): ");
    while let Some(number) = fib_stream.next().await {
        print!("{} ", number?);
    }
    println!();
    
    // Test client streaming
    println!("\n=== Client Streaming ===");
    let (sink, response_future) = client.sum_stream().await?;
    
    // Send numbers to sum
    let numbers = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    for number in numbers {
        sink.send(number).await?;
    }
    sink.finish().await?;
    
    let sum = response_future.await?;
    println!("Sum of [1,2,3,4,5] = {}", sum);
    
    // Test bidirectional streaming
    println!("\n=== Bidirectional Streaming ===");
    let (input_sink, mut output_stream) = client.calculate_stream().await?;
    
    // Send calculations
    tokio::spawn(async move {
        let calculations = vec![
            Calculation {
                operation: "add".to_string(),
                operands: vec![1.0, 2.0, 3.0],
                result: 0.0, // Will be calculated by server
                timestamp: 0,
            },
            Calculation {
                operation: "multiply".to_string(),
                operands: vec![2.0, 3.0, 4.0],
                result: 0.0,
                timestamp: 0,
            },
        ];
        
        for calc in calculations {
            let _ = input_sink.send(calc).await;
        }
        let _ = input_sink.finish().await;
    });
    
    // Receive results
    while let Some(result) = output_stream.next().await {
        let calc = result?;
        println!("{}: {:?} = {}", calc.operation, calc.operands, calc.result);
    }
    
    Ok(())
}
```

## Understanding the Generated Code

The `#[rpc_service]` macro generates several components for you:

### Generated Traits and Implementations

```rust
// Generated service handler
pub struct CalculatorHandler {
    service: Arc<dyn Calculator + Send + Sync>,
}

impl RpcService for CalculatorHandler {
    async fn call(&self, context: &RpcContext, method: &str, params: Bytes) -> RpcResult<Bytes> {
        match method {
            "add" => {
                let (a, b): (f64, f64) = serde_json::from_slice(&params)?;
                let result = self.service.add(a, b).await?;
                Ok(Bytes::from(serde_json::to_vec(&result)?))
            }
            // ... other methods
            _ => Err(RpcError::MethodNotFound(method.to_string()))
        }
    }
}
```

### Generated Client

```rust
// Generated client
pub struct CalculatorClient {
    client: RpcClient,
}

impl CalculatorClient {
    pub fn new(connection: Connection) -> Self {
        Self { 
            client: RpcClient::new(connection)
        }
    }
    
    pub async fn add(&self, a: f64, b: f64) -> RpcResult<f64> {
        let params = serde_json::to_vec(&(a, b))?;
        let result = self.client.call("Calculator", "add", Bytes::from(params)).await?;
        Ok(serde_json::from_slice(&result)?)
    }
    
    // ... other methods including streaming variants
}
```

## Running Your First Service

1. **Start the server** in one terminal:
```bash
cargo run --bin calculator-server
```

2. **Run the client** in another terminal:
```bash
cargo run --bin calculator-client
```

You should see output like:
```
=== Unary RPC ===
5.5 + 3.2 = 8.7

=== Server Streaming ===
Fibonacci(10): 0 1 1 2 3 5 8 13 21 34 

=== Client Streaming ===
Sum of [1,2,3,4,5] = 15

=== Bidirectional Streaming ===
add: [1.0, 2.0, 3.0] = 6
multiply: [2.0, 3.0, 4.0] = 24
```

## Key Concepts

### Service Definition
- Use `#[rpc_service]` to define your RPC interface
- Methods return `RpcResult<T>` for error handling
- Streaming methods use `RpcStreamSource<T>` and `RpcStreamSink<T>`

### Stream Ownership
QuicCrunch uses a split-stream architecture that solves Rust's ownership challenges:
- `RpcStreamSink<T>` for sending data
- `RpcStreamSource<T>` for receiving data
- Both can be passed independently and used in different tasks

### Error Handling
```rust
match client.add(1.0, 2.0).await {
    Ok(result) => println!("Result: {}", result),
    Err(RpcError::MethodNotFound(method)) => {
        eprintln!("Method {} not found", method);
    }
    Err(RpcError::Transport(msg)) => {
        eprintln!("Network error: {}", msg);
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

### Connection Management
- Each `RpcClient` uses one QUIC connection
- QUIC connections can multiplex many concurrent RPC calls
- Connections are automatically managed and cleaned up

## Performance Characteristics

QuicCrunch provides excellent performance out of the box:

- **Latency**: Sub-millisecond for local calls, minimal overhead for remote calls
- **Throughput**: Handles thousands of concurrent streams per connection
- **Memory**: Zero-copy serialization where possible, bounded memory usage
- **CPU**: Efficient async runtime integration, minimal context switching

## Project Structure

Here's a recommended project structure for QuicCrunch services:

```
my-rpc-service/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Service trait definitions
│   ├── service.rs          # Service implementations
│   ├── client.rs           # Client utilities
│   ├── bin/
│   │   ├── server.rs       # Server binary
│   │   └── client.rs       # Client binary
│   └── proto/              # Protocol definitions (if using external schemas)
├── examples/               # Usage examples
└── tests/                  # Integration tests
```

### lib.rs
```rust
pub mod service;
pub mod client;

// Re-export main types
pub use service::*;
pub use client::*;

// Re-export QuicCrunch types
pub use quiccrunch_core::rpc::{RpcResult, RpcError};
pub use quiccrunch_macros::rpc_service;
```

## Common Patterns

### Authentication
```rust
#[rpc_service]
trait AuthenticatedService {
    async fn login(&self, username: String, password: String) -> RpcResult<String>; // Returns token
    
    async fn get_user_data(&self, token: String, user_id: u64) -> RpcResult<UserData>;
}

impl AuthenticatedService for MyService {
    async fn get_user_data(&self, token: String, user_id: u64) -> RpcResult<UserData> {
        // Validate token first
        let user_id_from_token = self.validate_token(&token).await?;
        
        if user_id != user_id_from_token {
            return Err(RpcError::PermissionDenied("Access denied".to_string()));
        }
        
        self.fetch_user_data(user_id).await
    }
}
```

### Configuration
```rust
use quiccrunch_core::QuicTransportConfig;

let mut config = QuicTransportConfig::default();
config.max_concurrent_bidi_streams = 1000u32.into();
config.max_concurrent_uni_streams = 100u32.into();
config.max_idle_timeout = Some(Duration::from_secs(60).try_into()?);

let transport = QuicTransport::with_config(tls_config, config)?;
```

### Health Checks
```rust
#[rpc_service]
trait HealthService {
    async fn health_check(&self) -> RpcResult<HealthStatus>;
    async fn ready_check(&self) -> RpcResult<bool>;
}

#[derive(Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: u64,
    pub dependencies: Vec<DependencyStatus>,
}
```

## Next Steps

Now that you have a basic service running, explore these topics:

1. **[Streaming RPC Tutorial](tutorials/streaming-rpc.md)** - Deep dive into streaming patterns
2. **[Macro Documentation](macros.md)** - Advanced macro usage and configuration
3. **[Performance Optimization](optimization-guide.md)** - Tuning for production
4. **[Deployment Guide](deployment.md)** - Running in production
5. **[Migration Guides](migration/)** - Moving from other RPC frameworks

## Troubleshooting

### Common Issues

**Connection Refused**
```
Error: Transport error: Connection refused
```
- Ensure the server is running
- Check the address and port
- Verify firewall settings

**TLS Errors**
```
Error: TLS handshake failed
```
- Ensure client and server have compatible TLS configurations
- For development, use `TlsConfig::default()` on both sides
- For production, use proper certificates

**Serialization Errors**
```
Error: SerializationError("missing field")
```
- Ensure client and server use the same data structures
- Check that all fields in structs are properly annotated with serde
- Verify version compatibility

**Stream Errors**
```
Error: Stream closed unexpectedly
```
- Always call `sink.finish()` when done sending
- Handle network disconnections gracefully
- Implement proper error handling in stream processing

### Debug Mode

Enable verbose logging for debugging:

```rust
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

This will show detailed information about connections, RPC calls, and streaming operations.

## Getting Help

- **Documentation**: [docs.rs/quiccrunch](https://docs.rs/quiccrunch)
- **Examples**: Check the [examples directory](../quiccrunch-core/examples/)
- **Issues**: [GitHub Issues](https://github.com/your-org/quiccrunch/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/quiccrunch/discussions)

Happy coding with QuicCrunch! 🚀