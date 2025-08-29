#!/bin/bash
set -euo pipefail

# QuicCrunch Health Check Script
# Usage: ./health-check.sh [environment]

ENVIRONMENT=${1:-dev}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

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
}

info() {
    echo -e "${BLUE}[$(date +'%Y-%m-%d %H:%M:%S')] $1${NC}"
}

# Health check functions
check_quiccrunch_server() {
    info "Checking QuicCrunch server..."
    
    if curl -sf --max-time 10 http://localhost:9090/metrics >/dev/null 2>&1; then
        log "✓ QuicCrunch server is responding"
        return 0
    else
        error "✗ QuicCrunch server is not responding"
        return 1
    fi
}

check_prometheus() {
    info "Checking Prometheus..."
    
    if curl -sf --max-time 10 http://localhost:9091/-/healthy >/dev/null 2>&1; then
        log "✓ Prometheus is healthy"
        return 0
    else
        error "✗ Prometheus is not healthy"
        return 1
    fi
}

check_grafana() {
    info "Checking Grafana..."
    
    local response
    if response=$(curl -sf --max-time 10 http://localhost:3000/api/health 2>/dev/null); then
        if echo "$response" | grep -q '"database":"ok"'; then
            log "✓ Grafana is healthy"
            return 0
        else
            warn "✗ Grafana database issue: $response"
            return 1
        fi
    else
        error "✗ Grafana is not responding"
        return 1
    fi
}

check_alertmanager() {
    info "Checking AlertManager..."
    
    if curl -sf --max-time 10 http://localhost:9093/-/healthy >/dev/null 2>&1; then
        log "✓ AlertManager is healthy"
        return 0
    else
        error "✗ AlertManager is not healthy"
        return 1
    fi
}

check_docker_containers() {
    info "Checking Docker containers..."
    
    local compose_file
    case $ENVIRONMENT in
        dev)
            compose_file="docker/docker-compose.dev.yml"
            ;;
        *)
            compose_file="docker/docker-compose.prod.yml"
            ;;
    esac
    
    local failed_containers=0
    
    # Check each service
    local services=("quiccrunch-server" "prometheus" "grafana")
    
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        services+=("alertmanager" "nginx")
    fi
    
    for service in "${services[@]}"; do
        local container_name="${service}"
        if [[ "$ENVIRONMENT" == "dev" ]]; then
            container_name="${service}-dev"
        elif [[ "$ENVIRONMENT" == "prod" ]]; then
            container_name="${service}-prod"
        fi
        
        if docker ps --format "table {{.Names}}" | grep -q "$container_name"; then
            local status=$(docker inspect --format='{{.State.Health.Status}}' "$container_name" 2>/dev/null || echo "unknown")
            case $status in
                "healthy")
                    log "✓ $service container is healthy"
                    ;;
                "unhealthy")
                    error "✗ $service container is unhealthy"
                    ((failed_containers++))
                    ;;
                "starting")
                    warn "⏳ $service container is starting"
                    ;;
                *)
                    log "✓ $service container is running (no health check)"
                    ;;
            esac
        else
            error "✗ $service container is not running"
            ((failed_containers++))
        fi
    done
    
    if [[ $failed_containers -eq 0 ]]; then
        log "All containers are healthy"
        return 0
    else
        error "$failed_containers container(s) have issues"
        return 1
    fi
}

check_system_resources() {
    info "Checking system resources..."
    
    # Check disk space
    local disk_usage=$(df / | awk 'NR==2 {print $5}' | sed 's/%//')
    if [[ $disk_usage -gt 90 ]]; then
        error "✗ Disk usage is critical: ${disk_usage}%"
        return 1
    elif [[ $disk_usage -gt 80 ]]; then
        warn "⚠ Disk usage is high: ${disk_usage}%"
    else
        log "✓ Disk usage is normal: ${disk_usage}%"
    fi
    
    # Check memory usage
    local mem_usage=$(free | awk 'NR==2{printf "%.0f", $3*100/$2}')
    if [[ $mem_usage -gt 90 ]]; then
        error "✗ Memory usage is critical: ${mem_usage}%"
        return 1
    elif [[ $mem_usage -gt 80 ]]; then
        warn "⚠ Memory usage is high: ${mem_usage}%"
    else
        log "✓ Memory usage is normal: ${mem_usage}%"
    fi
    
    # Check load average
    local load_avg=$(uptime | awk -F'load average:' '{print $2}' | awk '{print $1}' | sed 's/,//')
    local cpu_cores=$(nproc)
    local load_per_core=$(echo "$load_avg / $cpu_cores" | bc -l)
    
    if (( $(echo "$load_per_core > 2.0" | bc -l) )); then
        error "✗ Load average is critical: $load_avg (${load_per_core} per core)"
        return 1
    elif (( $(echo "$load_per_core > 1.0" | bc -l) )); then
        warn "⚠ Load average is high: $load_avg (${load_per_core} per core)"
    else
        log "✓ Load average is normal: $load_avg (${load_per_core} per core)"
    fi
    
    return 0
}

# Performance test
run_performance_test() {
    info "Running basic performance test..."
    
    # Simple ping test using quiccrunch CLI
    if docker run --rm --network container:quiccrunch-server-${ENVIRONMENT} quiccrunch/cli:latest ping localhost:50051 --count 5 >/dev/null 2>&1; then
        log "✓ Performance test passed"
        return 0
    else
        error "✗ Performance test failed"
        return 1
    fi
}

# Generate health report
generate_report() {
    local timestamp=$(date +'%Y-%m-%d %H:%M:%S')
    local report_file="/tmp/quiccrunch-health-report-${ENVIRONMENT}-$(date +'%Y%m%d-%H%M%S').txt"
    
    {
        echo "QuicCrunch Health Check Report"
        echo "=============================="
        echo "Environment: $ENVIRONMENT"
        echo "Timestamp: $timestamp"
        echo ""
        
        echo "Service Status:"
        docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" | grep quiccrunch
        echo ""
        
        echo "Resource Usage:"
        echo "Disk: $(df / | awk 'NR==2 {print $5}')"
        echo "Memory: $(free -h | awk 'NR==2{printf "%s/%s (%.0f%%)", $3,$2,$3*100/$2}')"
        echo "Load: $(uptime | awk -F'load average:' '{print $2}')"
        echo ""
        
        echo "Docker Images:"
        docker images quiccrunch/* --format "table {{.Repository}}:{{.Tag}}\t{{.Size}}\t{{.CreatedAt}}"
        echo ""
        
        echo "Volumes:"
        docker volume ls | grep quiccrunch
        
    } > "$report_file"
    
    log "Health report generated: $report_file"
}

# Main health check
main() {
    log "Starting QuicCrunch health check for $ENVIRONMENT environment"
    
    local failed_checks=0
    
    # Core service checks
    check_docker_containers || ((failed_checks++))
    check_quiccrunch_server || ((failed_checks++))
    check_prometheus || ((failed_checks++))
    check_grafana || ((failed_checks++))
    
    # Production-specific checks
    if [[ "$ENVIRONMENT" == "prod" ]]; then
        check_alertmanager || ((failed_checks++))
    fi
    
    # System checks
    check_system_resources || ((failed_checks++))
    
    # Performance test
    run_performance_test || ((failed_checks++))
    
    # Generate report
    generate_report
    
    # Summary
    echo ""
    if [[ $failed_checks -eq 0 ]]; then
        log "🎉 All health checks passed!"
        exit 0
    else
        error "❌ $failed_checks health check(s) failed"
        exit 1
    fi
}

# Handle script arguments
case "${1:-}" in
    --help|-h)
        echo "QuicCrunch Health Check Script"
        echo "Usage: $0 [environment]"
        echo ""
        echo "Environments:"
        echo "  dev      - Development environment (default)"
        echo "  staging  - Staging environment"
        echo "  prod     - Production environment"
        echo ""
        echo "Exit codes:"
        echo "  0 - All checks passed"
        echo "  1 - One or more checks failed"
        exit 0
        ;;
    *)
        main
        ;;
esac