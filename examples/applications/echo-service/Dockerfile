# Dockerfile for QuicCrunch Echo Service
# This demonstrates the simplest possible QuicCrunch application containerization

# Build stage
FROM rust:1.75-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    cmake \
    clang \
    pkg-config \
    libssl-dev \
    git \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files first for better caching
COPY ../../../Cargo.toml ../../../Cargo.lock ./
COPY ../../../.cargo/ ./.cargo/

# Copy QuicCrunch core libraries
COPY ../../../quiccrunch-core/ ./quiccrunch-core/
COPY ../../../quiccrunch-macros/ ./quiccrunch-macros/

# Copy our application
COPY . ./examples/applications/echo-service/

WORKDIR /app/examples/applications/echo-service

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    netcat-openbsd \
    && rm -rf /var/lib/apt/lists/*

# Create application user
RUN useradd -r -s /bin/false -d /app echoservice

WORKDIR /app

# Copy binaries from builder
COPY --from=builder /app/examples/applications/echo-service/target/release/echo-server /usr/local/bin/
COPY --from=builder /app/examples/applications/echo-service/target/release/echo-client /usr/local/bin/

# Create necessary directories
RUN mkdir -p /app/data && chown -R echoservice:echoservice /app

USER echoservice

# Expose the default QUIC port
EXPOSE 50051/udp

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
    CMD echo "health-check" | nc -u 127.0.0.1 50051 >/dev/null 2>&1 || exit 1

# Default to running the server
CMD ["echo-server", "--address", "0.0.0.0:50051", "--verbose"]