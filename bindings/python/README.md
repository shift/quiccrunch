# QuicCrunch Python Bindings

Python bindings for the QuicCrunch high-performance QUIC transport library.

## Features

- **High Performance**: Built on Rust with zero-copy operations
- **Async/Await Support**: Native Python asyncio integration
- **Cap'n Proto**: Schema-based serialization for type safety
- **Cross-Platform**: Works on Linux, macOS, and Windows
- **Memory Safe**: Rust's memory safety guarantees

## Installation

### From Source

```bash
# Install maturin
pip install maturin

# Build and install the Python bindings
cd bindings/python
maturin develop

# Or build wheel
maturin build --release
pip install target/wheels/quiccrunch-*.whl
```

### Requirements

- Python 3.8+
- Rust toolchain (for building from source)

## Quick Start

### Client Example

```python
import asyncio
from quiccrunch import QuicClient

async def main():
    client = QuicClient()
    
    # Connect to server
    connection_id = await client.connect_async("127.0.0.1:50051")
    print(f"Connected with ID: {connection_id}")
    
    # Send a message
    client.send_message(connection_id, "Hello, QuicCrunch!")
    
    # Disconnect
    client.disconnect(connection_id)

asyncio.run(main())
```

### Server Example

```python
import asyncio
from quiccrunch import QuicServer

async def main():
    server = QuicServer()
    
    # Start listening
    server.listen("127.0.0.1:50051")
    print("Server listening on 127.0.0.1:50051")
    
    # Run server
    await server.serve_async()

asyncio.run(main())
```

### Synchronous API

For non-async code, use the blocking methods:

```python
from quiccrunch import QuicClient

client = QuicClient()
connection_id = client.connect("127.0.0.1:50051")
client.send_message(connection_id, b"Hello, World!")
client.disconnect(connection_id)
```

## API Reference

### QuicClient

#### Methods

- `connect(address: str) -> int` - Connect to server (blocking)
- `connect_async(address: str) -> int` - Connect to server (async)
- `send_message(connection_id: int, data: Union[str, bytes]) -> None` - Send message
- `disconnect(connection_id: int) -> None` - Disconnect from server
- `connection_count() -> int` - Get active connection count

### QuicServer

#### Methods

- `listen(address: str) -> None` - Start listening on address
- `serve() -> None` - Run server (blocking)
- `serve_async() -> None` - Run server (async)
- `connection_count() -> int` - Get active connection count

### QuicCrunchError

Exception class for all QuicCrunch errors.

#### Attributes

- `message: str` - Error message
- `error_code: int` - Numeric error code

## Error Handling

```python
from quiccrunch import QuicClient, QuicCrunchError

try:
    client = QuicClient()
    connection_id = client.connect("invalid-address")
except QuicCrunchError as e:
    print(f"Connection failed: {e.message} (code: {e.error_code})")
```

## Performance Tips

1. **Reuse Connections**: Create connections once and reuse them
2. **Use Async**: Prefer async methods for better concurrency
3. **Batch Operations**: Send multiple messages over the same connection
4. **Monitor Connections**: Use `connection_count()` to detect leaks

## Development

### Building

```bash
# Development build
maturin develop

# Release build
maturin build --release

# Build with debugging symbols
maturin develop --cargo-extra-args="--features=debug"
```

### Testing

```bash
# Install test dependencies
pip install pytest pytest-asyncio

# Run tests
pytest tests/
```

### Type Checking

```bash
# Install type checking tools
pip install mypy

# Run type checker
mypy python/quiccrunch/
```

## Examples

See the `examples/` directory for complete working examples:

- `echo_client.py` - Simple echo client
- `echo_server.py` - Simple echo server
- `chat_client.py` - Chat application client
- `chat_server.py` - Chat application server
- `benchmark.py` - Performance benchmarking

## Contributing

1. Follow Python PEP 8 style guidelines
2. Add type hints to all public APIs
3. Include tests for new functionality
4. Update documentation for API changes

## License

Licensed under either of:

- Apache License, Version 2.0
- MIT License

at your option.