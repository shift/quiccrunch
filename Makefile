# QuicCrunch Optimized Build System
# Provides cached, feature-specific builds with intelligent dependency management

.PHONY: help build clean test benchmark cache-status
.DEFAULT_GOAL := help

# Configuration
SCRIPT_DIR := scripts
BUILD_SCRIPT := $(SCRIPT_DIR)/build_optimized.sh
NIX_CMD := nix develop -c --

# Feature sets (matching build script)
MINIMAL_FEATURES := backend-tquic
STANDARD_FEATURES := backend-tquic,memory-pools  
PERFORMANCE_FEATURES := backend-tquic,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations
FULL_FEATURES := backend-tquic,lock-free-channels,memory-pools,simd-optimizations,adaptive-optimizations,io-uring-advanced,telemetry
GRPC_FEATURES := backend-tquic,grpc-comparison

help: ## Show this help message
	@echo "QuicCrunch Build System - Optimized for Feature Caching"
	@echo ""
	@echo "Quick builds:"
	@echo "  make minimal     - Minimal build (backend-tquic only)"
	@echo "  make standard    - Standard build with memory pools"
	@echo "  make performance - Performance build with all optimizations"
	@echo "  make full        - Full build with all features"
	@echo "  make grpc        - Build with gRPC comparison features"
	@echo ""
	@echo "Development:"
	@echo "  make dev         - Fast development build"
	@echo "  make dev-clean   - Clean dev build and rebuild"
	@echo "  make release     - Optimized release build"
	@echo ""
	@echo "Testing:"
	@echo "  make test        - Run all tests"
	@echo "  make test-perf   - Run performance tests"
	@echo "  make benchmark   - Run benchmarks"
	@echo ""
	@echo "Cache management:"
	@echo "  make cache-status - Show build cache status"
	@echo "  make clean       - Clean all build artifacts"
	@echo "  make clean-cache - Clean only build cache"
	@echo ""
	@echo "Advanced:"
	@echo "  make build FEATURES=<features> PROFILE=<profile>"
	@echo "  Available profiles: dev, dev-fast, release, bench"

# Quick build targets
minimal: ## Build with minimal features (fastest)
	@$(BUILD_SCRIPT) minimal dev

standard: ## Build with standard features  
	@$(BUILD_SCRIPT) standard dev

performance: ## Build with all performance optimizations
	@$(BUILD_SCRIPT) performance dev

full: ## Build with all features
	@$(BUILD_SCRIPT) full dev

grpc: ## Build with gRPC comparison features
	@$(BUILD_SCRIPT) grpc-compare dev

# Development targets
dev: performance ## Fast development build (alias for performance)

dev-clean: ## Clean and rebuild development build
	@$(BUILD_SCRIPT) clean performance
	@$(BUILD_SCRIPT) performance dev

release: ## Optimized release build
	@$(BUILD_SCRIPT) performance release

# Testing targets
test: ## Run all tests with performance features
	@echo "Running tests with performance features..."
	@$(BUILD_SCRIPT) performance dev
	@$(NIX_CMD) cargo test --features="$(PERFORMANCE_FEATURES)"

test-perf: ## Run performance-specific tests
	@echo "Running performance tests..."
	@$(BUILD_SCRIPT) performance dev  
	@$(NIX_CMD) cargo test --features="$(PERFORMANCE_FEATURES)" --test performance_*

test-minimal: ## Run tests with minimal features
	@echo "Running tests with minimal features..."
	@$(BUILD_SCRIPT) minimal dev
	@$(NIX_CMD) cargo test --features="$(MINIMAL_FEATURES)"

benchmark: ## Run benchmarks
	@echo "Running benchmarks..."
	@$(BUILD_SCRIPT) performance bench
	@$(NIX_CMD) cargo bench --features="$(PERFORMANCE_FEATURES)"

# Cache management
cache-status: ## Show build cache status
	@$(BUILD_SCRIPT) status

clean: ## Clean all build artifacts and cache
	@$(BUILD_SCRIPT) clean
	@echo "All build artifacts cleaned"

clean-cache: ## Clean only build cache
	@rm -rf target/cache
	@echo "Build cache cleaned"

clean-minimal: ## Clean minimal feature build
	@$(BUILD_SCRIPT) clean minimal

clean-performance: ## Clean performance feature build  
	@$(BUILD_SCRIPT) clean performance

clean-full: ## Clean full feature build
	@$(BUILD_SCRIPT) clean full

# Advanced build target
build: ## Custom build: make build FEATURES=<set> PROFILE=<profile>
	@$(BUILD_SCRIPT) $(or $(FEATURES),performance) $(or $(PROFILE),dev)

# Validation targets
check: ## Check code without building
	@$(NIX_CMD) cargo check --features="$(PERFORMANCE_FEATURES)"

check-all: ## Check all feature combinations
	@echo "Checking minimal features..."
	@$(NIX_CMD) cargo check --features="$(MINIMAL_FEATURES)"
	@echo "Checking standard features..."
	@$(NIX_CMD) cargo check --features="$(STANDARD_FEATURES)"
	@echo "Checking performance features..."
	@$(NIX_CMD) cargo check --features="$(PERFORMANCE_FEATURES)"
	@echo "Checking full features..."
	@$(NIX_CMD) cargo check --features="$(FULL_FEATURES)"

# Format and lint
fmt: ## Format code
	@$(NIX_CMD) cargo fmt

lint: ## Run clippy
	@$(BUILD_SCRIPT) performance dev
	@$(NIX_CMD) cargo clippy --features="$(PERFORMANCE_FEATURES)" -- -D warnings

# Documentation
docs: ## Build documentation
	@$(BUILD_SCRIPT) full dev
	@$(NIX_CMD) cargo doc --features="$(FULL_FEATURES)" --no-deps --open

# Install tools
install-tools: ## Install additional development tools
	@echo "Installing development tools..."
	@$(NIX_CMD) cargo install cargo-watch
	@$(NIX_CMD) cargo install cargo-expand
	@echo "Tools installed"

# Watch mode for development
watch: ## Watch for changes and rebuild
	@$(NIX_CMD) cargo watch -x "build --features=$(PERFORMANCE_FEATURES)"

watch-test: ## Watch for changes and run tests
	@$(NIX_CMD) cargo watch -x "test --features=$(PERFORMANCE_FEATURES)"

# Show current configuration
config: ## Show current build configuration
	@echo "Build Configuration:"
	@echo "  Script: $(BUILD_SCRIPT)"
	@echo "  Performance features: $(PERFORMANCE_FEATURES)"
	@echo "  Standard features: $(STANDARD_FEATURES)"
	@echo "  Minimal features: $(MINIMAL_FEATURES)"
	@echo ""
	@$(BUILD_SCRIPT) status