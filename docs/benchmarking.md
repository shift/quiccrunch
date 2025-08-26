# Benchmark Methodology

This document describes the benchmark methodology used in QuicCrunch for measuring performance, scalability, and efficiency.

## Benchmark Categories

### Transport Benchmarks

1. Message Size Performance
   - Small messages (≤1KB)
   - Medium messages (1KB-64KB)
   - Large messages (>64KB)

2. Concurrency Testing
   - Single stream
   - Multiple streams (2-2000)
   - Connection pooling

3. Protocol Overhead
   - Header overhead
   - Encryption impact
   - Multiplexing efficiency

### System Scaling Benchmarks

1. CPU Scaling
   - Frequency scaling (800MHz to max turbo)
   - Core count scaling
   - Thread count impact

2. Power States
   - Performance governor
   - Powersave governor
   - Balanced governor

3. Memory Impact
   - Bandwidth vs latency
   - Cache effects
   - NUMA considerations

## Running Benchmarks

### Quick Start Guide

Run all benchmarks:
```bash
# Basic benchmark suite
cargo bench

# Single benchmark category
cargo bench --bench transport

# System scaling tests (requires root)
sudo ./scripts/run_scaling_benchmark.sh
```

### Advanced Usage

1. Run specific test patterns:
```bash
# Run only transport tests
cargo bench --bench transport "transport_"

# Run only scaling tests
cargo bench --bench scaling "scale_"

# Run specific message size tests
cargo bench --bench transport "message_size_1kb"
```

2. Customize benchmark parameters:
```bash
# Set custom message sizes
QUICCRUNCH_MSG_SIZES="128,512,1024,4096" cargo bench

# Set concurrency level
QUICCRUNCH_CONCURRENT_STREAMS="10,50,100" cargo bench
```

## Benchmark Environment

### Hardware Requirements

1. CPU Requirements
   - Modern x86_64 CPU (Recommended: 8+ cores)
   - Frequency scaling support
   - Performance counters
   - RAPL support for power measurement

2. Memory Requirements
   - Minimum 16GB RAM
   - Dual-channel configuration
   - ECC recommended

3. Network Requirements
   - Gigabit Ethernet minimum (10GbE recommended)
   - Low latency switch/router (<1ms latency)
   - Minimal network congestion

### Environment Setup

1. System Configuration
   ```bash
   # Disable CPU frequency scaling
   sudo cpupower frequency-set -g performance

   # Drop filesystem caches
   sudo sh -c "echo 3 > /proc/sys/vm/drop_caches"

   # Disable swap
   sudo swapoff -a

   # Set real-time scheduling priority
   sudo chrt -f 99 cargo bench
   ```

2. Network Configuration
   ```bash
   # Optimize network settings
   sudo sysctl -w net.core.rmem_max=26214400
   sudo sysctl -w net.core.wmem_max=26214400
   sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 16777216"
   sudo sysctl -w net.ipv4.tcp_wmem="4096 87380 16777216"
   ```

3. CPU Isolation
   ```bash
   # Add to kernel boot parameters (GRUB)
   GRUB_CMDLINE_LINUX_DEFAULT="isolcpus=2-4 nohz_full=2-4 rcu_nocbs=2-4"
   ```

## Metrics Collection

### Core Metrics

1. Latency Measurements
   - Average latency
   - P50, P95, P99 latencies
   - Maximum latency
   - Latency distribution

2. Throughput Metrics
   - Messages per second
   - Bytes per second
   - Connection setup rate

3. Resource Usage
   - CPU utilization per core
   - Memory consumption
   - Network bandwidth
   - System calls

### Advanced Metrics

1. Power Consumption
   - Package power (CPU)
   - DRAM power 
   - System power
   - Performance per watt
   - Power efficiency (W/MHz)
   - Energy per operation
   - Frequency scaling impact
   - Governor mode effects

2. Hardware Monitoring
   - Core temperatures
   - Memory bandwidth
   - Cache hit rates
   - TLB misses

3. Protocol Metrics
   - Header overhead ratio
   - Encryption/decryption time
   - Compression ratio
   - Stream multiplexing efficiency

## Implementation Guide

### Basic Benchmark

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use quiccrunch::Transport;

fn benchmark_transport(c: &mut Criterion) {
    let mut group = c.benchmark_group("transport");
    
    // Test different message sizes
    for size in [128, 1024, 16384].iter() {
        group.throughput(Throughput::Bytes(*size));
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            size,
            |b, &size| {
                b.iter(|| {
                    let transport = Transport::new();
                    transport.send_message(vec![0u8; size])
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, benchmark_transport);
criterion_main!(benches);
```

### Advanced Measurements

```rust
use quiccrunch::metrics::{PowerMetrics, LatencyMetrics};
use quiccrunch::PowerMonitor;

async fn measure_performance() -> PerformanceReport {
    let mut metrics = Metrics::new();
    let mut power_monitor = PowerMonitor::new()?;
    
    // Collect baseline
    metrics.start_collection();
    power_monitor.update_metrics()?;
    
    // Configure workload
    let transport = Transport::new();
    let message_sizes = vec![1024, 16384, 65536];
    let concurrent_streams = 10;
    
    // Track initial power state
    let initial_power = power_monitor.get_package_power()?;
    let initial_freq = power_monitor.get_cpu_frequency()?;
    let governor = power_monitor.get_cpu_governor()?;
    
    println!("Initial state:");
    println!("  Power: {:.2}W", initial_power);
    println!("  Frequency: {:.0}MHz", initial_freq);
    println!("  Governor: {}", governor);
    
    // Run workload
    for size in message_sizes {
        let mut streams = Vec::new();
        for _ in 0..concurrent_streams {
            streams.push(transport.open_stream().await?);
        }
        
        for stream in &mut streams {
            // Track per-operation power
            let start_power = power_monitor.get_package_power()?;
            let start = Instant::now();
            
            stream.send_message(vec![0u8; size]).await?;
            
            let duration = start.elapsed();
            let end_power = power_monitor.get_package_power()?;
            let energy = (end_power + power_monitor.get_dram_power()?) * duration.as_secs_f64();
            
            println!("Message size {}B:", size);
            println!("  Energy: {:.3}J", energy);
            println!("  Power: {:.2}W", end_power);
            println!("  Efficiency: {:.3}J/KB", energy / (size as f64 / 1024.0));
        }
        
        // Update metrics after each batch
        power_monitor.update_metrics()?;
    }
    
    // Collect final results
    metrics.end_collection();
    
    // Generate consolidated report
    let mut report = metrics.generate_report();
    report.power_metrics = Some(PowerMetrics {
        avg_package_power: power_monitor.get_package_power()?,
        avg_dram_power: power_monitor.get_dram_power()?,
        efficiency: report.throughput.unwrap_or(0.0) / 
                   (power_monitor.get_package_power()? + power_monitor.get_dram_power()?),
    });
    
    report
}
```

## Results Analysis

### Data Processing

1. Statistical Analysis
```rust
use stats_alloc::StatsAlloc;
use criterion::stats::*;

fn analyze_results(samples: &[f64]) -> Statistics {
    let stats = Stats::new(samples);
    
    println!("Summary Statistics:");
    println!("Mean: {:.2} ms", stats.mean());
    println!("Std Dev: {:.2} ms", stats.std_dev());
    println!("Median: {:.2} ms", stats.median());
    println!("95th percentile: {:.2} ms", stats.percentile(95.0));
    
    stats
}
```

2. Visualization
```rust
use plotters::prelude::*;

fn plot_latency_distribution(data: &[f64]) -> Result<(), Box<dyn Error>> {
    let root = BitMapBackend::new("latency_dist.png", (800, 600))
        .into_drawing_area();
    root.fill(&WHITE)?;
    
    let max_latency = data.iter().fold(0./0., |m, v| v.max(m));
    let min_latency = data.iter().fold(0./0., |m, v| v.min(m));
    
    let mut chart = ChartBuilder::on(&root)
        .caption("Latency Distribution", ("sans-serif", 30))
        .margin(10)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d(
            min_latency..max_latency,
            0f64..100f64,
        )?;
    
    chart.configure_mesh().draw()?;
    
    // Plot histogram
    let histogram = Histogram::new(data, 50);
    chart.draw_series(
        Histogram::new(histogram.into_iter(), 50)
            .style(RED.filled()),
    )?;
    
    Ok(())
}
```

### Result Storage

Results are saved in multiple formats:
1. Raw data (JSON)
   ```json
   {
     "timestamp": "2025-08-23T10:00:00Z",
     "config": {
       "message_size": 1024,
       "concurrent_streams": 10
     },
     "metrics": {
       "latency_ms": [1.2, 1.3, 1.1, ...],
       "throughput_mbps": 945.7,
       "cpu_usage_percent": 65.2
     }
   }
   ```

2. Summary statistics (CSV)
   ```csv
   timestamp,test_case,mean_latency_ms,p95_latency_ms,throughput_mbps
   2025-08-23T10:00:00Z,transport_1kb,1.2,1.8,945.7
   ```

3. Plots (PNG/SVG)
   - Latency distribution
   - Throughput over time
   - Resource usage trends
   - Power efficiency curves

4. HTML reports (criterion)
   - Interactive visualizations
   - Regression detection
   - Historical comparisons

## Troubleshooting Guide

### Common Issues

1. High Variance in Results
   - Check for background processes
   - Verify CPU isolation
   - Monitor system temperature
   - Check for network interference

2. Lower Than Expected Performance
   - Verify hardware frequency scaling
   - Check network configuration
   - Monitor system resources
   - Validate benchmark parameters

3. Benchmark Failures
   - Check available system resources
   - Verify user permissions
   - Validate configuration
   - Check error logs

### Best Practices

1. Pre-benchmark Checklist
   - Disable power management
   - Clear system caches
   - Monitor system temperature
   - Verify network conditions

2. Running Guidelines
   - Run benchmarks multiple times
   - Discard warmup iterations
   - Monitor system stability
   - Log environmental conditions

3. Results Validation
   - Check for outliers
   - Verify statistical significance
   - Compare with baseline
   - Document anomalies