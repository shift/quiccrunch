#!/usr/bin/env bash
set -euo pipefail

# Run all checks
echo "Running Nix checks..."

# Format check
echo "Checking formatting..."
nix flake check .#fmt

# Clippy check
echo "Running clippy..."
nix flake check .#clippy

# Unit tests
echo "Running unit tests..."
nix flake check .#unit-tests

# Integration tests
echo "Running integration tests..."
nix flake check .#integration-tests

# Property tests
echo "Running property tests..."
nix flake check .#property-tests

# Benchmarks
echo "Running benchmarks..."
nix flake check .#benchmarks

# Code coverage
echo "Generating coverage report..."
nix flake check .#coverage

echo "All checks completed successfully!"