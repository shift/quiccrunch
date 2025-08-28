# QuicCrunch Documentation

Welcome to QuicCrunch - a high-performance RPC framework built on QUIC transport with native support for all streaming patterns.

## Quick Navigation

### 🚀 Getting Started
- **[Getting Started Guide](getting-started.md)** - Your first QuicCrunch service in 10 minutes
- **[Installation & Setup](getting-started.md#installation)** - Add QuicCrunch to your project

### 📚 Tutorials
- **[Streaming RPC Tutorial](tutorials/streaming-rpc.md)** - Comprehensive guide to all streaming patterns
  - Unary RPCs for simple request/response
  - Server streaming for real-time data feeds
  - Client streaming for batch uploads
  - Bidirectional streaming for interactive applications

### 🔧 Advanced Usage
- **[Procedural Macros Guide](macros.md)** - Master the `#[rpc_service]` macro
- **[Migration from gRPC](migration/grpc.md)** - Step-by-step migration strategies

### 📖 API Reference
- **[API Documentation](https://docs.rs/quiccrunch)** - Complete API reference with examples

## Why Choose QuicCrunch?

### 🏎️ Superior Performance
- **30-50% faster** connection setup than gRPC/HTTP2
- **Zero head-of-line blocking** with native QUIC multiplexing
- **Sub-millisecond latency** for local connections
- **Millions of RPC calls/sec** on modern hardware

### 🦀 Rust-Native Design
- **Type-safe streaming** with no runtime overhead
- **Zero-copy serialization** where possible
- **Native async/await** integration
- **Rich error types** instead of status codes

### 🌊 Streaming First
- **All four streaming patterns** supported natively
- **Split stream architecture** solves Rust ownership challenges
- **Automatic flow control** and backpressure handling
- **Easy composition** of streaming operations

### 🛠️ Developer Experience
- **No protobuf required** - use native Rust types
- **Ergonomic macros** eliminate boilerplate
- **Excellent error messages** with context
- **Comprehensive documentation** with real examples

## Learning Path

### 1. Beginners
Start with the **[Getting Started Guide](getting-started.md)** to:
- Install QuicCrunch
- Create your first service
- Understand basic concepts
- Run complete examples

### 2. Intermediate
Explore **[Streaming RPC Tutorial](tutorials/streaming-rpc.md)** to:
- Master all streaming patterns
- Learn advanced error handling
- Understand performance optimization
- Implement production patterns

### 3. Advanced
Study **[Procedural Macros Guide](macros.md)** to:
- Customize macro behavior
- Implement complex services
- Integrate with middleware
- Debug generated code

### 4. Migration
Use **[gRPC Migration Guide](migration/grpc.md)** to:
- Plan migration strategy
- Convert existing services
- Validate compatibility
- Optimize performance

## Code Examples

### Simple Service Definition
```rust
use quiccrunch_macros::rpc_service;
use quiccrunch_core::rpc::{RpcResult, RpcStreamSource};

#[rpc_service(name = "Calculator")]
trait Calculator {
    // Unary RPC
    async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
    
    // Server streaming
    async fn fibonacci(&self, n: u32) -> RpcResult<RpcStreamSource<u64>>;
}
```

### Running a Server
```rust
let server = RpcServer::new();
server.register_service("Calculator".to_string(), Arc::new(CalculatorImpl)).await;

let transport = QuicTransport::new(TlsConfig::default())?;
server.serve(&transport, "127.0.0.1:50051").await?;
```

### Using a Client
```rust
let transport = QuicTransport::new(TlsConfig::default())?;
let connection = transport.connect("127.0.0.1:50051").await?;
let client = CalculatorClient::new(connection);

let result = client.add(5, 3).await?;
println!("5 + 3 = {}", result);
```

## Key Features

| Feature | Description | Documentation |
|---------|-------------|---------------|
| **Unary RPCs** | Simple request/response pattern | [Tutorial](tutorials/streaming-rpc.md#unary-rpcs) |
| **Server Streaming** | Real-time data feeds, notifications | [Tutorial](tutorials/streaming-rpc.md#server-streaming) |
| **Client Streaming** | File uploads, batch operations | [Tutorial](tutorials/streaming-rpc.md#client-streaming) |
| **Bidirectional Streaming** | Interactive applications, chat | [Tutorial](tutorials/streaming-rpc.md#bidirectional-streaming) |
| **Connection Pooling** | High-throughput scenarios | [API Docs](https://docs.rs/quiccrunch) |
| **Automatic TLS** | Built-in security | [Getting Started](getting-started.md) |
| **Flow Control** | Backpressure handling | [Tutorial](tutorials/streaming-rpc.md#performance-optimization) |
| **Error Recovery** | Circuit breakers, retries | [Tutorial](tutorials/streaming-rpc.md#error-handling) |

## Production Features

### 🔒 Security
- **Automatic TLS 1.3** with QUIC transport
- **Certificate validation** and management
- **Authentication middleware** support
- **Rate limiting** and access control

### 📊 Observability
- **Built-in metrics** and tracing
- **Health check endpoints** 
- **Performance monitoring**
- **Error tracking and logging**

### 🔧 Operations
- **Graceful shutdown** handling
- **Configuration management**
- **Hot reloading** support
- **Deployment automation**

## Community & Support

### 📖 Documentation
- Complete API reference with examples
- Comprehensive tutorials and guides
- Migration assistance from other frameworks
- Best practices for production deployment

### 🐛 Issues & Feedback
- [GitHub Issues](https://github.com/your-org/quiccrunch/issues) for bug reports
- [GitHub Discussions](https://github.com/your-org/quiccrunch/discussions) for questions
- [Examples Repository](https://github.com/your-org/quiccrunch-examples) for more samples

### 🤝 Contributing
- [Contributing Guide](../CONTRIBUTING.md) for development setup
- [Code of Conduct](../CODE_OF_CONDUCT.md) for community guidelines
- [Development Roadmap](../ROADMAP.md) for future features

## Comparison with Other Frameworks

| Feature | QuicCrunch | gRPC | tonic | tarpc |
|---------|------------|------|-------|-------|
| **Transport** | QUIC/UDP | HTTP2/TCP | HTTP2/TCP | TCP/Serde |
| **Head-of-line blocking** | ❌ No | ✅ Yes | ✅ Yes | ✅ Yes |
| **Connection migration** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **0-RTT reconnection** | ✅ Yes | ❌ No | ❌ No | ❌ No |
| **Native Rust types** | ✅ Yes | ❌ No | ❌ No | ✅ Yes |
| **Schema definition** | Rust traits | Protobuf | Protobuf | Rust traits |
| **Streaming patterns** | All 4 | All 4 | All 4 | Limited |
| **Error handling** | Rich types | Status codes | Status codes | Rich types |
| **Performance** | Excellent | Good | Good | Good |

---

**Ready to get started?** Jump into the **[Getting Started Guide](getting-started.md)** and build your first QuicCrunch service in minutes!