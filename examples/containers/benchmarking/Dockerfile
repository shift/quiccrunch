# Benchmarking Infrastructure for QuicCrunch
# This container provides tools for performance testing

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
    gnuplot \
    time \
    procps \
    && rm -rf /var/lib/apt/lists/*

# Install Python analysis tools
RUN pip3 install \
    matplotlib \
    pandas \
    numpy \
    scipy \
    jupyter

# Install Rust benchmarking tools
RUN cargo install \
    cargo-criterion \
    flamegraph \
    cargo-bench

# Create benchmark user
RUN useradd -m -s /bin/bash benchmarker

WORKDIR /workspace

# Switch to benchmark user
USER benchmarker

# Create benchmark scripts
RUN mkdir -p /home/benchmarker/scripts /results

# Service load test script
RUN cat > /home/benchmarker/scripts/load-test.sh << 'EOF'
#!/bin/bash
set -e

DURATION=${BENCHMARK_DURATION:-60s}
CONNECTIONS=${BENCHMARK_CONNECTIONS:-100}

echo "Starting load test..."
echo "Duration: $DURATION"
echo "Connections: $CONNECTIONS"

# Test calculator service
echo "Load testing calculator service..."
(
  for ((i=1; i<=CONNECTIONS; i++)); do
    (
      timeout $DURATION bash -c '
        while true; do
          echo "{\"operation\":{\"Add\":[$(shuf -i 1-100 -n 1).0,$(shuf -i 1-100 -n 1).0]},\"id\":\"load-test\"}" | nc -u $CALCULATOR_SERVER
        done
      ' &
    )
  done
  wait
) > /results/calculator-load-test.log 2>&1

# Test chat service
echo "Load testing chat service..."
(
  for ((i=1; i<=CONNECTIONS; i++)); do
    (
      timeout $DURATION bash -c '
        echo "{\"Join\":{\"user\":\"load-test-$i\",\"room\":\"load-test\"}}" | nc -u $CHAT_SERVER
        while true; do
          echo "{\"SendMessage\":{\"user\":\"load-test-$i\",\"room\":\"load-test\",\"content\":\"Load test message\"}}" | nc -u $CHAT_SERVER
        done
      ' &
    )
  done
  wait
) > /results/chat-load-test.log 2>&1

echo "Load test complete! Results in /results/"
EOF

# Latency test script
RUN cat > /home/benchmarker/scripts/latency-test.sh << 'EOF'
#!/bin/bash
set -e

echo "Starting latency test..."

# Test calculator latency
echo "Testing calculator latency..."
for i in {1..100}; do
  start=$(date +%s%N)
  echo '{"operation":{"Add":[1.0,2.0]},"id":"latency-test"}' | nc -u $CALCULATOR_SERVER >/dev/null
  end=$(date +%s%N)
  latency=$((($end - $start) / 1000000))
  echo $latency >> /results/calculator-latency.dat
done

# Test echo latency
echo "Testing echo latency..."
for i in {1..100}; do
  start=$(date +%s%N)
  echo "latency test message" | nc -u $ECHO_SERVER >/dev/null
  end=$(date +%s%N)
  latency=$((($end - $start) / 1000000))
  echo $latency >> /results/echo-latency.dat
done

echo "Latency test complete! Results in /results/"
EOF

# Throughput test script
RUN cat > /home/benchmarker/scripts/throughput-test.sh << 'EOF'
#!/bin/bash
set -e

echo "Starting throughput test..."

# Test calculator throughput
echo "Testing calculator throughput..."
start=$(date +%s)
count=0
timeout 30s bash -c '
  while true; do
    echo "{\"operation\":{\"Add\":[1.0,2.0]},\"id\":\"throughput-test\"}" | nc -u $CALCULATOR_SERVER >/dev/null
    count=$((count + 1))
  done
' || true
end=$(date +%s)
duration=$((end - start))
throughput=$((count / duration))
echo "Calculator throughput: $throughput requests/second" | tee /results/calculator-throughput.txt

# Test echo throughput
echo "Testing echo throughput..."
start=$(date +%s)
count=0
timeout 30s bash -c '
  while true; do
    echo "throughput test message" | nc -u $ECHO_SERVER >/dev/null
    count=$((count + 1))
  done
' || true
end=$(date +%s)
duration=$((end - start))
throughput=$((count / duration))
echo "Echo throughput: $throughput requests/second" | tee /results/echo-throughput.txt

echo "Throughput test complete! Results in /results/"
EOF

# Generate plots script
RUN cat > /home/benchmarker/scripts/generate-plots.py << 'EOF'
#!/usr/bin/env python3
import matplotlib.pyplot as plt
import numpy as np
import os

def plot_latency(service):
    try:
        data = np.loadtxt(f'/results/{service}-latency.dat')
        
        plt.figure(figsize=(10, 6))
        plt.hist(data, bins=50, alpha=0.7, edgecolor='black')
        plt.title(f'{service.title()} Service Latency Distribution')
        plt.xlabel('Latency (ms)')
        plt.ylabel('Frequency')
        plt.grid(True, alpha=0.3)
        
        # Add statistics
        plt.axvline(np.mean(data), color='red', linestyle='--', label=f'Mean: {np.mean(data):.2f}ms')
        plt.axvline(np.percentile(data, 95), color='orange', linestyle='--', label=f'95th percentile: {np.percentile(data, 95):.2f}ms')
        plt.legend()
        
        plt.tight_layout()
        plt.savefig(f'/results/{service}-latency.png', dpi=300, bbox_inches='tight')
        plt.close()
        
        print(f"Generated {service} latency plot")
    except Exception as e:
        print(f"Failed to generate {service} latency plot: {e}")

def generate_summary():
    summary = []
    summary.append("# QuicCrunch Benchmark Results\n")
    
    # Add latency statistics
    for service in ['calculator', 'echo']:
        try:
            data = np.loadtxt(f'/results/{service}-latency.dat')
            summary.append(f"## {service.title()} Service Latency")
            summary.append(f"- Mean: {np.mean(data):.2f}ms")
            summary.append(f"- Median: {np.median(data):.2f}ms")
            summary.append(f"- 95th percentile: {np.percentile(data, 95):.2f}ms")
            summary.append(f"- 99th percentile: {np.percentile(data, 99):.2f}ms")
            summary.append("")
        except:
            pass
    
    # Add throughput results
    for service in ['calculator', 'echo']:
        try:
            with open(f'/results/{service}-throughput.txt', 'r') as f:
                summary.append(f"## {service.title()} Service Throughput")
                summary.append(f"- {f.read().strip()}")
                summary.append("")
        except:
            pass
    
    with open('/results/benchmark-summary.md', 'w') as f:
        f.write('\n'.join(summary))
    
    print("Generated benchmark summary")

if __name__ == "__main__":
    plot_latency('calculator')
    plot_latency('echo')
    generate_summary()
    print("Benchmark analysis complete!")
EOF

RUN chmod +x /home/benchmarker/scripts/*.sh /home/benchmarker/scripts/*.py

# Add scripts to PATH
ENV PATH="/home/benchmarker/scripts:${PATH}"

# Default command runs all benchmarks
CMD ["bash", "-c", "load-test.sh && latency-test.sh && throughput-test.sh && generate-plots.py"]