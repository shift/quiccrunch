#!/bin/bash

# Benchmark Framework Setup Script
# This script sets up the benchmark framework and its dependencies

set -e

echo "Setting up Benchmark Framework..."

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Cargo.toml not found. Please run this script from the framework directory."
    exit 1
fi

# Install Rust dependencies
echo "Installing Rust dependencies..."
cargo build --release

# Check system dependencies
echo "Checking system dependencies..."

# Check for required commands
required_commands=("cargo" "rustc" "ping" "ps" "netstat")
missing_commands=()

for cmd in "${required_commands[@]}"; do
    if ! command -v "$cmd" &> /dev/null; then
        missing_commands+=("$cmd")
    fi
done

if [ ${#missing_commands[@]} -ne 0 ]; then
    echo "Warning: Missing required commands: ${missing_commands[*]}"
    echo "Please install these commands before running benchmarks."
fi

# Create results directory
echo "Creating results directory..."
mkdir -p results
mkdir -p reports
mkdir -p logs

# Generate default configuration
echo "Generating default configuration..."
if [ ! -f "benchmark_config.yaml" ]; then
    ./target/release/benchmark_runner init --config benchmark_config.yaml --default
    echo "Default configuration created: benchmark_config.yaml"
fi

# Validate environment
echo "Validating environment..."
./target/release/benchmark_runner validate --system --dependencies

# Set up logging
echo "Setting up logging configuration..."
cat > logging.yaml << EOF
version: 1
disable_existing_loggers: false
formatters:
  default:
    format: "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
handlers:
  console:
    class: logging.StreamHandler
    level: INFO
    formatter: default
    stream: ext://sys.stdout
  file:
    class: logging.FileHandler
    level: DEBUG
    formatter: default
    filename: logs/benchmark.log
    mode: a
loggers:
  benchmark_framework:
    level: DEBUG
    handlers: [console, file]
    propagate: false
root:
  level: INFO
  handlers: [console]
EOF

# Create example scripts
echo "Creating example scripts..."

# Example run script
cat > run_benchmark.sh << 'EOF'
#!/bin/bash

# Example benchmark execution script

set -e

echo "Starting benchmark execution..."

# Clean up any existing processes
./target/release/benchmark_runner cleanup --processes --temp

# Validate environment
./target/release/benchmark_runner validate --system --dependencies

# Run benchmarks
./target/release/benchmark_runner run \
    --config benchmark_config.yaml \
    --parallel 2 \
    --log-level info

# Generate reports
./target/release/benchmark_runner report \
    --input ./results \
    --output ./reports \
    --format html

echo "Benchmark completed. Check ./reports for results."
EOF

chmod +x run_benchmark.sh

# Example quick test script
cat > quick_test.sh << 'EOF'
#!/bin/bash

# Quick test script for basic functionality

set -e

echo "Running quick benchmark test..."

# Run with minimal configuration
./target/release/benchmark_runner run \
    --implementations "gRPC" \
    --scenarios "light_load" \
    --parallel 1 \
    --log-level debug

echo "Quick test completed."
EOF

chmod +x quick_test.sh

# Example cleanup script
cat > cleanup.sh << 'EOF'
#!/bin/bash

# Cleanup script for benchmark artifacts

echo "Cleaning up benchmark artifacts..."

./target/release/benchmark_runner cleanup --results --temp --processes

# Clean up log files older than 7 days
find logs/ -name "*.log" -mtime +7 -delete 2>/dev/null || true

echo "Cleanup completed."
EOF

chmod +x cleanup.sh

# Create performance monitoring script
cat > monitor_performance.sh << 'EOF'
#!/bin/bash

# Performance monitoring script during benchmarks

LOG_FILE="logs/performance_monitor.log"
INTERVAL=1  # seconds

echo "Starting performance monitoring..."
echo "Timestamp,CPU%,Memory%,Load1,Load5,Load15" > "$LOG_FILE"

while true; do
    TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
    CPU=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | sed 's/%us,//')
    MEMORY=$(free | grep Mem | awk '{printf "%.1f", $3/$2 * 100.0}')
    LOAD=$(uptime | awk -F'load average:' '{print $2}' | awk '{print $1, $2, $3}' | tr ' ' ',')
    
    echo "$TIMESTAMP,$CPU,$MEMORY,$LOAD" >> "$LOG_FILE"
    sleep $INTERVAL
done
EOF

chmod +x monitor_performance.sh

# Create network configuration helper
cat > setup_network.sh << 'EOF'
#!/bin/bash

# Network configuration helper for benchmarking

# Function to check if running as root
check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo "This script requires root privileges for network configuration."
        echo "Please run with sudo or modify as needed for your system."
        exit 1
    }
}

# Function to configure network limits
configure_limits() {
    echo "Configuring network limits..."
    
    # Increase file descriptor limits
    echo "* soft nofile 65536" >> /etc/security/limits.conf
    echo "* hard nofile 65536" >> /etc/security/limits.conf
    
    # Configure network buffers
    sysctl -w net.core.rmem_max=134217728
    sysctl -w net.core.wmem_max=134217728
    sysctl -w net.ipv4.tcp_rmem="4096 87380 134217728"
    sysctl -w net.ipv4.tcp_wmem="4096 65536 134217728"
    
    echo "Network limits configured."
}

# Function to reset network configuration
reset_limits() {
    echo "Resetting network limits to defaults..."
    
    sysctl -w net.core.rmem_max=212992
    sysctl -w net.core.wmem_max=212992
    sysctl -w net.ipv4.tcp_rmem="4096 87380 6291456"
    sysctl -w net.ipv4.tcp_wmem="4096 65536 6291456"
    
    echo "Network limits reset to defaults."
}

# Main script logic
case "$1" in
    "configure")
        check_root
        configure_limits
        ;;
    "reset")
        check_root
        reset_limits
        ;;
    "check")
        echo "Current network configuration:"
        sysctl net.core.rmem_max net.core.wmem_max net.ipv4.tcp_rmem net.ipv4.tcp_wmem
        ;;
    *)
        echo "Usage: $0 {configure|reset|check}"
        echo "  configure - Set optimized network limits for benchmarking"
        echo "  reset     - Reset network limits to defaults"
        echo "  check     - Show current network configuration"
        exit 1
        ;;
esac
EOF

chmod +x setup_network.sh

# Create Docker configuration for isolated testing
cat > Dockerfile << 'EOF'
FROM rust:1.70-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    iputils-ping \
    net-tools \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy framework files
COPY . .

# Build the framework
RUN cargo build --release

# Create directories
RUN mkdir -p results reports logs

# Set environment variables
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1

# Expose ports for testing
EXPOSE 50051 8080 9000-9100

# Default command
CMD ["./target/release/benchmark_runner", "--help"]
EOF

# Create docker-compose.yml for multi-service testing
cat > docker-compose.yml << 'EOF'
version: '3.8'

services:
  benchmark-framework:
    build: .
    container_name: benchmark-framework
    volumes:
      - ./results:/app/results
      - ./reports:/app/reports
      - ./logs:/app/logs
      - ./benchmark_config.yaml:/app/benchmark_config.yaml
    environment:
      - RUST_LOG=info
    networks:
      - benchmark-network
    depends_on:
      - grpc-server
      - quiccrunch-server

  grpc-server:
    image: grpc-server:latest
    container_name: grpc-server
    ports:
      - "50051:50051"
    networks:
      - benchmark-network
    environment:
      - SERVER_HOST=0.0.0.0
      - SERVER_PORT=50051

  quiccrunch-server:
    image: quiccrunch-server:latest
    container_name: quiccrunch-server
    ports:
      - "8080:8080"
    networks:
      - benchmark-network
    environment:
      - SERVER_HOST=0.0.0.0
      - SERVER_PORT=8080

networks:
  benchmark-network:
    driver: bridge
EOF

echo "Creating documentation index..."
cat > INDEX.md << 'EOF'
# Benchmark Framework Documentation

## Quick Start

1. **Setup**: Run `./setup.sh` to initialize the framework
2. **Configure**: Edit `benchmark_config.yaml` for your test scenarios
3. **Validate**: Run `./target/release/benchmark_runner validate`
4. **Execute**: Run `./run_benchmark.sh` to start benchmarks
5. **Analyze**: Check `./reports` for generated reports

## Scripts

- `run_benchmark.sh` - Complete benchmark execution
- `quick_test.sh` - Quick functionality test
- `cleanup.sh` - Clean up artifacts and processes
- `monitor_performance.sh` - Real-time performance monitoring
- `setup_network.sh` - Network configuration helper

## Configuration Files

- `benchmark_config.yaml` - Main benchmark configuration
- `logging.yaml` - Logging configuration
- `Dockerfile` - Containerized testing environment
- `docker-compose.yml` - Multi-service testing setup

## Directories

- `results/` - Raw benchmark results
- `reports/` - Generated reports and charts
- `logs/` - Application and monitoring logs

## Docker Usage

```bash
# Build the framework container
docker build -t benchmark-framework .

# Run with docker-compose
docker-compose up benchmark-framework

# Execute benchmarks in container
docker run -v $(pwd)/results:/app/results benchmark-framework \
  ./target/release/benchmark_runner run
```

## Network Configuration

For optimal performance, configure network settings:

```bash
# Configure optimized limits (requires sudo)
sudo ./setup_network.sh configure

# Check current configuration
./setup_network.sh check

# Reset to defaults
sudo ./setup_network.sh reset
```

## Performance Monitoring

Monitor system resources during benchmarks:

```bash
# Start monitoring in background
./monitor_performance.sh &

# Run benchmarks
./run_benchmark.sh

# Stop monitoring
pkill -f monitor_performance.sh
```

## Troubleshooting

1. **Port conflicts**: Ensure ports 50051 and 8080 are available
2. **Resource limits**: Check system resources with `./target/release/benchmark_runner validate`
3. **Permission issues**: Some operations may require elevated privileges
4. **Network issues**: Verify network connectivity and firewall settings

## Support

For issues and questions:
1. Check logs in `./logs/` directory
2. Run validation: `./target/release/benchmark_runner validate --system --dependencies`
3. Review configuration: `benchmark_config.yaml`
4. Check system resources: `./monitor_performance.sh`
EOF

echo ""
echo "Benchmark Framework setup completed!"
echo ""
echo "Next steps:"
echo "1. Review and edit benchmark_config.yaml"
echo "2. Run './target/release/benchmark_runner validate' to check environment"
echo "3. Execute './run_benchmark.sh' to run benchmarks"
echo "4. Check './reports' directory for results"
echo ""
echo "For detailed documentation, see INDEX.md"
echo ""
echo "Example commands:"
echo "  ./quick_test.sh                    # Quick functionality test"
echo "  ./run_benchmark.sh                  # Full benchmark suite"
echo "  ./target/release/benchmark_runner --help  # Show all options"
echo ""