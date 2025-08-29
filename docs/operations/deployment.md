# Deployment Guide

This guide covers deploying QuicCrunch services to production environments, from single-server deployments to large-scale distributed systems.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Deployment Strategies](#deployment-strategies)
3. [Infrastructure Requirements](#infrastructure-requirements)
4. [Container Deployment](#container-deployment)
5. [Kubernetes Deployment](#kubernetes-deployment)
6. [Configuration Management](#configuration-management)
7. [Load Balancing](#load-balancing)
8. [TLS Configuration](#tls-configuration)
9. [Monitoring Setup](#monitoring-setup)
10. [Troubleshooting](#troubleshooting)

## Quick Start

### Single Server Deployment

For development or small-scale production deployments:

```bash
# 1. Build the application
cargo build --release

# 2. Create configuration
cat > config.toml << EOF
[server]
bind_addr = "0.0.0.0:50051"
max_connections = 1000
idle_timeout = "30s"

[tls]
cert_file = "/etc/ssl/certs/server.crt"
key_file = "/etc/ssl/private/server.key"

[logging]
level = "info"
format = "json"
EOF

# 3. Run the server
./target/release/quiccrunch-server --config config.toml
```

### Docker Deployment

```bash
# 1. Build Docker image
docker build -t quiccrunch-server:latest .

# 2. Run container
docker run -d \
  --name quiccrunch-server \
  -p 50051:50051/udp \
  -v /path/to/config.toml:/app/config.toml:ro \
  -v /path/to/certs:/etc/ssl/certs:ro \
  quiccrunch-server:latest
```

## Deployment Strategies

### Blue-Green Deployment

Minimize downtime with blue-green deployments:

```yaml
# docker-compose.yml
version: '3.8'
services:
  quiccrunch-blue:
    image: quiccrunch-server:v1.0.0
    ports:
      - "50051:50051/udp"
    environment:
      - DEPLOYMENT_COLOR=blue
    volumes:
      - ./config:/app/config:ro
      - ./certs:/etc/ssl/certs:ro

  quiccrunch-green:
    image: quiccrunch-server:v1.1.0
    ports:
      - "50052:50051/udp"
    environment:
      - DEPLOYMENT_COLOR=green
    volumes:
      - ./config:/app/config:ro
      - ./certs:/etc/ssl/certs:ro

  load-balancer:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - quiccrunch-blue
      - quiccrunch-green
```

### Rolling Deployment

For Kubernetes environments:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: quiccrunch-server
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
  selector:
    matchLabels:
      app: quiccrunch-server
  template:
    metadata:
      labels:
        app: quiccrunch-server
    spec:
      containers:
      - name: quiccrunch-server
        image: quiccrunch-server:v1.1.0
        ports:
        - containerPort: 50051
          protocol: UDP
        resources:
          requests:
            cpu: 100m
            memory: 128Mi
          limits:
            cpu: 500m
            memory: 512Mi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
```

### Canary Deployment

Gradually roll out changes to a subset of traffic:

```yaml
# Istio VirtualService for canary deployment
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: quiccrunch-canary
spec:
  hosts:
  - quiccrunch-service
  http:
  - match:
    - headers:
        canary:
          exact: "true"
    route:
    - destination:
        host: quiccrunch-service
        subset: v2
  - route:
    - destination:
        host: quiccrunch-service
        subset: v1
      weight: 90
    - destination:
        host: quiccrunch-service
        subset: v2
      weight: 10
```

## Infrastructure Requirements

### Hardware Requirements

| Deployment Size | CPU | Memory | Network | Storage |
|----------------|-----|--------|---------|---------|
| Small (< 1K RPS) | 2 cores | 4GB | 1 Gbps | 50GB SSD |
| Medium (< 10K RPS) | 4 cores | 8GB | 10 Gbps | 100GB SSD |
| Large (< 100K RPS) | 8 cores | 16GB | 10 Gbps | 200GB SSD |
| Extra Large (> 100K RPS) | 16+ cores | 32GB+ | 25+ Gbps | 500GB+ NVMe |

### Network Requirements

- **UDP Support**: QUIC requires UDP traffic (not just TCP)
- **Port Range**: Default port 50051, configure firewall accordingly
- **Load Balancer**: Must support UDP load balancing
- **Connection Migration**: Allow client IP changes for mobile scenarios

### Operating System Tuning

```bash
# Increase UDP buffer sizes
echo 'net.core.rmem_max = 134217728' >> /etc/sysctl.conf
echo 'net.core.wmem_max = 134217728' >> /etc/sysctl.conf
echo 'net.core.rmem_default = 65536' >> /etc/sysctl.conf
echo 'net.core.wmem_default = 65536' >> /etc/sysctl.conf

# Increase connection tracking
echo 'net.netfilter.nf_conntrack_max = 1048576' >> /etc/sysctl.conf

# Apply changes
sysctl -p
```

## Container Deployment

### Dockerfile

```dockerfile
# Multi-stage build for optimal image size
FROM rust:1.70-slim as builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY quiccrunch-core ./quiccrunch-core
COPY quiccrunch-macros ./quiccrunch-macros
COPY quiccrunch-cli ./quiccrunch-cli

# Build application
RUN cargo build --release --bin quiccrunch-server

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r quiccrunch && useradd -r -g quiccrunch quiccrunch

# Copy binary
COPY --from=builder /app/target/release/quiccrunch-server /usr/local/bin/

# Create directories
RUN mkdir -p /app/config /app/logs && \
    chown -R quiccrunch:quiccrunch /app

USER quiccrunch
WORKDIR /app

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1

EXPOSE 50051/udp 8080/tcp

CMD ["quiccrunch-server", "--config", "/app/config/server.toml"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  quiccrunch-server:
    build:
      context: .
      dockerfile: docker/Dockerfile.server
    ports:
      - "50051:50051/udp"
      - "8080:8080/tcp"
    environment:
      - RUST_LOG=info
      - SERVER_CONFIG_PATH=/app/config/server.toml
    volumes:
      - ./config:/app/config:ro
      - ./certs:/app/certs:ro
      - quiccrunch-logs:/app/logs
    networks:
      - quiccrunch-network
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
    depends_on:
      - prometheus
      - grafana

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus-data:/prometheus
    networks:
      - quiccrunch-network
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana-data:/var/lib/grafana
      - ./monitoring/grafana/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./monitoring/grafana/datasources:/etc/grafana/provisioning/datasources:ro
    networks:
      - quiccrunch-network

volumes:
  quiccrunch-logs:
  prometheus-data:
  grafana-data:

networks:
  quiccrunch-network:
    driver: bridge
```

## Kubernetes Deployment

### Complete Kubernetes Manifests

```yaml
# Namespace
apiVersion: v1
kind: Namespace
metadata:
  name: quiccrunch
---
# ConfigMap
apiVersion: v1
kind: ConfigMap
metadata:
  name: quiccrunch-config
  namespace: quiccrunch
data:
  server.toml: |
    [server]
    bind_addr = "0.0.0.0:50051"
    max_connections = 1000
    idle_timeout = "30s"
    
    [logging]
    level = "info"
    format = "json"
    
    [metrics]
    enabled = true
    bind_addr = "0.0.0.0:8080"
    path = "/metrics"
---
# Secret for TLS certificates
apiVersion: v1
kind: Secret
metadata:
  name: quiccrunch-tls
  namespace: quiccrunch
type: kubernetes.io/tls
data:
  tls.crt: LS0tLS1CRUdJTi... # base64 encoded certificate
  tls.key: LS0tLS1CRUdJTi... # base64 encoded private key
---
# Deployment
apiVersion: apps/v1
kind: Deployment
metadata:
  name: quiccrunch-server
  namespace: quiccrunch
  labels:
    app: quiccrunch-server
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxUnavailable: 1
      maxSurge: 1
  selector:
    matchLabels:
      app: quiccrunch-server
  template:
    metadata:
      labels:
        app: quiccrunch-server
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8080"
        prometheus.io/path: "/metrics"
    spec:
      serviceAccountName: quiccrunch-server
      containers:
      - name: quiccrunch-server
        image: quiccrunch-server:v1.0.0
        ports:
        - name: quic
          containerPort: 50051
          protocol: UDP
        - name: metrics
          containerPort: 8080
          protocol: TCP
        env:
        - name: RUST_LOG
          value: "info"
        - name: SERVER_CONFIG_PATH
          value: "/app/config/server.toml"
        volumeMounts:
        - name: config
          mountPath: /app/config
          readOnly: true
        - name: tls-certs
          mountPath: /app/certs
          readOnly: true
        resources:
          requests:
            cpu: 100m
            memory: 128Mi
            ephemeral-storage: 1Gi
          limits:
            cpu: 1000m
            memory: 512Mi
            ephemeral-storage: 5Gi
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 3
        securityContext:
          allowPrivilegeEscalation: false
          readOnlyRootFilesystem: true
          runAsNonRoot: true
          runAsUser: 10001
          capabilities:
            drop:
            - ALL
      volumes:
      - name: config
        configMap:
          name: quiccrunch-config
      - name: tls-certs
        secret:
          secretName: quiccrunch-tls
---
# Service
apiVersion: v1
kind: Service
metadata:
  name: quiccrunch-service
  namespace: quiccrunch
  labels:
    app: quiccrunch-server
spec:
  selector:
    app: quiccrunch-server
  ports:
  - name: quic
    port: 50051
    targetPort: 50051
    protocol: UDP
  - name: metrics
    port: 8080
    targetPort: 8080
    protocol: TCP
  type: LoadBalancer
---
# ServiceAccount
apiVersion: v1
kind: ServiceAccount
metadata:
  name: quiccrunch-server
  namespace: quiccrunch
---
# RBAC
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: quiccrunch-server
rules:
- apiGroups: [""]
  resources: ["pods", "services", "endpoints"]
  verbs: ["get", "list", "watch"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata:
  name: quiccrunch-server
roleRef:
  apiGroup: rbac.authorization.k8s.io
  kind: ClusterRole
  name: quiccrunch-server
subjects:
- kind: ServiceAccount
  name: quiccrunch-server
  namespace: quiccrunch
```

### Helm Chart

```yaml
# Chart.yaml
apiVersion: v2
name: quiccrunch
description: A Helm chart for QuicCrunch RPC server
type: application
version: 0.1.0
appVersion: "1.0.0"

# values.yaml
replicaCount: 3

image:
  repository: quiccrunch-server
  pullPolicy: IfNotPresent
  tag: "v1.0.0"

service:
  type: LoadBalancer
  quicPort: 50051
  metricsPort: 8080

ingress:
  enabled: false
  className: "nginx"
  annotations: {}
  hosts:
    - host: quiccrunch.local
      paths:
        - path: /
          pathType: Prefix
  tls: []

resources:
  limits:
    cpu: 1000m
    memory: 512Mi
  requests:
    cpu: 100m
    memory: 128Mi

autoscaling:
  enabled: true
  minReplicas: 2
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80

nodeSelector: {}
tolerations: []
affinity: {}

config:
  server:
    bind_addr: "0.0.0.0:50051"
    max_connections: 1000
    idle_timeout: "30s"
  logging:
    level: "info"
    format: "json"
  metrics:
    enabled: true
    bind_addr: "0.0.0.0:8080"
    path: "/metrics"
```

## Configuration Management

### Environment-Specific Configuration

```toml
# config/production.toml
[server]
bind_addr = "0.0.0.0:50051"
max_connections = 10000
idle_timeout = "60s"
max_stream_duration = "3600s"
keep_alive_interval = "30s"
max_concurrent_streams = 1000

[tls]
cert_file = "/etc/ssl/certs/quiccrunch.crt"
key_file = "/etc/ssl/private/quiccrunch.key"
ca_file = "/etc/ssl/certs/ca.crt"
verify_client = true

[logging]
level = "info"
format = "json"
output = "/app/logs/server.log"
rotation = "daily"
max_files = 7

[metrics]
enabled = true
bind_addr = "0.0.0.0:8080"
path = "/metrics"
histogram_buckets = [0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]

[tracing]
enabled = true
jaeger_endpoint = "http://jaeger:14268/api/traces"
sample_rate = 0.1

[limits]
max_request_size = "1MB"
max_response_size = "10MB"
rate_limit_requests_per_second = 1000
rate_limit_burst = 100
```

### Configuration Validation

```rust
// Example configuration validation script
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    server: ServerSettings,
    tls: TlsSettings,
    logging: LoggingSettings,
    metrics: MetricsSettings,
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerSettings {
    bind_addr: SocketAddr,
    max_connections: usize,
    idle_timeout: Duration,
    max_stream_duration: Duration,
}

fn validate_config(config_path: &str) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let config_content = std::fs::read_to_string(config_path)?;
    let config: ServerConfig = toml::from_str(&config_content)?;
    
    // Validate settings
    if config.server.max_connections == 0 {
        return Err("max_connections must be greater than 0".into());
    }
    
    if config.server.idle_timeout.as_secs() < 10 {
        return Err("idle_timeout must be at least 10 seconds".into());
    }
    
    Ok(config)
}
```

## Load Balancing

### HAProxy Configuration

```
# haproxy.cfg
global
    daemon
    maxconn 4096
    log stdout local0 info

defaults
    mode tcp
    option tcplog
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms

frontend quiccrunch_frontend
    bind *:50051
    mode tcp
    default_backend quiccrunch_backend

backend quiccrunch_backend
    mode tcp
    balance roundrobin
    option tcp-check
    tcp-check connect
    server quic1 10.0.1.10:50051 check
    server quic2 10.0.1.11:50051 check
    server quic3 10.0.1.12:50051 check
```

### NGINX Load Balancer

```nginx
# nginx.conf
upstream quiccrunch_backend {
    server 10.0.1.10:50051;
    server 10.0.1.11:50051;
    server 10.0.1.12:50051;
}

server {
    listen 50051 udp;
    proxy_pass quiccrunch_backend;
    proxy_timeout 1s;
    proxy_responses 1;
    proxy_bind $remote_addr transparent;
}
```

## TLS Configuration

### Certificate Management

```bash
# Generate CA certificate
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days 3650 -key ca.key -out ca.crt \
    -subj "/C=US/ST=CA/L=San Francisco/O=QuicCrunch/CN=QuicCrunch CA"

# Generate server certificate
openssl genrsa -out server.key 4096
openssl req -new -key server.key -out server.csr \
    -subj "/C=US/ST=CA/L=San Francisco/O=QuicCrunch/CN=quiccrunch.example.com"

# Sign server certificate
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt -days 365 \
    -extensions v3_req -extfile openssl.cnf

# Verify certificate
openssl verify -CAfile ca.crt server.crt
```

### Let's Encrypt Integration

```bash
# Install certbot
sudo apt-get install certbot

# Generate certificate
sudo certbot certonly --standalone \
    --preferred-challenges http \
    -d quiccrunch.example.com

# Auto-renewal
echo "0 12 * * * /usr/bin/certbot renew --quiet" | sudo crontab -
```

## Monitoring Setup

### Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "quiccrunch_rules.yml"

scrape_configs:
  - job_name: 'quiccrunch'
    static_configs:
      - targets: ['quiccrunch-service:8080']
    metrics_path: /metrics
    scrape_interval: 10s
    scrape_timeout: 5s

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

### Grafana Dashboard

```json
{
  "dashboard": {
    "title": "QuicCrunch Metrics",
    "panels": [
      {
        "title": "RPC Requests per Second",
        "type": "stat",
        "targets": [
          {
            "expr": "rate(quiccrunch_rpc_requests_total[5m])"
          }
        ]
      },
      {
        "title": "Average Latency",
        "type": "stat",
        "targets": [
          {
            "expr": "rate(quiccrunch_rpc_duration_seconds_sum[5m]) / rate(quiccrunch_rpc_duration_seconds_count[5m])"
          }
        ]
      }
    ]
  }
}
```

## Troubleshooting

### Common Issues

1. **UDP Traffic Blocked**
   ```bash
   # Check firewall rules
   sudo iptables -L -n | grep 50051
   
   # Allow UDP traffic
   sudo iptables -A INPUT -p udp --dport 50051 -j ACCEPT
   ```

2. **Certificate Issues**
   ```bash
   # Verify certificate
   openssl x509 -in server.crt -text -noout
   
   # Check certificate chain
   openssl verify -CAfile ca.crt server.crt
   ```

3. **Performance Issues**
   ```bash
   # Check UDP buffer sizes
   cat /proc/sys/net/core/rmem_max
   cat /proc/sys/net/core/wmem_max
   
   # Monitor network traffic
   sudo netstat -su | grep -i udp
   ```

### Deployment Validation

```bash
#!/bin/bash
# deployment-validation.sh

set -e

echo "🔍 Validating QuicCrunch deployment..."

# Check if service is running
if ! curl -f http://localhost:8080/health; then
    echo "❌ Health check failed"
    exit 1
fi

# Check metrics endpoint
if ! curl -f http://localhost:8080/metrics | grep -q "quiccrunch_"; then
    echo "❌ Metrics endpoint not responding"
    exit 1
fi

# Test RPC connectivity
if ! timeout 10 ./quiccrunch-cli --server localhost:50051 ping; then
    echo "❌ RPC connectivity test failed"
    exit 1
fi

echo "✅ Deployment validation successful"
```

## Security Considerations

- **Network Policies**: Implement Kubernetes network policies to restrict traffic
- **Pod Security Standards**: Use restricted pod security standards
- **Secrets Management**: Use external secret management systems (Vault, AWS Secrets Manager)
- **Image Security**: Regularly scan container images for vulnerabilities
- **TLS**: Always use TLS in production with proper certificate management

---

This deployment guide provides comprehensive coverage for production deployments. For specific deployment scenarios or troubleshooting, refer to the [Operations Guide](README.md).