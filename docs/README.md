# QuicCrunch Documentation

Welcome to QuicCrunch - a high-performance RPC framework built on QUIC transport with native support for all streaming patterns.

## Quick Navigation

### 🚀 Getting Started
- **[Getting Started Guide](getting-started.md)** - Your first QuicCrunch service in 10 minutes
- **[Installation & Setup](setup.md)** - Complete installation and environment setup
- **[Quick Reference](quick-reference.md)** - Essential API patterns and examples

### 📚 Tutorials & Guides
- **[Streaming RPC Tutorial](tutorials/streaming-rpc.md)** - Comprehensive guide to all streaming patterns
- **[Procedural Macros Guide](macros.md)** - Master the `#[rpc_service]` macro
- **[Performance Optimization](OPTIMIZATION_GUIDE.md)** - Maximize throughput and minimize latency
- **[Testing Guide](testing.md)** - Unit testing, integration testing, and benchmarking

### 🔄 Migration Guides
- **[Migration Hub](migration/README.md)** - Migration strategies and framework comparisons
- **[From gRPC](migration/grpc.md)** - Step-by-step gRPC to QuicCrunch migration
- **[From Apache Thrift](migration/thrift.md)** - Thrift to QuicCrunch migration guide
- **[From JSON-RPC & Others](migration/README.md)** - Additional framework migrations

### 🚀 Production & Operations
- **[Operations Guide](operations/README.md)** - Complete production deployment guide
- **[Deployment](operations/deployment.md)** - Docker, Kubernetes, and cloud deployment
- **[Troubleshooting](operations/troubleshooting.md)** - Diagnostic procedures and common issues
- **[Monitoring](operations/monitoring.md)** - Metrics, logging, and observability

### 📖 API Reference
- **[Auto-Generated API Docs](api-generation/README.md)** - Comprehensive API documentation system
- **[Error Reference](reference/errors.md)** - Complete error codes and resolution guide
- **[Rust API Documentation](https://docs.rs/quiccrunch)** - Native Rust API reference

### 💡 Examples & Use Cases
- **[Production Examples](examples/README.md)** - Real-world implementation patterns
- **[Performance Examples](examples/high-throughput.md)** - High-performance use cases
- **[Enterprise Patterns](examples/microservices.md)** - Large-scale architecture examples

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

### 🔒 Security & Reliability
- **Automatic TLS 1.3** with QUIC transport security
- **Certificate management** and validation
- **Authentication middleware** support  
- **Rate limiting** and access control
- **Circuit breaker** patterns for resilience

### 📊 Observability & Monitoring
- **Built-in metrics** with Prometheus integration
- **Distributed tracing** with OpenTelemetry support
- **Health check endpoints** for orchestration
- **Performance monitoring** and alerting
- **Structured logging** with correlation IDs

### 🔧 Operations & Deployment
- **Container-ready** with Docker and Kubernetes manifests
- **Auto-scaling** based on metrics and load
- **Graceful shutdown** handling
- **Configuration management** with environment-based configs
- **CI/CD integration** with automated testing

### 🌐 Enterprise Features
- **Service discovery** and load balancing
- **API versioning** and backward compatibility
- **Multi-region deployment** with connection migration
- **Compliance tools** for SOC2, GDPR, etc.
- **Professional support** and training available

## Documentation Structure

```
docs/
├── 📋 Core Documentation
│   ├── getting-started.md          # Quick start guide
│   ├── setup.md                    # Installation and setup
│   ├── macros.md                   # Procedural macros guide
│   ├── testing.md                  # Testing strategies
│   └── OPTIMIZATION_GUIDE.md       # Performance optimization
│
├── 📚 Tutorials
│   └── tutorials/
│       └── streaming-rpc.md        # Comprehensive streaming guide
│
├── 🔄 Migration Guides
│   └── migration/
│       ├── README.md               # Migration hub
│       ├── grpc.md                 # gRPC migration
│       └── thrift.md               # Apache Thrift migration
│
├── 🚀 Operations & Production
│   └── operations/
│       ├── README.md               # Operations overview
│       ├── deployment.md           # Production deployment
│       ├── troubleshooting.md      # Diagnostic procedures
│       └── monitoring.md           # Observability setup
│
├── 📖 API Reference
│   ├── api-generation/             # Auto-generated docs system
│   └── reference/
│       └── errors.md               # Complete error reference
│
└── 💡 Examples & Patterns
    └── examples/
        ├── README.md               # Production use cases
        ├── high-throughput.md      # Performance examples
        └── microservices.md        # Enterprise patterns
```

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

## Community & Support

### 📖 Comprehensive Documentation
- **200+ pages** of detailed documentation and guides
- **Auto-generated API docs** with live examples
- **Migration assistance** from all major RPC frameworks  
- **Production best practices** and deployment patterns
- **Real-world examples** and enterprise use cases

### 🐛 Issues & Feedback
- [GitHub Issues](https://github.com/your-org/quiccrunch/issues) for bug reports and feature requests
- [GitHub Discussions](https://github.com/your-org/quiccrunch/discussions) for questions and community support
- [Documentation Site](https://docs.quiccrunch.dev) for searchable docs and tutorials

### 🤝 Contributing
- [Contributing Guide](../CONTRIBUTING.md) for development setup and guidelines
- [Code of Conduct](../CODE_OF_CONDUCT.md) for community standards
- [Development Roadmap](../ROADMAP.md) for planned features and releases

### 💼 Enterprise Support
- **Professional consulting** for large-scale migrations
- **Training programs** for development teams
- **Priority support** with SLA guarantees
- **Custom feature development** for enterprise needs

---

**Ready to get started?** Jump into the **[Getting Started Guide](getting-started.md)** and build your first QuicCrunch service in minutes!

**Need to migrate?** Check out our **[Migration Hub](migration/README.md)** for comprehensive migration strategies from any RPC framework.

**Going to production?** Follow our **[Operations Guide](operations/README.md)** for deployment, monitoring, and troubleshooting.