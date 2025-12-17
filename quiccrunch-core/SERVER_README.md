# QuicCrunch Production Server & Client

Production-ready QUIC server and client binaries with proper structured logging (no emojis).

## Building

### Using Nix (Recommended)
```bash
nix develop -c bash -c "cd quiccrunch-core && cargo build --release --bin quic_server --bin quic_client"
```

### Using Cargo (requires cmake, perl, go)
```bash
cd quiccrunch-core
cargo build --release --bin quic_server --bin quic_client
```

Binaries will be in `target/release/`:
- `target/release/quic_server`
- `target/release/quic_client`

## Running the Server

### Basic Usage
```bash
# Default: listen on 127.0.0.1:4433 with 4 workers
./target/release/quic_server

# Custom address
./target/release/quic_server 0.0.0.0:8443

# Custom address and worker count
./target/release/quic_server 0.0.0.0:8443 8
```

### Arguments
1. **Address** (optional, default: `127.0.0.1:4433`): IP:PORT to listen on
2. **Workers** (optional, default: `4`): Number of worker threads

### Logging Levels

Set logging level with `RUST_LOG` environment variable:

```bash
# Info level (default) - shows connections and summary stats
RUST_LOG=info ./target/release/quic_server

# Debug level - shows per-connection details
RUST_LOG=debug ./target/release/quic_server

# Trace level - shows all messages and data transfers
RUST_LOG=trace ./target/release/quic_server

# Module-specific logging
RUST_LOG=quiccrunch_core=debug,tquic=warn ./target/release/quic_server
```

### Example Output (Info Level)
```
[2025-10-22T01:00:00.123Z INFO  quic_server] Starting QUIC server
[2025-10-22T01:00:00.124Z INFO  quic_server] Address: 127.0.0.1:4433
[2025-10-22T01:00:00.125Z INFO  quic_server] Workers: 4
[2025-10-22T01:00:00.200Z INFO  quic_server] Server listening on 127.0.0.1:4433
[2025-10-22T01:00:05.123Z DEBUG quic_server] Connection #0 accepted: ConnectionId(...)
[2025-10-22T01:00:05.125Z DEBUG quic_server] Connection #0: stream 2 opened
[2025-10-22T01:00:05.230Z DEBUG quic_server] Connection #0: processed 5 messages, 85 bytes total
[2025-10-22T01:00:05.231Z DEBUG quic_server] Connection #0 completed successfully
```

## Running the Client

### Basic Usage
```bash
# Connect to default server with 5 messages
./target/release/quic_client

# Custom server address
./target/release/quic_client 192.168.1.100:4433

# Custom message count
./target/release/quic_client 127.0.0.1:4433 10

# Custom worker count
./target/release/quic_client 127.0.0.1:4433 10 4
```

### Arguments
1. **Address** (optional, default: `127.0.0.1:4433`): Server IP:PORT to connect to
2. **Message Count** (optional, default: `5`): Number of messages to send
3. **Workers** (optional, default: `2`): Number of worker threads

### Example Output
```
[2025-10-22T01:00:05.000Z INFO  quic_client] Starting QUIC client
[2025-10-22T01:00:05.001Z INFO  quic_client] Server address: 127.0.0.1:4433
[2025-10-22T01:00:05.002Z INFO  quic_client] Messages to send: 5
[2025-10-22T01:00:05.003Z INFO  quic_client] Workers: 2
[2025-10-22T01:00:05.004Z INFO  quic_client] Connecting to 127.0.0.1:4433
[2025-10-22T01:00:05.123Z INFO  quic_client] Connected in 119ms: connection_id=ConnectionId(...)
[2025-10-22T01:00:05.124Z INFO  quic_client] Stream opened: stream_id=2
[2025-10-22T01:00:05.125Z INFO  quic_client] Sending 5 messages
[2025-10-22T01:00:05.230Z INFO  quic_client] Client completed successfully
[2025-10-22T01:00:05.231Z INFO  quic_client] Messages: 5
[2025-10-22T01:00:05.232Z INFO  quic_client] Bytes sent: 56
[2025-10-22T01:00:05.233Z INFO  quic_client] Bytes received: 86
[2025-10-22T01:00:05.234Z INFO  quic_client] Duration: 105ms
[2025-10-22T01:00:05.235Z INFO  quic_client] Throughput: 10.84 Mbps
```

## Testing

### Quick Test (Development)
```bash
# Terminal 1: Start server
nix develop -c bash -c "./target/debug/quic_server"

# Terminal 2: Run client
nix develop -c bash -c "./target/debug/quic_client"
```

### Production Test
```bash
# Terminal 1: Start server with 8 workers
RUST_LOG=info ./target/release/quic_server 0.0.0.0:4433 8

# Terminal 2: Run client with 100 messages
RUST_LOG=info ./target/release/quic_client 127.0.0.1:4433 100 4
```

### Load Testing
```bash
# Start server
./target/release/quic_server 0.0.0.0:4433 16

# Run multiple clients in parallel
for i in {1..10}; do
    ./target/release/quic_client 127.0.0.1:4433 1000 &
done
wait
```

## Configuration

### TLS Certificates

The server and client use embedded test certificates by default. For production use:

1. Generate your own certificates
2. Update the `TquicServer::new()` and `TquicClient::new()` code to load custom certs
3. See `quiccrunch-core/src/transport/adapter.rs` for certificate configuration

### Performance Tuning

#### Worker Count
- **Server**: Set to number of CPU cores for optimal performance
- **Client**: Usually 2-4 workers is sufficient

#### Buffer Sizes
- Default receive buffer: 8192 bytes
- Modify in source if larger messages needed

#### Connection Pooling
- Each worker maintains its own connection pool
- SO_REUSEPORT allows multiple workers to share the same port

## Monitoring

### Connection Stats
Enable debug logging to see per-connection statistics:
```bash
RUST_LOG=debug ./target/release/quic_server
```

Output includes:
- Connection ID
- Stream ID
- Message count
- Total bytes transferred
- Connection duration

### System Monitoring
```bash
# Monitor connections
ss -tan | grep 4433

# Monitor resource usage
top -p $(pgrep quic_server)

# Monitor network throughput
iftop
```

## Troubleshooting

### Server won't start
- Check if port is already in use: `ss -tan | grep 4433`
- Try different port: `./target/release/quic_server 127.0.0.1:5443`
- Check firewall: `sudo ufw allow 4433/udp` (UDP not TCP!)

### Client can't connect
- Verify server is running: `ss -uan | grep 4433`
- Check network connectivity: `ping <server-ip>`
- Enable debug logging: `RUST_LOG=debug ./target/release/quic_client`

### Performance issues
- Increase worker count
- Check CPU usage with `top`
- Monitor network with `iftop`
- Enable trace logging to identify bottlenecks

## Architecture

- **Multi-threaded**: Uses worker pool with SO_REUSEPORT for true parallelism
- **Lock-free**: Connection routing uses lock-free data structures
- **Async**: Built on Tokio async runtime
- **Transport**: Uses tQUIC v1.6.0 QUIC implementation
- **Logging**: Structured logging via `env_logger` and `log` crate

## Differences from Example

The `bidirectional_streaming.rs` example is for demonstration. These binaries are production-ready:

1. **No emojis** - Professional structured logging only
2. **Statistics** - Tracks throughput, latency, message counts
3. **Configuration** - Command-line arguments for all settings
4. **Monitoring** - Detailed logs at multiple levels
5. **Error handling** - Robust error handling and recovery
6. **Production ready** - Suitable for deployment

## License

Same as QuicCrunch project (Apache-2.0 OR MIT)
