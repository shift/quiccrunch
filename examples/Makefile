# QuicCrunch Examples Makefile
# Convenient commands for development and testing

.PHONY: help build up down test test-integration test-performance clean dev-env logs

# Default target
help:
	@echo "QuicCrunch Examples - Available commands:"
	@echo ""
	@echo "  make build              - Build all example applications"
	@echo "  make up                 - Start all example services"
	@echo "  make down               - Stop all services"
	@echo "  make test               - Run all tests"
	@echo "  make test-integration   - Run integration tests only"
	@echo "  make test-performance   - Run performance benchmarks"
	@echo "  make dev-env            - Start development environment"
	@echo "  make logs               - Show logs from all services"
	@echo "  make clean              - Clean up containers and volumes"
	@echo "  make validate           - Validate the setup"
	@echo ""

# Build all applications
build:
	@echo "Building all QuicCrunch example applications..."
	docker-compose build

# Start all example services
up:
	@echo "Starting all QuicCrunch example services..."
	docker-compose up -d
	@echo "Waiting for services to be ready..."
	@sleep 10
	@docker-compose ps
	@echo ""
	@echo "Services are running! Try these commands:"
	@echo "  Calculator: docker-compose exec calculator-server calculator-client --operation add --a 5 --b 3"
	@echo "  Echo: docker-compose exec echo-server echo-client --message 'Hello!'"
	@echo "  Chat: docker-compose run --rm chat-server chat-client --user alice --interactive"

# Stop all services
down:
	@echo "Stopping all services..."
	docker-compose down

# Run all tests
test: test-unit test-integration

# Run unit tests
test-unit:
	@echo "Running unit tests..."
	docker-compose -f docker-compose.ci.yml --profile test up --abort-on-container-exit unit-tests

# Run integration tests
test-integration:
	@echo "Starting services for integration testing..."
	docker-compose -f docker-compose.ci.yml up -d calculator-server-ci chat-server-ci echo-server-ci
	@echo "Waiting for services to be ready..."
	@sleep 15
	@echo "Running integration tests..."
	docker-compose -f docker-compose.ci.yml --profile test up --abort-on-container-exit integration-tests
	@echo "Stopping test services..."
	docker-compose -f docker-compose.ci.yml down

# Run performance benchmarks
test-performance:
	@echo "Starting services for performance testing..."
	docker-compose -f docker-compose.ci.yml up -d calculator-server-ci chat-server-ci echo-server-ci
	@echo "Waiting for services to be ready..."
	@sleep 15
	@echo "Running performance benchmarks..."
	docker-compose -f docker-compose.ci.yml --profile benchmark up --abort-on-container-exit performance-tests
	@echo "Stopping test services..."
	docker-compose -f docker-compose.ci.yml down

# Start development environment
dev-env:
	@echo "Starting development environment..."
	docker-compose --profile dev up -d dev-tools
	@echo "Development environment ready!"
	@echo "Connect with: docker-compose exec dev-tools bash"

# Show logs from all services
logs:
	docker-compose logs -f

# Clean up everything
clean:
	@echo "Cleaning up containers and volumes..."
	docker-compose down -v --remove-orphans
	docker-compose -f docker-compose.ci.yml down -v --remove-orphans
	docker system prune -f
	@echo "Cleanup complete!"

# Validate the setup
validate:
	@echo "Validating QuicCrunch examples setup..."
	@./scripts/validate-setup.sh

# Quick development cycle
dev: build up
	@echo "Development environment ready!"
	@echo "Services are running and ready for development."

# Full CI test cycle
ci: build test-unit test-integration test-performance
	@echo "Full CI test cycle completed!"

# Code quality checks
quality:
	@echo "Running code quality checks..."
	docker-compose -f docker-compose.ci.yml --profile quality up --abort-on-container-exit code-quality