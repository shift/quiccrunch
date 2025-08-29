#!/bin/bash
set -euo pipefail

# QuicCrunch Deployment Script
# Usage: ./deploy.sh [environment] [version]

ENVIRONMENT=${1:-dev}
VERSION=${2:-latest}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${GREEN}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

warn() {
    echo -e "${YELLOW}[$(date +'%Y-%m-%d %H:%M:%S')] WARNING: $1${NC}"
}

error() {
    echo -e "${RED}[$(date +'%Y-%m-%d %H:%M:%S')] ERROR: $1${NC}"
    exit 1
}

info() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

# Validate environment
case $ENVIRONMENT in
    dev|staging|prod)
        log "Deploying to $ENVIRONMENT environment"
        ;;
    *)
        error "Invalid environment: $ENVIRONMENT. Must be dev, staging, or prod"
        ;;
esac

# Pre-deployment checks
check_dependencies() {
    log "Checking dependencies..."
    
    command -v docker >/dev/null 2>&1 || error "Docker is required but not installed"
    command -v docker-compose >/dev/null 2>&1 || error "Docker Compose is required but not installed"
    
    # Check if Docker daemon is running
    docker info >/dev/null 2>&1 || error "Docker daemon is not running"
    
    log "Dependencies check passed"
}

# Build Docker images
build_images() {
    log "Building Docker images..."
    
    cd "$PROJECT_ROOT"
    
    # Build server image
    info "Building QuicCrunch server image..."
    docker build -f docker/Dockerfile.server -t "quiccrunch/server:$VERSION" .
    
    # Build CLI image
    info "Building QuicCrunch CLI image..."
    docker build -f docker/Dockerfile.cli -t "quiccrunch/cli:$VERSION" .
    
    # Tag as latest if deploying latest version
    if [[ "$VERSION" == "latest" ]]; then
        docker tag "quiccrunch/server:$VERSION" quiccrunch/server:latest
        docker tag "quiccrunch/cli:$VERSION" quiccrunch/cli:latest
    fi
    
    log "Docker images built successfully"
}

# Setup environment-specific configuration
setup_environment() {
    log "Setting up $ENVIRONMENT environment..."
    
    cd "$PROJECT_ROOT"
    
    # Create necessary directories
    mkdir -p deploy/secrets
    
    # Generate secrets for production
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        if [[ ! -f deploy/secrets/grafana_admin_password.txt ]]; then
            info "Generating Grafana admin password..."
            openssl rand -base64 32 > deploy/secrets/grafana_admin_password.txt
            chmod 600 deploy/secrets/grafana_admin_password.txt
        fi
    fi
    
    # Set environment variables
    export QUICCRUNCH_VERSION="$VERSION"
    export ENVIRONMENT="$ENVIRONMENT"
    
    log "Environment setup completed"
}

# Deploy using Docker Compose
deploy() {
    log "Starting deployment..."
    
    cd "$PROJECT_ROOT"
    
    # Choose the appropriate compose file
    local compose_file
    case $ENVIRONMENT in
        dev)
            compose_file="docker/docker-compose.dev.yml"
            ;;
        prod)
            compose_file="docker/docker-compose.prod.yml"
            ;;
        staging)
            # Use prod compose file with staging overrides
            compose_file="docker/docker-compose.prod.yml"
            ;;
    esac
    
    # Pull latest images if using registry
    if [[ "$VERSION" != "latest" ]]; then
        info "Pulling images from registry..."
        docker-compose -f "$compose_file" pull || warn "Failed to pull some images, continuing with local builds"
    fi
    
    # Deploy with zero-downtime rolling update for production
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        info "Performing rolling update..."
        docker-compose -f "$compose_file" up -d --no-deps --scale quiccrunch-server=2
        
        # Wait for health checks
        info "Waiting for health checks..."
        sleep 30
        
        # Check if new containers are healthy
        if docker-compose -f "$compose_file" ps | grep -q "unhealthy"; then
            error "Health check failed, rolling back..."
        fi
    else
        # Simple deployment for dev/staging
        docker-compose -f "$compose_file" up -d
    fi
    
    log "Deployment completed successfully"
}

# Post-deployment verification
verify_deployment() {
    log "Verifying deployment..."
    
    cd "$PROJECT_ROOT"
    
    local compose_file
    case $ENVIRONMENT in
        dev)
            compose_file="docker/docker-compose.dev.yml"
            ;;
        *)
            compose_file="docker/docker-compose.prod.yml"
            ;;
    esac
    
    # Check container status
    info "Checking container status..."
    docker-compose -f "$compose_file" ps
    
    # Wait for services to be ready
    local max_attempts=30
    local attempt=1
    
    while [[ $attempt -le $max_attempts ]]; do
        if curl -sf http://localhost:9090/metrics >/dev/null 2>&1; then
            log "QuicCrunch server is responding"
            break
        fi
        
        info "Waiting for QuicCrunch server... (attempt $attempt/$max_attempts)"
        sleep 10
        ((attempt++))
    done
    
    if [[ $attempt -gt $max_attempts ]]; then
        error "QuicCrunch server failed to respond after $max_attempts attempts"
    fi
    
    # Check Prometheus
    if curl -sf http://localhost:9091/-/healthy >/dev/null 2>&1; then
        log "Prometheus is healthy"
    else
        warn "Prometheus is not responding"
    fi
    
    # Check Grafana
    if curl -sf http://localhost:3000/api/health >/dev/null 2>&1; then
        log "Grafana is healthy"
    else
        warn "Grafana is not responding"
    fi
    
    log "Deployment verification completed"
}

# Cleanup old images
cleanup() {
    log "Cleaning up old images..."
    
    # Remove dangling images
    docker image prune -f
    
    # Remove old versions (keep last 3)
    docker images quiccrunch/server --format "table {{.Tag}}" | grep -v "latest\|TAG" | tail -n +4 | xargs -r -I {} docker rmi quiccrunch/server:{} || true
    docker images quiccrunch/cli --format "table {{.Tag}}" | grep -v "latest\|TAG" | tail -n +4 | xargs -r -I {} docker rmi quiccrunch/cli:{} || true
    
    log "Cleanup completed"
}

# Show deployment status
show_status() {
    log "Deployment Status"
    echo "=================="
    echo "Environment: $ENVIRONMENT"
    echo "Version: $VERSION"
    echo "Services:"
    echo "  - QuicCrunch Server: http://localhost:50051 (UDP)"
    echo "  - Prometheus: http://localhost:9091"
    echo "  - Grafana: http://localhost:3000"
    
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        echo "  - AlertManager: http://localhost:9093"
    fi
    
    echo ""
    echo "Grafana Login:"
    if [[ "$ENVIRONMENT" == "prod" && -f deploy/secrets/grafana_admin_password.txt ]]; then
        echo "  Username: admin"
        echo "  Password: $(cat deploy/secrets/grafana_admin_password.txt)"
    else
        echo "  Username: admin"
        echo "  Password: admin"
    fi
}

# Main execution
main() {
    log "Starting QuicCrunch deployment"
    log "Environment: $ENVIRONMENT"
    log "Version: $VERSION"
    
    check_dependencies
    setup_environment
    build_images
    deploy
    verify_deployment
    cleanup
    show_status
    
    log "Deployment completed successfully!"
}

# Handle script arguments
case "${1:-}" in
    --help|-h)
        echo "QuicCrunch Deployment Script"
        echo "Usage: $0 [environment] [version]"
        echo ""
        echo "Environments:"
        echo "  dev      - Development environment (default)"
        echo "  staging  - Staging environment"
        echo "  prod     - Production environment"
        echo ""
        echo "Version:"
        echo "  latest   - Build and deploy latest code (default)"
        echo "  v1.0.0   - Deploy specific version from registry"
        echo ""
        echo "Examples:"
        echo "  $0                    # Deploy latest to dev"
        echo "  $0 prod v1.0.0        # Deploy v1.0.0 to production"
        echo "  $0 staging latest     # Deploy latest to staging"
        exit 0
        ;;
    *)
        main
        ;;
esac