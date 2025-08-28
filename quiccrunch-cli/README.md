# QuicCrunch CLI

Command-line tools for the QuicCrunch RPC framework, providing code generation, testing, and development utilities.

## Installation

```bash
cargo install quiccrunch-cli
```

Or build from source:

```bash
cargo build --release -p quiccrunch-cli
```

## Commands

### Generate Code

Generate RPC service code from Cap'n Proto schemas:

```bash
quiccrunch generate --input schema.capnp --output src/generated --language rust
```

### Test RPC Services

Test RPC service methods:

```bash
quiccrunch test --endpoint localhost:8080 --service Calculator --method add --params '{"a": 5, "b": 3}'
```

### Call RPC Methods

Make individual RPC calls:

```bash
quiccrunch call localhost:8080 Calculator.add '{"a": 5, "b": 3}'
```

### Benchmark Performance

Benchmark RPC service performance:

```bash
quiccrunch benchmark localhost:8080 Calculator.add '{"a": 1, "b": 2}' --duration 30 --connections 10
```

### Development Server

Start a development server:

```bash
quiccrunch serve --port 8080 --service calculator.rs --watch
```

### Initialize Projects

Create new QuicCrunch projects:

```bash
quiccrunch init my-project --template basic
```

## Examples

### Generate a Calculator Service

1. Create a Cap'n Proto schema (`calculator.capnp`):
```capnp
@0x123456789abcdef0;

interface Calculator {
    add @0 (a: Int32, b: Int32) -> (result: Int32);
    subtract @1 (a: Int32, b: Int32) -> (result: Int32);
}
```

2. Generate Rust code:
```bash
quiccrunch generate -i calculator.capnp -o src/generated -l rust -s Calculator
```

3. Test the service:
```bash
quiccrunch test -e localhost:8080 -s Calculator -m add -p '{"a": 10, "b": 5}'
```

### Performance Testing

Benchmark your service with multiple concurrent connections:

```bash
quiccrunch benchmark localhost:8080 Calculator.add '{"a": 1, "b": 2}' \
    --duration 60 \
    --connections 50 \
    --output json > benchmark_results.json
```

## Configuration

The CLI can be configured with environment variables:

- `QUICCRUNCH_ENDPOINT` - Default endpoint for RPC calls
- `QUICCRUNCH_TIMEOUT` - Default timeout for operations
- `QUICCRUNCH_VERBOSE` - Enable verbose output

## Integration

The CLI integrates seamlessly with:

- Cap'n Proto schema files
- QuicCrunch RPC services  
- Development workflows
- CI/CD pipelines
- Performance monitoring