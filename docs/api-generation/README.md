# API Documentation Generation

This directory contains tools and configurations for generating comprehensive API documentation for QuicCrunch.

## Overview

QuicCrunch uses a multi-layered approach to API documentation:

1. **Rust Documentation** - Generated with `cargo doc` with enhanced rustdoc comments
2. **OpenAPI Specification** - Machine-readable API specifications
3. **Interactive Documentation** - Web-based API explorers
4. **Language Bindings** - Auto-generated client libraries

## Quick Start

```bash
# Generate all documentation
./scripts/generate-docs.sh

# Generate only Rust docs
cargo doc --no-deps --open

# Generate OpenAPI spec
./scripts/generate-openapi.sh

# Start documentation server
./scripts/serve-docs.sh
```

## Documentation Types

### 1. Rust API Documentation

Enhanced rustdoc with comprehensive examples and cross-references.

**Configuration**: `.cargo/config.toml`
```toml
[doc]
all-features = true
rustdoc-args = [
    "--cfg", "docsrs",
    "--generate-link-to-definition",
    "--enable-index-page",
    "-Zunstable-options"
]
```

**Features**:
- Comprehensive trait and struct documentation
- Complete working examples for all public APIs
- Performance characteristics and usage patterns
- Cross-references between related components
- Architecture diagrams and flow charts

### 2. OpenAPI Specification

Machine-readable API specifications for HTTP endpoints and service interfaces.

**Generated Files**:
- `openapi/quiccrunch-api.yaml` - Core API specification
- `openapi/metrics-api.yaml` - Metrics and monitoring endpoints
- `openapi/health-api.yaml` - Health check endpoints

### 3. Interactive Documentation

Web-based documentation with live examples and API testing.

**Components**:
- **Swagger UI** - Interactive API explorer
- **Redoc** - Clean, responsive documentation
- **API Examples** - Live code samples and testing

### 4. Client Libraries

Auto-generated client libraries for multiple languages.

**Supported Languages**:
- Rust (native)
- Python
- JavaScript/TypeScript  
- Go
- Java
- C#

## Documentation Generation

### Automated Generation Script

```bash
#!/bin/bash
# scripts/generate-docs.sh

set -e

echo "🔄 Generating QuicCrunch API Documentation..."

# Clean previous builds
rm -rf target/doc docs/generated

# Create output directories
mkdir -p docs/generated/{rust,openapi,examples}

echo "📚 Generating Rust documentation..."
RUSTDOCFLAGS="--cfg docsrs --enable-index-page -Zunstable-options" \
cargo +nightly doc --all-features --no-deps --document-private-items

# Copy to docs directory
cp -r target/doc/* docs/generated/rust/

echo "🔧 Generating OpenAPI specifications..."
./scripts/generate-openapi.sh

echo "🌐 Generating interactive documentation..."
./scripts/generate-swagger.sh

echo "📖 Generating API examples..."
./scripts/generate-examples.sh

echo "🔗 Generating client libraries..."
./scripts/generate-clients.sh

echo "✅ Documentation generation complete!"
echo "📍 View at: file://$(pwd)/docs/generated/index.html"
```

### Rust Documentation Enhancement

```rust
// src/lib.rs - Enhanced documentation attributes

#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/your-org/quiccrunch/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/your-org/quiccrunch/main/assets/favicon.ico",
    html_root_url = "https://docs.rs/quiccrunch/"
)]

/// # QuicCrunch Core Library
/// 
/// QuicCrunch is a high-performance RPC framework built on QUIC transport.
/// 
/// ## Quick Start
/// 
/// ```rust
/// use quiccrunch_core::{RpcServer, RpcClient};
/// use quiccrunch_macros::rpc_service;
/// 
/// #[rpc_service(name = "Calculator")]
/// trait Calculator {
///     async fn add(&self, a: i32, b: i32) -> RpcResult<i32>;
/// }
/// 
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Start server
/// let server = RpcServer::new();
/// server.register_service("Calculator".to_string(), Arc::new(CalculatorImpl)).await;
/// 
/// // Connect client
/// let client = CalculatorClient::connect("localhost:50051").await?;
/// let result = client.add(5, 3).await?;
/// # Ok(())
/// # }
/// ```
/// 
/// ## Architecture
/// 
/// QuicCrunch is built around several core concepts:
/// 
/// - **[RpcServer]** - Handles incoming connections and dispatches requests
/// - **[RpcClient]** - Manages client connections and request lifecycle  
/// - **[RpcStreamSink]** / **[RpcStreamSource]** - Handle streaming data
/// - **[Transport]** - Pluggable transport layer (QUIC, TCP, etc.)
/// 
/// ## Performance Characteristics
/// 
/// - **Latency**: Sub-millisecond for local connections
/// - **Throughput**: Millions of RPC calls per second
/// - **Concurrency**: Thousands of concurrent streams per connection
/// - **Memory**: Low allocation overhead with zero-copy where possible
/// 
/// ## Examples
/// 
/// See the [examples directory](https://github.com/your-org/quiccrunch/tree/main/examples)
/// for comprehensive usage examples.
pub mod rpc;
```

### OpenAPI Generation

```bash
#!/bin/bash
# scripts/generate-openapi.sh

set -e

echo "🔧 Generating OpenAPI specifications..."

# Generate core API spec
cat > docs/generated/openapi/quiccrunch-api.yaml << 'EOF'
openapi: 3.0.3
info:
  title: QuicCrunch API
  description: High-performance RPC framework built on QUIC transport
  version: 1.0.0
  contact:
    name: QuicCrunch Support
    url: https://github.com/your-org/quiccrunch
    email: support@quiccrunch.dev
  license:
    name: MIT OR Apache-2.0
    url: https://github.com/your-org/quiccrunch/blob/main/LICENSE

servers:
  - url: https://api.quiccrunch.dev
    description: Production server
  - url: http://localhost:50051
    description: Local development server

paths:
  /health:
    get:
      summary: Health check endpoint
      description: Returns the health status of the QuicCrunch server
      operationId: healthCheck
      responses:
        '200':
          description: Service is healthy
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/HealthResponse'
        '503':
          description: Service is unhealthy
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'

  /metrics:
    get:
      summary: Prometheus metrics
      description: Returns metrics in Prometheus format
      operationId: getMetrics
      responses:
        '200':
          description: Metrics data
          content:
            text/plain:
              schema:
                type: string
                example: |
                  # HELP quiccrunch_rpc_requests_total Total number of RPC requests
                  # TYPE quiccrunch_rpc_requests_total counter
                  quiccrunch_rpc_requests_total{method="add",status="success"} 1234

  /rpc:
    post:
      summary: Execute RPC call
      description: Execute a remote procedure call
      operationId: executeRpc
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/RpcRequest'
      responses:
        '200':
          description: RPC call successful
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/RpcResponse'
        '400':
          description: Invalid request
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ErrorResponse'

components:
  schemas:
    HealthResponse:
      type: object
      required:
        - status
        - timestamp
      properties:
        status:
          type: string
          enum: [healthy, unhealthy]
          example: healthy
        timestamp:
          type: string
          format: date-time
          example: "2023-01-01T12:00:00Z"
        version:
          type: string
          example: "1.0.0"
        uptime_seconds:
          type: integer
          example: 86400

    RpcRequest:
      type: object
      required:
        - method
        - params
      properties:
        method:
          type: string
          description: Name of the RPC method to call
          example: "Calculator.add"
        params:
          type: object
          description: Parameters for the RPC method
          example:
            a: 5
            b: 3
        timeout_ms:
          type: integer
          description: Request timeout in milliseconds
          example: 5000

    RpcResponse:
      type: object
      required:
        - result
      properties:
        result:
          description: Result of the RPC call
          example: 8
        error:
          $ref: '#/components/schemas/RpcError'

    RpcError:
      type: object
      required:
        - code
        - message
      properties:
        code:
          type: string
          enum: [INVALID_REQUEST, METHOD_NOT_FOUND, INTERNAL_ERROR, TIMEOUT]
          example: INVALID_REQUEST
        message:
          type: string
          example: "Invalid parameters for method Calculator.add"
        details:
          type: object

    ErrorResponse:
      type: object
      required:
        - error
        - timestamp
      properties:
        error:
          type: string
          example: "Service temporarily unavailable"
        timestamp:
          type: string
          format: date-time
          example: "2023-01-01T12:00:00Z"
        request_id:
          type: string
          example: "req_123456789"

  securitySchemes:
    BearerAuth:
      type: http
      scheme: bearer
      bearerFormat: JWT
    ApiKeyAuth:
      type: apiKey
      in: header
      name: X-API-Key

security:
  - BearerAuth: []
  - ApiKeyAuth: []

tags:
  - name: Health
    description: Health check and status endpoints
  - name: RPC
    description: Remote procedure call execution
  - name: Metrics
    description: Monitoring and metrics endpoints
EOF

echo "✅ OpenAPI specification generated"
```

### Interactive Documentation

```bash
#!/bin/bash
# scripts/generate-swagger.sh

set -e

echo "🌐 Generating interactive documentation..."

# Create Swagger UI
mkdir -p docs/generated/swagger

cat > docs/generated/swagger/index.html << 'EOF'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>QuicCrunch API Documentation</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui.css" />
    <style>
        html {
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }
        *, *:before, *:after {
            box-sizing: inherit;
        }
        body {
            margin:0;
            background: #fafafa;
        }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@4.15.5/swagger-ui-standalone-preset.js"></script>
    <script>
    window.onload = function() {
        const ui = SwaggerUIBundle({
            url: '../openapi/quiccrunch-api.yaml',
            dom_id: '#swagger-ui',
            deepLinking: true,
            presets: [
                SwaggerUIBundle.presets.apis,
                SwaggerUIStandalonePreset
            ],
            plugins: [
                SwaggerUIBundle.plugins.DownloadUrl
            ],
            layout: "StandaloneLayout"
        });
    };
    </script>
</body>
</html>
EOF

# Create Redoc documentation
cat > docs/generated/redoc/index.html << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <title>QuicCrunch API Reference</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link href="https://fonts.googleapis.com/css?family=Montserrat:300,400,700|Roboto:300,400,700" rel="stylesheet">
    <style>
        body { margin: 0; padding: 0; }
    </style>
</head>
<body>
    <redoc spec-url='../openapi/quiccrunch-api.yaml'></redoc>
    <script src="https://cdn.jsdelivr.net/npm/redoc@2.0.0/bundles/redoc.standalone.js"></script>
</body>
</html>
EOF

echo "✅ Interactive documentation generated"
```

### Client Library Generation

```bash
#!/bin/bash
# scripts/generate-clients.sh

set -e

echo "🔗 Generating client libraries..."

# Install OpenAPI Generator if not present
if ! command -v openapi-generator-cli &> /dev/null; then
    echo "Installing OpenAPI Generator..."
    npm install -g @openapitools/openapi-generator-cli
fi

# Create client directories
mkdir -p docs/generated/clients/{python,javascript,go,java,csharp}

# Generate Python client
echo "🐍 Generating Python client..."
openapi-generator-cli generate \
    -i docs/generated/openapi/quiccrunch-api.yaml \
    -g python \
    -o docs/generated/clients/python \
    --additional-properties=packageName=quiccrunch_client,projectName=quiccrunch-client

# Generate JavaScript client
echo "📜 Generating JavaScript client..."
openapi-generator-cli generate \
    -i docs/generated/openapi/quiccrunch-api.yaml \
    -g javascript \
    -o docs/generated/clients/javascript \
    --additional-properties=projectName=quiccrunch-client

# Generate Go client
echo "🐹 Generating Go client..."
openapi-generator-cli generate \
    -i docs/generated/openapi/quiccrunch-api.yaml \
    -g go \
    -o docs/generated/clients/go \
    --additional-properties=packageName=quiccrunch

# Generate TypeScript client  
echo "🔷 Generating TypeScript client..."
openapi-generator-cli generate \
    -i docs/generated/openapi/quiccrunch-api.yaml \
    -g typescript-axios \
    -o docs/generated/clients/typescript \
    --additional-properties=npmName=@quiccrunch/client

echo "✅ Client libraries generated"
```

## Documentation Serving

### Local Development Server

```bash
#!/bin/bash
# scripts/serve-docs.sh

set -e

echo "🌐 Starting documentation server..."

# Create main index page
cat > docs/generated/index.html << 'EOF'
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>QuicCrunch Documentation</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            line-height: 1.6;
            color: #333;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }
        .header {
            text-align: center;
            margin-bottom: 40px;
            padding: 40px;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }
        .docs-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
            margin-bottom: 40px;
        }
        .doc-card {
            background: white;
            padding: 30px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
            transition: transform 0.2s;
        }
        .doc-card:hover {
            transform: translateY(-2px);
        }
        .doc-card h3 {
            margin-top: 0;
            color: #2c3e50;
        }
        .doc-card a {
            color: #3498db;
            text-decoration: none;
            font-weight: 500;
        }
        .doc-card a:hover {
            text-decoration: underline;
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🚀 QuicCrunch Documentation</h1>
        <p>High-performance RPC framework built on QUIC transport</p>
    </div>

    <div class="docs-grid">
        <div class="doc-card">
            <h3>📚 Rust API Documentation</h3>
            <p>Complete Rust API reference with examples and implementation details.</p>
            <a href="rust/quiccrunch/index.html">View Rust Docs →</a>
        </div>

        <div class="doc-card">
            <h3>🔧 Interactive API Explorer</h3>
            <p>Test APIs directly in your browser with Swagger UI.</p>
            <a href="swagger/index.html">Open API Explorer →</a>
        </div>

        <div class="doc-card">
            <h3>📖 API Reference</h3>
            <p>Clean, responsive API documentation with Redoc.</p>
            <a href="redoc/index.html">View API Reference →</a>
        </div>

        <div class="doc-card">
            <h3>🐍 Python Client</h3>
            <p>Auto-generated Python client library with examples.</p>
            <a href="clients/python/README.md">Python Client →</a>
        </div>

        <div class="doc-card">
            <h3>📜 JavaScript Client</h3>
            <p>JavaScript/Node.js client library for web applications.</p>
            <a href="clients/javascript/README.md">JavaScript Client →</a>
        </div>

        <div class="doc-card">
            <h3>🐹 Go Client</h3>
            <p>Go client library for server-side applications.</p>
            <a href="clients/go/README.md">Go Client →</a>
        </div>

        <div class="doc-card">
            <h3>🔷 TypeScript Client</h3>
            <p>Fully typed TypeScript client with excellent IDE support.</p>
            <a href="clients/typescript/README.md">TypeScript Client →</a>
        </div>

        <div class="doc-card">
            <h3>📋 OpenAPI Specification</h3>
            <p>Machine-readable API specifications for tooling integration.</p>
            <a href="openapi/quiccrunch-api.yaml">Download OpenAPI Spec →</a>
        </div>
    </div>

    <div class="doc-card">
        <h3>🔗 Additional Resources</h3>
        <ul>
            <li><a href="../getting-started.md">Getting Started Guide</a></li>
            <li><a href="../tutorials/streaming-rpc.md">Streaming RPC Tutorial</a></li>
            <li><a href="../migration/grpc.md">Migration from gRPC</a></li>
            <li><a href="../operations/README.md">Operations Guide</a></li>
        </ul>
    </div>
</body>
</html>
EOF

# Start HTTP server
if command -v python3 &> /dev/null; then
    echo "📍 Documentation available at: http://localhost:8000"
    cd docs/generated && python3 -m http.server 8000
elif command -v python &> /dev/null; then
    echo "📍 Documentation available at: http://localhost:8000"
    cd docs/generated && python -m SimpleHTTPServer 8000
elif command -v node &> /dev/null; then
    npx http-server docs/generated -p 8000
else
    echo "❌ No suitable HTTP server found. Install Python or Node.js to serve docs."
    exit 1
fi
```

## CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/docs.yml
name: Documentation

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  docs:
    name: Generate Documentation
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: nightly
          components: rust-docs
          override: true

      - name: Install Node.js
        uses: actions/setup-node@v3
        with:
          node-version: '18'
          
      - name: Install OpenAPI Generator
        run: npm install -g @openapitools/openapi-generator-cli

      - name: Generate Documentation
        run: ./scripts/generate-docs.sh

      - name: Deploy to GitHub Pages
        if: github.ref == 'refs/heads/main'
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: ./docs/generated
```

## Documentation Maintenance

### Automated Updates

```bash
#!/bin/bash
# scripts/update-docs.sh

set -e

echo "🔄 Updating documentation..."

# Update dependencies
cargo update

# Regenerate documentation
./scripts/generate-docs.sh

# Validate generated documentation
./scripts/validate-docs.sh

# Check for broken links
./scripts/check-links.sh

echo "✅ Documentation updated successfully"
```

### Link Validation

```bash
#!/bin/bash
# scripts/check-links.sh

set -e

echo "🔗 Checking documentation links..."

# Install link checker if not present
if ! command -v markdown-link-check &> /dev/null; then
    npm install -g markdown-link-check
fi

# Check all markdown files
find docs -name "*.md" -exec markdown-link-check {} \;

echo "✅ Link validation complete"
```

---

This auto-generated API documentation system provides comprehensive, up-to-date documentation that reduces maintenance burden while improving developer experience. The system integrates with CI/CD for automatic updates and supports multiple output formats for different use cases.