# QuicCrunch Setup Guide

## Prerequisites

### Development Environment

QuicCrunch requires the following tools, which are automatically provisioned through the Nix development shell:

- Rust toolchain (stable)
- Cap'n Proto compiler
- OpenSSL
- CMake
- CPU frequency control tools (for benchmarking)
- Temperature monitoring tools

### Setting Up the Development Environment

1. Install Nix with flakes support:
   ```bash
   sh <(curl -L https://nixos.org/nix/install) --daemon
   ```

2. Enable flakes in Nix:
   ```bash
   mkdir -p ~/.config/nix
   echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf
   ```

3. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/quiccrunch.git
   cd quiccrunch
   ```

4. Enter the development shell:
   ```bash
   nix develop
   ```

## Building the Project

1. Build the project:
   ```bash
   cargo build
   ```

2. Run tests:
   ```bash
   cargo test
   ```

3. Build documentation:
   ```bash
   cargo doc --no-deps
   ```

## Configuration

### Basic Configuration

Add QuicCrunch to your project's `Cargo.toml`:

```toml
[dependencies]
quiccrunch = "0.1"
```

### Transport Configuration

Configure the QUIC transport with custom settings:

```rust
use std::time::Duration;
use quiccrunch::TransportConfig;

let config = TransportConfig {
    connect_timeout: Duration::from_secs(30),
    max_concurrent_streams: 2000,
    keep_alive_interval: Duration::from_secs(10),
    initial_rtt: Duration::from_millis(50),
    max_message_size: 20 * 1024 * 1024, // 20MB
    stream_read_timeout: Duration::from_secs(60),
};
```

### TLS Configuration

Set up TLS security with certificates:

```rust
use quiccrunch::{TlsConfig, cert_utils};

// Generate or load certificates
let (ca_cert, ca_key) = cert_utils::generate_ca_cert()?;
let (cert, key) = cert_utils::generate_node_cert(&ca_cert, "server1")?;

let tls_config = TlsConfig {
    cert,
    key,
    ca_cert,
};
```

### Optional Features

Enable optional features in your Cargo.toml:

```toml
[dependencies]
quiccrunch = { version = "0.1", features = ["telemetry"] }
```

Available features:
- `tls` (default): Enables TLS support
- `telemetry`: Enables OpenTelemetry integration

## Troubleshooting

### Common Issues

1. CPU Frequency Control Access
   - Error: "No write access to CPU frequency controls"
   - Solution: Run benchmarks with sudo for full frequency scaling support

2. Certificate Generation Failures
   - Error: "Failed to generate certificates"
   - Solution: Ensure OpenSSL is properly installed and accessible

3. Build Failures
   - Error: "failed to run custom build command for capnp-xxx"
   - Solution: Ensure Cap'n Proto compiler is installed (automatically handled in nix shell)

### Getting Help

- Check the [GitHub Issues](https://github.com/yourusername/quiccrunch/issues)
- Join our [Discord community](https://discord.gg/yourdiscord)
- Read the [API documentation](https://docs.rs/quiccrunch)