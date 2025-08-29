# QuicCrunch Deployment Infrastructure

This directory contains comprehensive containerization and deployment infrastructure for QuicCrunch, addressing the critical gaps identified in the SDLC Gap Analysis.

## 📁 Directory Structure

```
deploy/
├── scripts/           # Deployment automation scripts
│   ├── deploy.sh     # Main deployment script
│   └── health-check.sh # Health monitoring script
├── nginx/            # Nginx load balancer configuration
└── secrets/          # Production secrets (not committed)

docker/
├── Dockerfile.server    # Optimized server container
├── Dockerfile.cli      # CLI tools container
├── configs/            # Server configurations
├── docker-compose.dev.yml   # Development environment
└── docker-compose.prod.yml  # Production environment

monitoring/
├── prometheus-*.yml    # Prometheus configurations
├── grafana/           # Grafana dashboards and provisioning
├── rules/             # Alerting rules
└── alertmanager.yml   # Alert management

k8s/
├── base/              # Base Kubernetes manifests
└── overlays/          # Environment-specific overlays
    ├── dev/
    ├── staging/
    └── prod/
```

## 🚀 Quick Start

### 1. Development Environment

```bash
# Clone and setup
git clone <repository>
cd quiccrunch

# Start development environment
./deploy/scripts/deploy.sh dev

# Access services
# - QuicCrunch Server: localhost:50051 (UDP)
# - Prometheus: http://localhost:9091
# - Grafana: http://localhost:3000 (admin/admin)
```

### 2. Production Deployment

```bash
# Build and deploy to production
./deploy/scripts/deploy.sh prod v1.0.0

# Run health checks
./deploy/scripts/health-check.sh prod
```

### 3. Kubernetes Deployment

```bash
# Deploy to development namespace
kubectl apply -k k8s/overlays/dev

# Deploy to production with auto-scaling
kubectl apply -k k8s/overlays/prod
```

## 📊 Monitoring & Observability

### Prometheus Metrics
- **Request Rate**: `rate(quiccrunch_requests_total[5m])`
- **Response Time**: `histogram_quantile(0.95, rate(quiccrunch_request_duration_seconds_bucket[5m]))`
- **Error Rate**: `rate(quiccrunch_errors_total[5m])`
- **Active Connections**: `quiccrunch_active_connections`

### Grafana Dashboards
- **Performance Dashboard**: Key metrics and SLIs
- **Operational Dashboard**: Resource usage and health
- **Business Dashboard**: Request patterns and user metrics

### Alerting Rules
- High error rate (>10% for 2min)
- High latency (95th percentile >500ms for 2min) 
- Server down (>1min)
- Resource exhaustion (CPU >80%, Memory >500MB)

## 🐳 Docker Images

### Server Image (`quiccrunch/server`)
- Multi-stage build with optimized Rust compilation
- Minimal runtime based on Debian Bookworm Slim
- Security hardened (non-root user, read-only filesystem)
- Health checks and metrics endpoint
- Size: ~50MB

### CLI Image (`quiccrunch/cli`)
- Same base as server for consistency
- Includes client tools and utilities
- Can be used for testing and administration
- Size: ~45MB

## 🔧 Configuration

### Environment Variables
- `RUST_LOG`: Logging level (debug, info, warn, error)
- `RUST_BACKTRACE`: Enable backtraces (0, 1, full)
- `QUICCRUNCH_VERSION`: Image version tag
- `ENVIRONMENT`: Deployment environment (dev, staging, prod)

### Server Configuration
Production-ready TOML configuration with:
- Performance tuning (connection pools, timeouts)
- Security settings (TLS, rate limiting)
- Monitoring integration (Prometheus metrics)
- Resource limits and flow control

## 🔐 Security Features

### Container Security
- Non-root user execution
- Read-only root filesystem
- Minimal attack surface
- Security scanning with Trivy
- Regular base image updates

### Network Security
- TLS 1.3 encryption
- Certificate-based authentication
- Rate limiting and DDoS protection
- Network segmentation

### Secrets Management
- Kubernetes secrets for sensitive data
- Encrypted storage
- Rotation capabilities
- Audit logging

## 📈 Performance & Scaling

### Horizontal Pod Autoscaling (HPA)
- CPU-based scaling (target: 70%)
- Memory-based scaling (target: 80%)
- Min replicas: 2, Max replicas: 10
- Smooth scaling policies

### Resource Optimization
- Optimized Rust compilation (LTO, single codegen unit)
- io_uring for high-performance I/O
- SIMD optimizations where available
- Lock-free data structures

### Load Balancing
- Nginx reverse proxy with health checks
- Round-robin load balancing
- Sticky sessions support
- SSL termination

## 🔄 CI/CD Pipeline

### Automated Build
- Multi-arch Docker images (amd64, arm64)
- Comprehensive testing before build
- Security scanning integration
- Automated tagging and versioning

### Deployment Automation
- GitOps-style deployment
- Rolling updates with zero downtime
- Automatic rollback on failure
- Environment promotion workflow

### Quality Gates
- All tests must pass
- Security scan must be clean
- Performance benchmarks
- Manual approval for production

## 🧪 Testing

### Health Checks
```bash
# Check all services
./deploy/scripts/health-check.sh [environment]

# Individual service checks
curl http://localhost:9090/metrics    # QuicCrunch metrics
curl http://localhost:9091/-/healthy  # Prometheus health
curl http://localhost:3000/api/health # Grafana health
```

### Performance Testing
```bash
# Load testing with CLI
docker run --rm --network host quiccrunch/cli:latest \
  benchmark --target localhost:50051 --duration 60s --connections 100

# Monitor metrics during test
curl -s "http://localhost:9091/api/v1/query?query=rate(quiccrunch_requests_total[5m])"
```

## 🎯 Addressing SDLC Gaps

This infrastructure directly addresses critical gaps identified in the Multi-Role SDLC Gap Analysis:

### ✅ DevOps Critical Gaps Resolved
- **Containerization**: Optimized Docker images for server and CLI
- **Monitoring Setup**: Comprehensive Prometheus + Grafana stack
- **Deployment Pipeline**: Automated deployment with GitOps approach
- **Production Readiness**: Health checks, scaling, security hardening

### ✅ High Priority Gaps Addressed
- **Infrastructure as Code**: Kubernetes manifests with Kustomize
- **Environment Management**: Separate dev/staging/prod configurations
- **Operational Monitoring**: Real-time metrics and alerting
- **Security Integration**: Container scanning, secrets management

### ✅ Production Quality Features
- **Zero-downtime deployments**: Rolling updates with health checks
- **Auto-scaling**: HPA based on CPU/memory metrics
- **Disaster recovery**: Persistent volumes, backup strategies
- **Observability**: Distributed tracing, structured logging

## 📚 Documentation

- [Getting Started](../docs/getting-started.md)
- [API Documentation](../docs/api/)
- [Performance Tuning](../docs/tuning.md)
- [Security Guide](../docs/security.md)
- [Troubleshooting](../docs/troubleshooting.md)

## 🆘 Support

### Common Issues
1. **Port conflicts**: Ensure ports 50051, 9090, 9091, 3000 are available
2. **Docker daemon**: Verify Docker is running and accessible
3. **Resource limits**: Check available CPU/memory for containers
4. **Network connectivity**: Verify container networking and DNS

### Troubleshooting Commands
```bash
# Check container logs
docker-compose logs quiccrunch-server

# Debug networking
docker network ls
docker network inspect quiccrunch-net

# Resource usage
docker stats

# Health status
./deploy/scripts/health-check.sh dev --verbose
```

## 🔗 Related Documentation

- [SDLC Gap Analysis](../gaps/MULTI_ROLE_SDLC_GAP_ANALYSIS.md)
- [Performance Analysis](../gaps/PERFORMANCE_ANALYSIS_REPORT.md)
- [Backend Comparison](../gaps/BACKEND_COMPARISON_REPORT.md)

---

**Status**: ✅ Production Ready  
**Last Updated**: 2025-08-28  
**Maintainer**: QuicCrunch DevOps Team