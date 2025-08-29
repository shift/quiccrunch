# Troubleshooting Guide

This guide helps diagnose and resolve common issues with QuicCrunch deployments, from development to production environments.

## Table of Contents

1. [Quick Diagnostics](#quick-diagnostics)
2. [Connection Issues](#connection-issues)
3. [Performance Problems](#performance-problems)
4. [TLS/Certificate Issues](#tlscertificate-issues)
5. [Memory and Resource Issues](#memory-and-resource-issues)
6. [Logging and Debugging](#logging-and-debugging)
7. [Network Configuration](#network-configuration)
8. [Container/Kubernetes Issues](#containerkubernetes-issues)
9. [Common Error Messages](#common-error-messages)
10. [Diagnostic Tools](#diagnostic-tools)

## Quick Diagnostics

### Health Check Script

```bash
#!/bin/bash
# quiccrunch-health-check.sh

set -e

echo "🏥 QuicCrunch Health Diagnostic"
echo "================================"

# Basic connectivity
echo "📡 Testing basic connectivity..."
if curl -f -s http://localhost:8080/health > /dev/null; then
    echo "✅ Health endpoint responding"
else
    echo "❌ Health endpoint not responding"
    echo "   Check if server is running on port 8080"
fi

# QUIC connectivity
echo "🔌 Testing QUIC connectivity..."
if timeout 5 nc -u localhost 50051 < /dev/null; then
    echo "✅ UDP port 50051 is reachable"
else
    echo "❌ UDP port 50051 is not reachable"
    echo "   Check firewall and server binding"
fi

# Certificate validation
echo "🔒 Checking TLS configuration..."
if [ -f "/etc/ssl/certs/server.crt" ]; then
    if openssl x509 -in /etc/ssl/certs/server.crt -checkend 86400 -noout; then
        echo "✅ Server certificate is valid"
    else
        echo "⚠️ Server certificate expires within 24 hours"
    fi
else
    echo "⚠️ Server certificate not found at default location"
fi

# Resource usage
echo "📊 Checking resource usage..."
CPU_USAGE=$(top -bn1 | grep "quiccrunch" | awk '{print $9}' | head -1)
MEM_USAGE=$(top -bn1 | grep "quiccrunch" | awk '{print $10}' | head -1)

if [ ! -z "$CPU_USAGE" ]; then
    echo "CPU Usage: ${CPU_USAGE}%"
    echo "Memory Usage: ${MEM_USAGE}%"
else
    echo "❌ QuicCrunch process not found"
fi

echo "================================"
echo "Health check complete!"
```

### Quick Status Check

```bash
# Check if QuicCrunch is running
ps aux | grep quiccrunch

# Check port binding
netstat -ulnp | grep :50051

# Check recent logs
tail -f /var/log/quiccrunch/server.log

# Test RPC call
echo '{"method": "health_check", "params": {}}' | nc -u localhost 50051
```

## Connection Issues

### Client Cannot Connect to Server

**Symptoms:**
- Connection timeouts
- "Connection refused" errors
- No response from server

**Diagnostic Steps:**

1. **Verify Server is Running**
   ```bash
   # Check process
   ps aux | grep quiccrunch-server
   
   # Check if port is bound
   netstat -ulnp | grep :50051
   
   # Check systemd status (if using systemd)
   systemctl status quiccrunch-server
   ```

2. **Test Network Connectivity**
   ```bash
   # Test UDP connectivity
   nc -u -v hostname 50051
   
   # Test from remote host
   nmap -sU hostname -p 50051
   
   # Check firewall rules
   iptables -L -n | grep 50051
   ```

3. **Verify Configuration**
   ```bash
   # Check bind address
   grep bind_addr /etc/quiccrunch/server.toml
   
   # Ensure it's not bound to localhost only
   # Should be "0.0.0.0:50051" not "127.0.0.1:50051"
   ```

**Common Solutions:**

- **Wrong bind address**: Change `bind_addr = "127.0.0.1:50051"` to `bind_addr = "0.0.0.0:50051"`
- **Firewall blocking UDP**: `sudo ufw allow 50051/udp`
- **SELinux restrictions**: `sudo setsebool -P httpd_can_network_connect 1`

### QUIC Handshake Failures

**Symptoms:**
- Connection attempts timeout during handshake
- TLS/certificate errors

**Diagnostic Steps:**

1. **Check Certificate Configuration**
   ```bash
   # Verify certificate files exist
   ls -la /etc/ssl/certs/server.crt /etc/ssl/private/server.key
   
   # Check certificate validity
   openssl x509 -in /etc/ssl/certs/server.crt -noout -dates
   
   # Verify certificate chain
   openssl verify -CAfile /etc/ssl/certs/ca.crt /etc/ssl/certs/server.crt
   ```

2. **Test TLS Handshake**
   ```bash
   # Use openssl to test TLS
   echo | openssl s_client -connect hostname:50051 -servername hostname
   
   # Check certificate expiration
   echo | openssl s_client -connect hostname:50051 2>/dev/null | openssl x509 -noout -dates
   ```

**Common Solutions:**

- **Expired certificate**: Renew certificate with Let's Encrypt or CA
- **Wrong hostname**: Ensure certificate CN/SAN matches connection hostname
- **Self-signed certificate**: Add `-k` flag for testing or add CA to trust store

### Connection Drops/Instability

**Symptoms:**
- Connections drop after period of inactivity
- Intermittent connection failures
- High connection churn

**Diagnostic Steps:**

1. **Check Timeout Configuration**
   ```toml
   # In server.toml
   [server]
   idle_timeout = "60s"          # Increase if connections drop too quickly
   keep_alive_interval = "30s"   # Send keep-alive packets
   max_stream_duration = "3600s" # Maximum stream lifetime
   ```

2. **Monitor Connection Metrics**
   ```bash
   # Check connection counts
   curl http://localhost:8080/metrics | grep quiccrunch_connections
   
   # Monitor connection lifecycle
   tail -f /var/log/quiccrunch/server.log | grep -E "(connection|stream)"
   ```

**Common Solutions:**

- **Increase idle timeout**: Set longer `idle_timeout` in configuration
- **Enable keep-alive**: Configure `keep_alive_interval`
- **Check network stability**: Look for packet loss or network issues

## Performance Problems

### High Latency

**Symptoms:**
- RPC calls taking longer than expected
- Increasing response times under load

**Diagnostic Steps:**

1. **Check Server Performance Metrics**
   ```bash
   # Check CPU and memory usage
   top -p $(pgrep quiccrunch-server)
   
   # Check RPC latency metrics
   curl http://localhost:8080/metrics | grep quiccrunch_rpc_duration
   
   # Check request rate
   curl http://localhost:8080/metrics | grep quiccrunch_rpc_requests_total
   ```

2. **Network Latency Testing**
   ```bash
   # Test network round-trip time
   ping hostname
   
   # Test UDP specifically
   hping3 -2 -p 50051 hostname
   
   # Check for packet loss
   mtr hostname
   ```

3. **Profile Application Performance**
   ```bash
   # Enable performance profiling
   RUST_LOG=debug ./quiccrunch-server --config server.toml
   
   # Use perf for CPU profiling
   perf record -g ./quiccrunch-server --config server.toml
   perf report
   ```

**Common Solutions:**

- **Increase worker threads**: Set `TOKIO_WORKER_THREADS=8` environment variable
- **Tune buffer sizes**: Increase UDP buffer sizes in OS configuration
- **Optimize serialization**: Use binary formats instead of JSON
- **Enable connection pooling**: Reuse connections on client side

### Low Throughput

**Symptoms:**
- Lower than expected requests per second
- Server not utilizing available resources

**Diagnostic Steps:**

1. **Check Resource Utilization**
   ```bash
   # CPU utilization per core
   mpstat -P ALL 1
   
   # Memory usage
   free -h
   
   # Network interface statistics
   sar -n DEV 1
   
   # Disk I/O (if applicable)
   iostat -x 1
   ```

2. **Application-Level Bottlenecks**
   ```bash
   # Check connection limits
   curl http://localhost:8080/metrics | grep quiccrunch_connections_active
   
   # Check stream limits
   curl http://localhost:8080/metrics | grep quiccrunch_streams_active
   
   # Check queue depths
   curl http://localhost:8080/metrics | grep quiccrunch_queue
   ```

**Common Solutions:**

- **Increase connection limits**: Raise `max_connections` in configuration
- **Tune worker pool**: Increase `TOKIO_WORKER_THREADS`
- **Optimize data structures**: Use more efficient serialization
- **Enable io_uring**: Use io_uring transport for better I/O performance

### Memory Leaks

**Symptoms:**
- Continuously increasing memory usage
- Eventually leading to OOM kills

**Diagnostic Steps:**

1. **Monitor Memory Usage**
   ```bash
   # Track memory usage over time
   while true; do
     ps -o pid,rss,vsz,comm -p $(pgrep quiccrunch-server)
     sleep 60
   done
   
   # Use valgrind for detailed analysis
   valgrind --tool=memcheck --leak-check=full ./quiccrunch-server
   ```

2. **Check for Resource Cleanup**
   ```bash
   # Monitor open file descriptors
   lsof -p $(pgrep quiccrunch-server) | wc -l
   
   # Check for growing connection counts
   watch 'curl -s http://localhost:8080/metrics | grep connections'
   ```

**Common Solutions:**

- **Check stream cleanup**: Ensure streams are properly closed
- **Review connection handling**: Verify connections are cleaned up
- **Update dependencies**: Use latest versions with memory fixes
- **Set memory limits**: Use container memory limits to detect issues early

## TLS/Certificate Issues

### Certificate Verification Failures

**Symptoms:**
- "Certificate verification failed" errors
- TLS handshake failures

**Diagnostic Steps:**

1. **Verify Certificate Chain**
   ```bash
   # Check full certificate chain
   openssl s_client -connect hostname:50051 -showcerts
   
   # Verify against CA
   openssl verify -CAfile ca.crt server.crt
   
   # Check certificate details
   openssl x509 -in server.crt -text -noout
   ```

2. **Test Different TLS Settings**
   ```bash
   # Test with different TLS versions
   openssl s_client -tls1_3 -connect hostname:50051
   openssl s_client -tls1_2 -connect hostname:50051
   ```

**Common Solutions:**

- **Add intermediate certificates**: Include full certificate chain
- **Update CA bundle**: Ensure client has up-to-date CA certificates
- **Check certificate dates**: Verify certificate is not expired
- **Verify hostname matching**: Ensure certificate CN/SAN matches

### Self-Signed Certificate Issues

**Symptoms:**
- Certificate validation errors in production
- Clients refusing to connect

**Diagnostic Steps:**

1. **Generate Proper Certificates**
   ```bash
   # Generate CA
   openssl genrsa -out ca.key 4096
   openssl req -new -x509 -days 365 -key ca.key -out ca.crt
   
   # Generate server certificate
   openssl genrsa -out server.key 4096
   openssl req -new -key server.key -out server.csr
   openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -out server.crt
   ```

2. **Configure Client Trust**
   ```bash
   # Add CA to system trust store (Ubuntu/Debian)
   sudo cp ca.crt /usr/local/share/ca-certificates/
   sudo update-ca-certificates
   
   # For client applications
   export SSL_CERT_FILE=/path/to/ca.crt
   ```

## Memory and Resource Issues

### Out of Memory (OOM) Issues

**Symptoms:**
- Process killed by OOM killer
- Memory allocation failures

**Diagnostic Steps:**

1. **Check Memory Limits**
   ```bash
   # Check system memory
   free -h
   
   # Check process memory usage
   cat /proc/$(pgrep quiccrunch-server)/status | grep -E "(VmSize|VmRSS|VmHWM)"
   
   # Check OOM killer logs
   dmesg | grep -i "killed process"
   journalctl -u quiccrunch-server | grep -i "memory"
   ```

2. **Monitor Memory Growth**
   ```bash
   # Track memory usage patterns
   while true; do
     echo "$(date): $(ps -o rss= -p $(pgrep quiccrunch-server))"
     sleep 10
   done
   ```

**Common Solutions:**

- **Increase system memory**: Add more RAM or swap
- **Set memory limits**: Use systemd or container limits
- **Optimize buffer sizes**: Reduce buffer sizes in configuration
- **Enable memory pooling**: Use object pools for frequent allocations

### File Descriptor Exhaustion

**Symptoms:**
- "Too many open files" errors
- Connection acceptance failures

**Diagnostic Steps:**

1. **Check File Descriptor Limits**
   ```bash
   # Check current limits
   ulimit -n
   
   # Check system-wide limits
   cat /proc/sys/fs/file-max
   
   # Check current usage
   lsof -p $(pgrep quiccrunch-server) | wc -l
   ```

2. **Monitor File Descriptor Usage**
   ```bash
   # Track FD usage over time
   watch 'lsof -p $(pgrep quiccrunch-server) | wc -l'
   
   # Check specific file types
   lsof -p $(pgrep quiccrunch-server) | grep -E "(socket|pipe|file)"
   ```

**Common Solutions:**

- **Increase ulimits**: Set higher limits in `/etc/security/limits.conf`
- **Configure systemd limits**: Set `LimitNOFILE=65536` in service file
- **Close unused connections**: Implement connection cleanup
- **Use connection pooling**: Reduce number of open connections

## Logging and Debugging

### Enable Debug Logging

```toml
# server.toml
[logging]
level = "debug"
format = "json"
output = "/var/log/quiccrunch/debug.log"
```

```bash
# Runtime logging level change
export RUST_LOG=debug
./quiccrunch-server --config server.toml

# Module-specific logging
export RUST_LOG=quiccrunch_core::transport=debug,quiccrunch_core::rpc=info
```

### Structured Logging Analysis

```bash
# Parse JSON logs with jq
tail -f /var/log/quiccrunch/server.log | jq -r '.message'

# Filter by log level
cat /var/log/quiccrunch/server.log | jq 'select(.level == "ERROR")'

# Filter by component
cat /var/log/quiccrunch/server.log | jq 'select(.target | contains("transport"))'

# Extract timing information
cat /var/log/quiccrunch/server.log | jq 'select(.fields.duration_ms != null) | .fields.duration_ms'
```

### Distributed Tracing

```toml
# Enable tracing
[tracing]
enabled = true
jaeger_endpoint = "http://localhost:14268/api/traces"
sample_rate = 1.0  # 100% sampling for debugging
```

```bash
# Check Jaeger for traces
curl "http://localhost:16686/api/traces?service=quiccrunch-server"
```

## Network Configuration

### UDP Buffer Tuning

```bash
# Check current UDP buffer sizes
cat /proc/sys/net/core/rmem_max
cat /proc/sys/net/core/wmem_max

# Increase UDP buffers for high throughput
echo 134217728 > /proc/sys/net/core/rmem_max
echo 134217728 > /proc/sys/net/core/wmem_max

# Make permanent
echo 'net.core.rmem_max = 134217728' >> /etc/sysctl.conf
echo 'net.core.wmem_max = 134217728' >> /etc/sysctl.conf
sysctl -p
```

### Firewall Configuration

```bash
# UFW (Ubuntu)
sudo ufw allow 50051/udp
sudo ufw allow 8080/tcp

# iptables
sudo iptables -A INPUT -p udp --dport 50051 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 8080 -j ACCEPT

# firewalld (RHEL/CentOS)
sudo firewall-cmd --permanent --add-port=50051/udp
sudo firewall-cmd --permanent --add-port=8080/tcp
sudo firewall-cmd --reload
```

### NAT and Load Balancer Issues

```bash
# Check for NAT translation issues
tcpdump -i any -n udp port 50051

# Test direct connection bypassing load balancer
nc -u server-ip 50051

# Check load balancer health checks
curl http://localhost:8080/health
```

## Container/Kubernetes Issues

### Container Networking

**Symptoms:**
- Service not accessible from outside container
- Internal connectivity issues

**Diagnostic Steps:**

```bash
# Check container networking
docker network ls
docker inspect container_name | jq '.[0].NetworkSettings'

# Test connectivity from inside container
docker exec -it container_name nc -u -v localhost 50051

# Check port mapping
docker port container_name
```

### Kubernetes Troubleshooting

```bash
# Check pod status
kubectl get pods -n quiccrunch

# Check pod logs
kubectl logs -f deployment/quiccrunch-server -n quiccrunch

# Check service endpoints
kubectl get endpoints -n quiccrunch

# Check network policies
kubectl get networkpolicies -n quiccrunch

# Test internal connectivity
kubectl exec -it pod_name -- nc -u -v service-name 50051
```

### Resource Limits

```yaml
# Proper resource configuration
resources:
  requests:
    cpu: 100m
    memory: 128Mi
  limits:
    cpu: 1000m
    memory: 512Mi

# Check if pod is being throttled
kubectl describe pod pod_name | grep -A 5 -B 5 "Throttled"
```

## Common Error Messages

### "Connection Refused"

**Cause**: Server not running or not bound to the correct address
**Solution**: 
```bash
# Check if server is running
ps aux | grep quiccrunch-server

# Check port binding
netstat -ulnp | grep :50051

# Restart server with correct configuration
./quiccrunch-server --bind-addr 0.0.0.0:50051
```

### "Certificate Verification Failed"

**Cause**: Invalid or expired TLS certificate
**Solution**:
```bash
# Check certificate validity
openssl x509 -in server.crt -noout -dates

# Regenerate if expired
./scripts/generate-certs.sh

# Update certificate in configuration
```

### "Too Many Open Files"

**Cause**: File descriptor limit exceeded
**Solution**:
```bash
# Increase limits
echo "* soft nofile 65536" >> /etc/security/limits.conf
echo "* hard nofile 65536" >> /etc/security/limits.conf

# For systemd services
echo "LimitNOFILE=65536" >> /etc/systemd/system/quiccrunch.service
systemctl daemon-reload
systemctl restart quiccrunch
```

### "Address Already in Use"

**Cause**: Another process using the port
**Solution**:
```bash
# Find process using port
lsof -i :50051

# Kill conflicting process
kill -9 $(lsof -t -i:50051)

# Or change port in configuration
```

## Diagnostic Tools

### Network Diagnostics

```bash
# Install network diagnostic tools
apt-get install netstat-nat tcpdump nmap netcat-openbsd

# Monitor network traffic
tcpdump -i any -n udp port 50051

# Test UDP connectivity
nc -u -v hostname 50051

# Scan for open ports
nmap -sU hostname -p 50051
```

### Performance Profiling

```bash
# Install profiling tools
apt-get install linux-perf htop iotop

# CPU profiling
perf record -g ./quiccrunch-server
perf report

# Memory profiling
valgrind --tool=massif ./quiccrunch-server
ms_print massif.out.*
```

### Log Analysis

```bash
# Install log analysis tools
apt-get install jq goaccess

# Analyze JSON logs
cat server.log | jq -r 'select(.level == "ERROR") | .message'

# Generate log reports
goaccess server.log --log-format=COMBINED
```

---

For additional troubleshooting support, check the [Operations Guide](README.md) or report issues on [GitHub Issues](https://github.com/your-org/quiccrunch/issues).