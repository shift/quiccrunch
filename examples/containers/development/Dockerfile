# Development Environment for QuicCrunch Contributors
# This container provides all tools needed for QuicCrunch development

FROM rust:1.75-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    # Build tools
    cmake \
    clang \
    pkg-config \
    libssl-dev \
    git \
    # Development tools
    curl \
    wget \
    unzip \
    jq \
    # Networking tools for testing
    netcat-openbsd \
    tcpdump \
    wireshark-common \
    # Text processing
    ripgrep \
    fd-find \
    # Monitoring tools
    htop \
    iotop \
    # Documentation tools
    pandoc \
    # Container tools
    docker.io \
    docker-compose \
    && rm -rf /var/lib/apt/lists/*

# Install additional Rust tools
RUN cargo install \
    cargo-watch \
    cargo-edit \
    cargo-audit \
    cargo-deny \
    cargo-outdated \
    cargo-tree \
    cargo-expand \
    cargo-machete \
    flamegraph \
    tokio-console

# Install development utilities
RUN rustup component add rustfmt clippy rust-analyzer

# Create development user
RUN useradd -m -s /bin/bash dev && \
    usermod -aG docker dev

# Set up workspace
WORKDIR /workspace

# Switch to dev user
USER dev

# Set up shell
RUN echo 'alias ll="ls -la"' >> ~/.bashrc && \
    echo 'alias la="ls -la"' >> ~/.bashrc && \
    echo 'alias grep="rg"' >> ~/.bashrc && \
    echo 'alias find="fd"' >> ~/.bashrc && \
    echo 'export RUST_LOG=debug' >> ~/.bashrc && \
    echo 'export RUST_BACKTRACE=1' >> ~/.bashrc

# Create useful development scripts
RUN mkdir -p /home/dev/scripts

# Script to run all tests
RUN cat > /home/dev/scripts/test-all.sh << 'EOF'
#!/bin/bash
set -e
echo "Running unit tests..."
cargo test --workspace
echo "Running integration tests..."
cargo test --manifest-path tests/integration/Cargo.toml
echo "Running examples..."
cargo check --examples --workspace
echo "All tests passed!"
EOF

# Script to run benchmarks
RUN cat > /home/dev/scripts/bench-all.sh << 'EOF'
#!/bin/bash
set -e
echo "Running benchmarks..."
cargo bench --workspace
echo "Running performance comparison..."
cargo run --example performance_comparison
echo "Benchmarks complete!"
EOF

# Script to format and lint
RUN cat > /home/dev/scripts/lint.sh << 'EOF'
#!/bin/bash
set -e
echo "Formatting code..."
cargo fmt --all
echo "Running clippy..."
cargo clippy --workspace --all-targets -- -D warnings
echo "Checking documentation..."
cargo doc --workspace --no-deps
echo "Linting complete!"
EOF

RUN chmod +x /home/dev/scripts/*.sh

# Add scripts to PATH
ENV PATH="/home/dev/scripts:${PATH}"

# Set default working directory
WORKDIR /workspace

# Default command
CMD ["/bin/bash"]