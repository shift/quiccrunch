# QuicCrunch Containerization Examples

This directory provides comprehensive containerization examples and testing infrastructure for the QuicCrunch RPC framework. QuicCrunch is a high-performance RPC library (similar to gRPC) that developers use to build their own services.

## 🎯 Overview

This collection demonstrates:
- **Example Applications**: How to containerize applications built with QuicCrunch
- **Development Environment**: Containers for contributors and developers
- **Testing Infrastructure**: Comprehensive testing setup for the library
- **Integration Testing**: Multi-service communication examples
- **Performance Benchmarking**: Tools for performance validation

## 📁 Directory Structure

```
examples/
├── applications/           # Example QuicCrunch-based applications
│   ├── calculator/        # Mathematical RPC service
│   ├── chat-service/      # Multi-user chat application
│   ├── echo-service/      # Simple echo service
│   ├── file-transfer/     # File transfer service (future)
│   └── streaming-chat/    # Streaming chat example (future)
├── containers/            # Development and testing infrastructure
│   ├── development/       # Contributor development environment
│   ├── testing/          # Integration testing tools
│   └── benchmarking/     # Performance testing tools
├── docker-compose.yml     # Multi-service development setup
├── docker-compose.ci.yml  # CI/CD optimized configuration
└── README.md             # This file
```

## 🚀 Quick Start

### Prerequisites

- Docker and Docker Compose
- Git (for cloning the repository)

### Run Example Services

```bash
# Clone the repository
git clone https://github.com/your-org/quiccrunch.git
cd quiccrunch/examples

# Start all example services
docker-compose up -d

# Check service health
docker-compose ps

# Test the calculator service
docker-compose exec calculator-server calculator-client --operation add --a 5 --b 3

# Test the chat service interactively
docker-compose run --rm chat-server chat-client --user alice --interactive

# Test the echo service
docker-compose exec echo-server echo-client --message "Hello QuicCrunch!"
```

### Development Environment

```bash
# Start development container
docker-compose --profile dev up -d dev-tools

# Enter development environment
docker-compose exec dev-tools bash

# Inside the container:
# - All QuicCrunch source code is mounted at /workspace
# - Rust toolchain with development tools is pre-installed
# - Helper scripts are available in PATH

# Run all tests
test-all.sh

# Format and lint code
lint.sh

# Run benchmarks
bench-all.sh
```

## 📖 Example Applications

### Calculator Service

A mathematical RPC service demonstrating:
- JSON-based request/response protocol
- Error handling (division by zero, invalid operations)
- Concurrent request processing
- Interactive and command-line clients

**Features:**
- Basic operations: add, subtract, multiply, divide
- Advanced operations: power, square root
- Request/response correlation with unique IDs
- Comprehensive error handling

**Usage:**
```bash
# Start calculator server
docker run -p 50051:50051/udp quiccrunch-calculator

# Use calculator client
docker run --network host quiccrunch-calculator calculator-client --interactive
```

See [Calculator README](applications/calculator/README.md) for detailed documentation.

### Chat Service

A multi-user chat application demonstrating:
- Room-based chat system
- User management
- Message history
- Concurrent multi-user support

**Features:**
- Join/leave chat rooms
- Send messages to rooms
- List active rooms and users
- Message history with pagination
- Room size limits and management

**Usage:**
```bash
# Start chat server
docker run -p 50052:50051/udp quiccrunch-chat-service

# Connect with multiple users
docker run --network host quiccrunch-chat-service chat-client --user alice --interactive
docker run --network host quiccrunch-chat-service chat-client --user bob --interactive
```

### Echo Service

A simple echo service demonstrating:
- Minimal QuicCrunch application structure
- Basic request/response pattern
- Container health checks

**Usage:**
```bash
# Start echo server
docker run -p 50053:50051/udp quiccrunch-echo-service

# Send echo requests
docker run --network host quiccrunch-echo-service echo-client --message "Hello!"
```

## 🧪 Testing Infrastructure

### Integration Tests

Run comprehensive integration tests across all services:

```bash
# Run all integration tests
docker-compose --profile test up integration-tests

# Run specific test suites
docker-compose run integration-tests cargo test calculator_integration
docker-compose run integration-tests cargo test chat_integration
docker-compose run integration-tests cargo test multi_service_integration
```

**Test Coverage:**
- Individual service functionality
- Multi-service workflows
- Concurrent operation handling
- Error conditions and edge cases
- Performance characteristics

### Performance Benchmarking

Run performance tests to validate service characteristics:

```bash
# Run performance benchmarks
docker-compose --profile benchmark up benchmark

# View benchmark results
docker-compose run benchmark cat /results/benchmark-summary.md
```

**Benchmarks Include:**
- Latency measurements (mean, median, 95th percentile)
- Throughput testing under load
- Concurrent connection handling
- Memory usage stability
- Connection reuse benefits

### CI/CD Testing

For automated testing in CI/CD pipelines:

```bash
# Run CI/CD test suite
docker-compose -f docker-compose.ci.yml --profile test up --abort-on-container-exit

# Run code quality checks
docker-compose -f docker-compose.ci.yml --profile quality up code-quality

# Generate documentation
docker-compose -f docker-compose.ci.yml --profile docs up docs-generation
```

## 🛠️ Development Environment

The development container provides:

**Pre-installed Tools:**
- Rust toolchain with latest stable version
- Cargo extensions: watch, edit, audit, deny, outdated
- Development utilities: ripgrep, fd-find, flamegraph
- Documentation tools: pandoc
- Container tools: docker, docker-compose

**Helper Scripts:**
- `test-all.sh`: Run all tests (unit + integration)
- `bench-all.sh`: Run all benchmarks
- `lint.sh`: Format code and run linting

**Usage:**
```bash
# Start development environment
docker-compose --profile dev up -d dev-tools

# Attach to development container
docker-compose exec dev-tools bash

# Code is mounted at /workspace
cd /workspace

# Make changes and test
cargo build
test-all.sh
```

## 📊 Performance Characteristics

QuicCrunch applications typically demonstrate:

- **Low Latency**: Sub-100ms for simple operations
- **High Throughput**: 1000+ requests/second per service
- **Concurrent Streams**: Efficient multiplexing over single connections
- **Connection Reuse**: Significant performance benefits from connection pooling

Actual performance depends on:
- Service complexity
- Network conditions
- Hardware specifications
- Concurrent load

## 🔧 Creating New Applications

To create a new QuicCrunch-based application:

1. **Create Application Structure:**
```bash
mkdir examples/applications/my-service
cd examples/applications/my-service
```

2. **Create Cargo.toml:**
```toml
[package]
name = "quiccrunch-my-service"
version = "0.1.0"
edition = "2021"

[dependencies]
quiccrunch-core = { path = "../../../quiccrunch-core" }
tokio = { version = "1.0", features = ["full"] }
# Add other dependencies as needed
```

3. **Implement Service Logic:**
```rust
use quiccrunch_core::{QuicServer, MessageHandler, Result};

#[derive(Clone)]
struct MyServiceHandler;

impl MessageHandler for MyServiceHandler {
    type Response = bytes::Bytes;
    
    async fn handle(&self, message: bytes::Bytes) -> bytes::Bytes {
        // Implement your service logic here
        format!("Processed: {}", String::from_utf8_lossy(&message)).into()
    }
}
```

4. **Create Dockerfile:**
```dockerfile
# Use the calculator service Dockerfile as a template
# Modify paths and binary names as needed
```

5. **Add to docker-compose.yml:**
```yaml
my-service:
  build:
    context: ./applications/my-service
    dockerfile: Dockerfile
  ports:
    - "50054:50051/udp"
  # Add health checks and other configuration
```

6. **Create Integration Tests:**
```rust
// Add tests in tests/integration/tests/my_service_integration.rs
```

## 🐛 Troubleshooting

### Common Issues

**Service Won't Start:**
- Check port availability: `netstat -ln | grep 50051`
- Verify Docker daemon is running
- Check logs: `docker-compose logs service-name`

**Connection Refused:**
- Ensure service health checks pass: `docker-compose ps`
- Verify firewall settings for UDP ports
- Check network configuration

**Tests Failing:**
- Ensure all services are healthy before running tests
- Check service logs for errors
- Verify environment variables are set correctly

**Performance Issues:**
- Monitor resource usage: `docker stats`
- Check for port conflicts
- Verify network configuration

### Debug Mode

Enable debug logging:
```bash
export RUST_LOG=debug
docker-compose up
```

View detailed logs:
```bash
docker-compose logs -f service-name
```

## 📚 Additional Resources

- [QuicCrunch Core Documentation](../../quiccrunch-core/README.md)
- [API Examples](../../quiccrunch-core/examples/README.md)
- [Performance Testing Guide](../../docs/PERFORMANCE_TESTING.md)
- [Deployment Guide](../../docs/operations/deployment.md)

## 🤝 Contributing

When adding new examples or improving existing ones:

1. Follow existing patterns and conventions
2. Include comprehensive tests
3. Add proper documentation
4. Ensure Docker health checks work correctly
5. Update this README with new examples

## 📄 License

This project is licensed under either of:
- Apache License, Version 2.0
- MIT License

at your option.