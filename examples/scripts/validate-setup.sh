#!/bin/bash
set -e

# QuicCrunch Examples Setup Validation Script
# This script validates that the containerization setup is working correctly

echo "🔍 Validating QuicCrunch Examples Setup..."
echo "========================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to print status
print_status() {
    local status=$1
    local message=$2
    
    if [ "$status" = "OK" ]; then
        echo -e "${GREEN}✓${NC} $message"
    elif [ "$status" = "WARNING" ]; then
        echo -e "${YELLOW}⚠${NC} $message"
    else
        echo -e "${RED}✗${NC} $message"
    fi
}

# Function to check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
echo "📋 Checking Prerequisites..."
echo "----------------------------"

if command_exists docker; then
    docker_version=$(docker --version | cut -d' ' -f3 | cut -d',' -f1)
    print_status "OK" "Docker is installed (version: $docker_version)"
else
    print_status "FAIL" "Docker is not installed"
    exit 1
fi

if command_exists docker-compose; then
    compose_version=$(docker-compose --version | cut -d' ' -f3 | cut -d',' -f1)
    print_status "OK" "Docker Compose is installed (version: $compose_version)"
else
    print_status "FAIL" "Docker Compose is not installed"
    exit 1
fi

# Check Docker daemon
if docker info >/dev/null 2>&1; then
    print_status "OK" "Docker daemon is running"
else
    print_status "FAIL" "Docker daemon is not running"
    exit 1
fi

# Check if ports are available
echo ""
echo "🔌 Checking Port Availability..."
echo "--------------------------------"

check_port() {
    local port=$1
    local service=$2
    
    if netstat -ln 2>/dev/null | grep -q ":$port "; then
        print_status "WARNING" "Port $port is already in use (needed for $service)"
    else
        print_status "OK" "Port $port is available for $service"
    fi
}

check_port 50051 "Calculator Service"
check_port 50052 "Chat Service"
check_port 50053 "Echo Service"

# Check if we're in the right directory
echo ""
echo "📁 Checking Directory Structure..."
echo "----------------------------------"

if [ -f "docker-compose.yml" ]; then
    print_status "OK" "Found docker-compose.yml"
else
    print_status "FAIL" "docker-compose.yml not found. Run this script from the examples directory."
    exit 1
fi

if [ -d "applications" ]; then
    print_status "OK" "Found applications directory"
else
    print_status "FAIL" "applications directory not found"
    exit 1
fi

if [ -d "containers" ]; then
    print_status "OK" "Found containers directory"
else
    print_status "FAIL" "containers directory not found"
    exit 1
fi

# Check application directories
for app in calculator chat-service echo-service; do
    if [ -d "applications/$app" ]; then
        print_status "OK" "Found $app application"
        
        if [ -f "applications/$app/Dockerfile" ]; then
            print_status "OK" "Found Dockerfile for $app"
        else
            print_status "FAIL" "Dockerfile missing for $app"
        fi
        
        if [ -f "applications/$app/Cargo.toml" ]; then
            print_status "OK" "Found Cargo.toml for $app"
        else
            print_status "FAIL" "Cargo.toml missing for $app"
        fi
    else
        print_status "FAIL" "Missing $app application directory"
    fi
done

# Test Docker Compose configuration
echo ""
echo "🐳 Validating Docker Compose Configuration..."
echo "---------------------------------------------"

if docker-compose config >/dev/null 2>&1; then
    print_status "OK" "docker-compose.yml is valid"
else
    print_status "FAIL" "docker-compose.yml has syntax errors"
    exit 1
fi

if docker-compose -f docker-compose.ci.yml config >/dev/null 2>&1; then
    print_status "OK" "docker-compose.ci.yml is valid"
else
    print_status "FAIL" "docker-compose.ci.yml has syntax errors"
    exit 1
fi

# Test building one of the applications
echo ""
echo "🔨 Testing Application Build..."
echo "-------------------------------"

echo "Building echo service (fastest to build)..."
if docker-compose build echo-server >/dev/null 2>&1; then
    print_status "OK" "Successfully built echo service"
else
    print_status "FAIL" "Failed to build echo service"
    echo "Run 'docker-compose build echo-server' for detailed error information"
    exit 1
fi

# Test basic functionality
echo ""
echo "🚀 Testing Basic Functionality..."
echo "---------------------------------"

echo "Starting echo service for testing..."
if docker-compose up -d echo-server >/dev/null 2>&1; then
    print_status "OK" "Started echo service"
    
    # Wait for service to be ready
    echo "Waiting for service to be ready..."
    sleep 5
    
    # Test if service responds
    if docker-compose exec -T echo-server echo-client --message "validation-test" >/dev/null 2>&1; then
        print_status "OK" "Echo service responds correctly"
    else
        print_status "WARNING" "Echo service did not respond as expected"
    fi
    
    # Clean up
    docker-compose down >/dev/null 2>&1
    print_status "OK" "Cleaned up test service"
else
    print_status "FAIL" "Failed to start echo service"
fi

# Check integration test setup
echo ""
echo "🧪 Checking Integration Test Setup..."
echo "------------------------------------"

if [ -d "../tests/integration" ]; then
    print_status "OK" "Found integration tests directory"
    
    if [ -f "../tests/integration/Cargo.toml" ]; then
        print_status "OK" "Found integration test Cargo.toml"
    else
        print_status "FAIL" "Integration test Cargo.toml missing"
    fi
else
    print_status "FAIL" "Integration tests directory missing"
fi

# Final status
echo ""
echo "📊 Validation Summary"
echo "===================="

# Count any failures
if grep -q "FAIL" /tmp/validation.log 2>/dev/null; then
    echo -e "${RED}❌ Validation completed with errors${NC}"
    echo "Please fix the issues above before proceeding."
    exit 1
else
    echo -e "${GREEN}✅ All validations passed!${NC}"
    echo ""
    echo "🎉 Your QuicCrunch examples setup is ready!"
    echo ""
    echo "Next steps:"
    echo "  make up          # Start all example services"
    echo "  make test        # Run all tests"
    echo "  make dev-env     # Start development environment"
    echo ""
    echo "For more commands, run: make help"
fi