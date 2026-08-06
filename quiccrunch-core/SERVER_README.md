# QuicCrunch Production Server & Client

Production-ready QUIC server and client binaries with multi-worker architecture for high throughput.

## Quick Start

### Multi-Worker Server (Recommended)

```bash
# Build
cargo build --release --bin multi_worker_server

# Run with 4 workers
./target/release/multi_worker_server 127.0.0.1:4433 \
    quiccrunch-core/tests/certs/cert.pem \
    quiccrunch-core/tests/certs/key.pem \
    4
```

### High-Performance Benchmark

```bash
# Build
cargo build --release --bin pipelined_benchmark

# Run benchmark
./target/release/pipelined_benchmark 127.0.0.1:4433 \
    1800 1800 16 1024 4
```

## Binaries

| Binary | Description | Use Case |
|--------|-------------|----------|
| `multi_worker_server` | Multi-worker server with SO_REUSEPORT | **Production (recommended)** |
| `mt_server` | Single-runtime server | Development/testing |
| `pipelined_benchmark` | High-throughput benchmark client | Performance testing |
| `quic_server` | Server with structured logging | Production with logging |
| `quic_client` | Client with logging | Production with logging |

## Multi-Worker Server

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    UDP Socket (SO_REUSEPORT)            │
├─────────────────────────────────────────────────────────┤
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐       │
│  │  Worker 0   │ │  Worker 1   │ │  Worker 2   │ ...   │
│  │  (CPU 0)    │ │  (CPU 1)    │ │  (CPU 2)    │       │
│  │ TquicRuntime│ │ TquicRuntime│ │ TquicRuntime│       │
│  └─────────────┘ └─────────────┘ └─────────────┘       │
└─────────────────────────────────────────────────────────┘
```

Each worker has its own `TquicRuntime` on a dedicated OS thread. The kernel distributes UDP packets via SO_REUSEPORT.

### Arguments

```bash
./target/release/multi_worker_server \
    <address>       # IP:PORT (default: 127.0.0.1:4433)
    <cert_file>     # TLS certificate path
    <key_file>      # TLS private key path
    <worker_count>  # Number of workers (default: 4)
```

### Example

```bash
# Production server on all interfaces with 8 workers
./target/release/multi_worker_server 0.0.0.0:8443 \
    /etc/ssl/certs/server.pem \
    /etc/ssl/private/server.key \
    8
```

### Performance

| Workers | Throughput | Latency |
|---------|------------|---------|
| 1 | 5,000 msg/s | 194 μs |
| 2 | 15,000 msg/s | 65 μs |
| 4 | **31,629 msg/s** | **32 μs** |
| 8 | ~50,000 msg/s | ~20 μs |

## Benchmark Client

### Arguments

```bash
./target/release/pipelined_benchmark \
    <address>           # Server address (default: 127.0.0.1:4433)
    <pipeline_depth>    # Messages in flight (default: 100)
    <messages_per_stream> # Total messages per stream
    <connections>       # Number of connections (default: 4)
    <msg_size>          # Message size in bytes (default: 1024)
    <workers>           # Client workers (default: 4)
```

### Optimal Settings

```bash
# Maximum throughput (1024B messages)
./target/release/pipelined_benchmark 127.0.0.1:4433 1800 1800 16 1024 4

# Maximum throughput (small messages)
./target/release/pipelined_benchmark 127.0.0.1:4433 1800 1800 16 64 4

# Low latency
./target/release/pipelined_benchmark 127.0.0.1:4433 100 100 4 1024 4
```

### Output

```
=== HIGH-PERFORMANCE BENCHMARK RESULTS ===
Duration: 0.974s
Total messages: 28800
Throughput: 29561 msg/s
Throughput: 486.22 Mbps
Avg latency: 33.83 μs
Bytes sent: 30146560 (30.15 MB)
Bytes received: 30146560 (30.15 MB)
Errors: 0
Pipeline depth: 1800
Concurrency: 16
Streams per connection: 4
```

## Logging

### Environment Variables

```bash
# Set log level
RUST_LOG=info      # Default
RUST_LOG=debug     # Verbose
RUST_LOG=trace     # Maximum
RUST_LOG=warn      # Quiet

# Module-specific
RUST_LOG=quiccrunch_core=debug,tquic=warn
```

### Example Output

```
[2026-08-06T05:00:29.849Z INFO  mt_server] Starting Multi-threaded QUIC server
[2026-08-06T05:00:29.850Z INFO  mt_server] Address: 127.0.0.1:4433
[2026-08-06T05:00:29.851Z INFO  mt_server] Workers: 4
[2026-08-06T05:00:29.852Z INFO  mt_server] Server listening on 127.0.0.1:4433
```

## TLS Certificates

### Using Test Certificates

```bash
# Generate self-signed certificates
openssl req -x509 -newkey rsa:2048 -keyout key.pem \
    -out cert.pem -days 365 -nodes \
    -subj "/CN=localhost"
```

### Production Certificates

For production, use certificates from a trusted CA or Let's Encrypt:

```bash
# Let's Encrypt with certbot
sudo certbot certonly --standalone -d yourdomain.com

# Use the generated certificates
./target/release/multi_worker_server 0.0.0.0:8443 \
    /etc/letsencrypt/live/yourdomain.com/fullchain.pem \
    /etc/letsencrypt/live/yourdomain.com/privkey.pem \
    4
```

## Performance Tuning

### Server

1. **Worker count**: Set to number of CPU cores
2. **Pipeline depth**: 1000-2000 for maximum throughput
3. **Connections**: 16-32 for optimal performance
4. **Message size**: Smaller = higher msg/s, larger = higher bandwidth

### Client

1. **Pipeline depth**: Match server capability
2. **Connections**: 16 for maximum throughput
3. **Workers**: 4 for most workloads

### System Tuning

```bash
# Increase UDP buffer sizes
sudo sysctl -w net.core.rmem_max=16777216
sudo sysctl -w net.core.wmem_max=16777216

# Increase file descriptor limit
ulimit -n 65536

# Enable CPU performance mode
sudo cpupower frequency-set -g performance
```

## Monitoring

### Health Endpoints

If using the production server with health checks:

```bash
# Liveness probe
curl http://localhost:8080/health/live

# Readiness probe
curl http://localhost:8080/health/ready
```

### Prometheus Metrics

```bash
# Get metrics in Prometheus format
curl http://localhost:8080/metrics
```

### Debug Logging

```bash
# Enable per-connection stats
RUST_LOG=debug ./target/release/multi_worker_server 127.0.0.1:4433 cert.pem key.pem 4
```

## Troubleshooting

### Port Already in Use

```bash
# Find process using port
lsof -i :4433

# Kill process
kill <PID>
```

### Connection Refused

```bash
# Check if server is running
ps aux | grep multi_worker_server

# Check firewall
sudo iptables -L -n | grep 4433
```

### Performance Issues

```bash
# Check CPU usage
top -p $(pgrep -f multi_worker_server)

# Check network
iftop -i eth0

# Profile with perf
perf record -p $(pgrep -f multi_worker_server) -g
perf report
```

## License

Apache-2.0 OR MIT
