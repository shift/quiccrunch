# Testing Infrastructure for QuicCrunch
# This container provides tools for running integration tests

FROM rust:1.75-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    clang \
    pkg-config \
    libssl-dev \
    git \
    curl \
    netcat-openbsd \
    jq \
    python3 \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Install Python testing tools
RUN pip3 install \
    pytest \
    requests \
    websockets \
    asyncio \
    aiohttp

# Install Rust testing tools
RUN cargo install \
    cargo-nextest \
    cargo-tarpaulin

# Create test user
RUN useradd -m -s /bin/bash tester

WORKDIR /workspace

# Switch to test user
USER tester

# Create test scripts
RUN mkdir -p /home/tester/scripts

# Integration test runner
RUN cat > /home/tester/scripts/run-integration-tests.sh << 'EOF'
#!/bin/bash
set -e

echo "Starting integration tests..."

# Wait for services to be ready
echo "Waiting for services..."
for service in $CALCULATOR_SERVER $CHAT_SERVER $ECHO_SERVER; do
    echo "Waiting for $service..."
    timeout 60 bash -c "until nc -z ${service/:/ }; do sleep 1; done"
    echo "$service is ready!"
done

# Run integration tests
echo "Running integration tests..."
cargo test --manifest-path tests/integration/Cargo.toml --verbose

echo "Integration tests complete!"
EOF

# Performance validation test
RUN cat > /home/tester/scripts/validate-performance.sh << 'EOF'
#!/bin/bash
set -e

echo "Validating service performance..."

# Test calculator service performance
echo "Testing calculator performance..."
time for i in {1..100}; do
    echo '{"operation":{"Add":[1.0,2.0]},"id":"test"}' | nc -u ${CALCULATOR_SERVER/:/ } >/dev/null
done

# Test echo service performance
echo "Testing echo performance..."
time for i in {1..100}; do
    echo "test message $i" | nc -u ${ECHO_SERVER/:/ } >/dev/null
done

echo "Performance validation complete!"
EOF

RUN chmod +x /home/tester/scripts/*.sh

# Add scripts to PATH
ENV PATH="/home/tester/scripts:${PATH}"

# Default command
CMD ["run-integration-tests.sh"]