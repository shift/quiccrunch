#!/bin/bash
set -euo pipefail

# QuicCrunch Infrastructure Overview Script
# Shows current deployment status and available commands

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

print_header() {
    echo -e "${BOLD}${BLUE}"
    echo "╔════════════════════════════════════════════════════════════════╗"
    echo "║                    QuicCrunch Infrastructure                   ║"
    echo "║              Containerization & Deployment Stack              ║"
    echo "╚════════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
}

print_section() {
    echo -e "${BOLD}${CYAN}▶ $1${NC}"
    echo -e "${CYAN}$( printf '─%.0s' {1..60} )${NC}"
}

check_prerequisites() {
    print_section "Prerequisites Check"
    
    local missing=0
    
    # Check Docker
    if command -v docker >/dev/null 2>&1; then
        if docker info >/dev/null 2>&1; then
            echo -e "${GREEN}✓ Docker: $(docker --version | cut -d' ' -f3)${NC}"
        else
            echo -e "${RED}✗ Docker daemon not running${NC}"
            ((missing++))
        fi
    else
        echo -e "${RED}✗ Docker not installed${NC}"
        ((missing++))
    fi
    
    # Check Docker Compose
    if command -v docker-compose >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Docker Compose: $(docker-compose --version | cut -d' ' -f4)${NC}"
    else
        echo -e "${RED}✗ Docker Compose not installed${NC}"
        ((missing++))
    fi
    
    # Check Kubernetes (optional)
    if command -v kubectl >/dev/null 2>&1; then
        echo -e "${GREEN}✓ kubectl: $(kubectl version --client -o json 2>/dev/null | jq -r '.clientVersion.gitVersion // "unknown"')${NC}"
    else
        echo -e "${YELLOW}⚠ kubectl not installed (optional for K8s deployments)${NC}"
    fi
    
    # Check available ports
    local ports=(50051 9090 9091 3000)
    local port_conflicts=0
    
    for port in "${ports[@]}"; do
        if ss -tuln | grep -q ":$port "; then
            echo -e "${YELLOW}⚠ Port $port is in use${NC}"
            ((port_conflicts++))
        fi
    done
    
    if [[ $port_conflicts -eq 0 ]]; then
        echo -e "${GREEN}✓ All required ports are available${NC}"
    fi
    
    echo ""
    if [[ $missing -gt 0 ]]; then
        echo -e "${RED}Missing $missing required dependencies. Please install them first.${NC}"
        return 1
    fi
}

show_infrastructure_overview() {
    print_section "Infrastructure Components"
    
    echo -e "${BOLD}🐳 Docker Images:${NC}"
    echo "  • quiccrunch/server - Optimized QUIC server (~50MB)"
    echo "  • quiccrunch/cli    - Client tools and utilities (~45MB)"
    echo ""
    
    echo -e "${BOLD}📊 Monitoring Stack:${NC}"
    echo "  • Prometheus - Metrics collection and alerting"
    echo "  • Grafana    - Visualization and dashboards"
    echo "  • AlertManager - Alert routing and notifications"
    echo ""
    
    echo -e "${BOLD}🔧 Deployment Options:${NC}"
    echo "  • Docker Compose - Local development and simple production"
    echo "  • Kubernetes     - Production-grade orchestration with auto-scaling"
    echo "  • CI/CD Pipeline - Automated builds and deployments"
    echo ""
}

show_quick_commands() {
    print_section "Quick Start Commands"
    
    echo -e "${BOLD}Development Environment:${NC}"
    echo -e "${CYAN}  ./deploy/scripts/deploy.sh dev${NC}"
    echo "    └─ Starts full stack with monitoring"
    echo ""
    
    echo -e "${BOLD}Production Deployment:${NC}"
    echo -e "${CYAN}  ./deploy/scripts/deploy.sh prod v1.0.0${NC}"
    echo "    └─ Deploys specific version to production"
    echo ""
    
    echo -e "${BOLD}Health Monitoring:${NC}"
    echo -e "${CYAN}  ./deploy/scripts/health-check.sh [environment]${NC}"
    echo "    └─ Comprehensive health and performance checks"
    echo ""
    
    echo -e "${BOLD}Kubernetes Deployment:${NC}"
    echo -e "${CYAN}  kubectl apply -k k8s/overlays/dev${NC}"
    echo "    └─ Deploy to Kubernetes development namespace"
    echo ""
}

show_service_endpoints() {
    print_section "Service Endpoints"
    
    echo -e "${BOLD}Core Services:${NC}"
    echo "  • QuicCrunch Server: localhost:50051 (UDP)"
    echo "  • Metrics Endpoint:  localhost:9090/metrics"
    echo ""
    
    echo -e "${BOLD}Monitoring Dashboards:${NC}"
    echo "  • Prometheus:  http://localhost:9091"
    echo "  • Grafana:     http://localhost:3000 (admin/admin)"
    echo "  • AlertManager: http://localhost:9093"
    echo ""
}

check_current_status() {
    print_section "Current Deployment Status"
    
    # Check if any QuicCrunch containers are running
    local containers
    containers=$(docker ps --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" | grep -i quiccrunch || true)
    
    if [[ -n "$containers" ]]; then
        echo -e "${GREEN}✓ Active QuicCrunch containers:${NC}"
        echo "$containers" | column -t
        echo ""
        
        # Check health of services
        echo -e "${BOLD}Service Health:${NC}"
        
        if curl -sf --max-time 5 http://localhost:9090/metrics >/dev/null 2>&1; then
            echo -e "${GREEN}✓ QuicCrunch Server responding${NC}"
        else
            echo -e "${RED}✗ QuicCrunch Server not responding${NC}"
        fi
        
        if curl -sf --max-time 5 http://localhost:9091/-/healthy >/dev/null 2>&1; then
            echo -e "${GREEN}✓ Prometheus healthy${NC}"
        else
            echo -e "${YELLOW}⚠ Prometheus not accessible${NC}"
        fi
        
        if curl -sf --max-time 5 http://localhost:3000/api/health >/dev/null 2>&1; then
            echo -e "${GREEN}✓ Grafana healthy${NC}"
        else
            echo -e "${YELLOW}⚠ Grafana not accessible${NC}"
        fi
    else
        echo -e "${YELLOW}No QuicCrunch containers currently running${NC}"
        echo "Use './deploy/scripts/deploy.sh dev' to start development environment"
    fi
    echo ""
}

show_documentation_links() {
    print_section "Documentation & Resources"
    
    echo -e "${BOLD}📚 Key Documentation:${NC}"
    echo "  • Deployment Guide:    deploy/README.md"
    echo "  • Getting Started:     docs/getting-started.md"
    echo "  • Performance Tuning: docs/tuning.md"
    echo "  • API Reference:       docs/api/"
    echo ""
    
    echo -e "${BOLD}🔍 Analysis Reports:${NC}"
    echo "  • SDLC Gap Analysis:   gaps/MULTI_ROLE_SDLC_GAP_ANALYSIS.md"
    echo "  • Performance Report:  gaps/PERFORMANCE_ANALYSIS_REPORT.md"
    echo "  • Backend Comparison:  gaps/BACKEND_COMPARISON_REPORT.md"
    echo ""
}

show_troubleshooting() {
    print_section "Common Troubleshooting"
    
    echo -e "${BOLD}🔧 Common Issues:${NC}"
    echo "  • Port conflicts: Check if ports 50051, 9090, 9091, 3000 are free"
    echo "  • Docker issues:  Restart Docker daemon and try again"
    echo "  • Build failures: Clear Docker cache with 'docker system prune'"
    echo "  • Permission:     Ensure user is in docker group"
    echo ""
    
    echo -e "${BOLD}🧪 Debugging Commands:${NC}"
    echo -e "${CYAN}  docker-compose logs quiccrunch-server${NC}  # View server logs"
    echo -e "${CYAN}  docker stats${NC}                           # Resource usage"
    echo -e "${CYAN}  ./deploy/scripts/health-check.sh dev --verbose${NC}  # Detailed health info"
    echo ""
}

show_next_steps() {
    print_section "Next Steps"
    
    echo -e "${BOLD}🚀 Getting Started:${NC}"
    echo "  1. Run prerequisite check and fix any issues"
    echo "  2. Start development environment: ./deploy/scripts/deploy.sh dev"
    echo "  3. Wait for services to start (2-3 minutes)"
    echo "  4. Open Grafana dashboard: http://localhost:3000"
    echo "  5. Run health checks: ./deploy/scripts/health-check.sh dev"
    echo ""
    
    echo -e "${BOLD}📈 Production Deployment:${NC}"
    echo "  1. Build and tag release: git tag v1.0.0 && git push --tags"
    echo "  2. Deploy to production: ./deploy/scripts/deploy.sh prod v1.0.0"
    echo "  3. Verify deployment: ./deploy/scripts/health-check.sh prod"
    echo "  4. Monitor metrics and set up alerting"
    echo ""
}

# Main execution
main() {
    clear
    print_header
    
    case "${1:-}" in
        --check|check)
            check_prerequisites
            check_current_status
            ;;
        --docs|docs)
            show_documentation_links
            ;;
        --troubleshoot|troubleshoot)
            show_troubleshooting
            ;;
        --help|-h|help)
            echo "QuicCrunch Infrastructure Overview"
            echo ""
            echo "Usage: $0 [command]"
            echo ""
            echo "Commands:"
            echo "  check         - Check prerequisites and current status"
            echo "  docs          - Show documentation links"
            echo "  troubleshoot  - Show troubleshooting guide"
            echo "  help          - Show this help message"
            echo ""
            echo "Without arguments, shows full overview."
            ;;
        *)
            check_prerequisites
            echo ""
            show_infrastructure_overview
            show_quick_commands
            show_service_endpoints
            check_current_status
            show_next_steps
            ;;
    esac
}

# Handle script execution
main "$@"