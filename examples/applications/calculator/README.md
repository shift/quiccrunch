# Calculator Service Example

This example demonstrates how to build and containerize a calculator service using the QuicCrunch RPC framework.

## Features

- Basic arithmetic operations (add, subtract, multiply, divide)
- Advanced operations (power, square root)
- JSON-based request/response protocol
- Interactive client mode
- Error handling for edge cases (division by zero, negative square roots)
- Proper containerization with health checks

## Building and Running

### Local Development

```bash
# Build the application
cargo build --release

# Start the server
cargo run --bin calculator-server -- --address 127.0.0.1:50051 --verbose

# In another terminal, run the client
cargo run --bin calculator-client -- --interactive
```

### Using Docker

```bash
# Build the container
docker build -t quiccrunch-calculator .

# Run the server
docker run -p 50051:50051/udp quiccrunch-calculator

# Run the client (connecting to server running on host)
docker run --network host quiccrunch-calculator calculator-client --interactive
```

### Docker Compose

```bash
# Use the provided docker-compose.yml
docker-compose up -d calculator-server
docker-compose run calculator-client --interactive
```

## API Reference

### Operations

All operations are sent as JSON over QUIC streams:

```json
{
  "operation": {
    "Add": [5.0, 3.0]
  },
  "id": "unique-request-id"
}
```

**Supported Operations:**
- `Add`: `{"Add": [a, b]}` - Addition
- `Subtract`: `{"Subtract": [a, b]}` - Subtraction  
- `Multiply`: `{"Multiply": [a, b]}` - Multiplication
- `Divide`: `{"Divide": [a, b]}` - Division (handles division by zero)
- `Power`: `{"Power": [base, exponent]}` - Exponentiation
- `Sqrt`: `{"Sqrt": [number]}` - Square root (handles negative numbers)

### Response Format

```json
{
  "result": {
    "Ok": 8.0
  },
  "id": "unique-request-id"
}
```

Error responses:
```json
{
  "result": {
    "Err": "Division by zero"
  },
  "id": "unique-request-id"
}
```

## Client Usage Examples

### Interactive Mode
```bash
calculator-client --interactive
> add 5 3
Result: 8
> sqrt 16
Result: 4
> div 10 0
Calculation error: Division by zero
```

### Command Line
```bash
calculator-client --operation add --a 5 --b 3
calculator-client --operation sqrt --a 16
```

## Performance Characteristics

- **Latency**: Low latency due to QUIC's reduced handshake overhead
- **Throughput**: Supports concurrent calculations via QUIC streams
- **Reliability**: Built-in error handling and connection recovery
- **Security**: Uses QUIC's built-in encryption

## Development Notes

This example showcases:
1. **Service Definition**: How to define RPC operations using Rust enums and serde
2. **Message Handling**: Custom `MessageHandler` implementation for business logic
3. **Client Implementation**: Both interactive and programmatic client usage
4. **Error Handling**: Proper error propagation and user-friendly messages
5. **Containerization**: Production-ready Docker configuration
6. **Testing**: Health checks and validation

## Extending the Example

To add new operations:
1. Add the operation to the `Operation` enum in `src/lib.rs`
2. Implement the calculation logic in `CalculatorRequest::calculate()`
3. Update the client parsing in `src/client.rs`
4. Rebuild and test

## Integration with Other Services

This calculator can be used as a microservice in larger applications:
- **API Gateway**: Put behind a REST/GraphQL gateway
- **Batch Processing**: Use for mathematical computations in data pipelines
- **Real-time Applications**: Integrate with streaming systems
- **Scientific Computing**: Extend for domain-specific calculations