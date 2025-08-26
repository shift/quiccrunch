# Performance Tuning Guide

This guide provides comprehensive recommendations for optimizing QuicCrunch performance in various deployment scenarios.

## Quick Start Optimizations

### Basic Configuration

```rust
use quiccrunch::TransportConfig;

let config = TransportConfig {
    // Increase concurrent streams for higher throughput
    max_concurrent_streams: 2000,
    // Reduce timeout for faster error detection
    connect_timeout: Duration::from_secs(10),
    // Optimize for your network conditions
    initial_rtt: Duration::from_millis(50),
    // Adjust based on your message sizes
    max_message_size: 16 * 1024 * 1024,
    // Enable performance optimizations
    optimize_for_throughput: true,
    enable_zero_copy: true,
};
```

## System-Level Tuning

### CPU Optimization

1. Frequency Scaling
   ```bash
   # Set CPU governor to performance mode
   sudo cpupower frequency-set -g performance
   
   # Verify settings
   cpupower frequency-info
   
   # Monitor CPU frequencies
   watch -n1 "cat /proc/cpuinfo | grep MHz"
   ```

2. CPU Affinity
   ```bash
   # Pin process to specific CPUs
   taskset -c 0-4 ./your_application
   
   # Verify CPU affinity
   ps -o pid,psr,comm -p $(pgrep your_application)
   
   # Monitor CPU usage per core
   mpstat -P ALL 1
   ```

3. NUMA Optimization
   ```bash
   # Run with local memory allocation
   numactl --localalloc ./your_application
   
   # Monitor NUMA stats
   numastat -p $(pgrep your_application)
   
   # View NUMA topology
   numactl --hardware
   ```

### Network Optimization

1. Socket Buffers
   ```bash
   # Increase socket buffers
   sudo sysctl -w net.core.rmem_max=26214400
   sudo sysctl -w net.core.wmem_max=26214400
   
   # Verify settings
   sysctl net.core.rmem_max
   sysctl net.core.wmem_max
   ```

2. Network Stack
   ```bash
   # Optimize TCP settings
   sudo sysctl -w net.ipv4.tcp_timestamps=0
   sudo sysctl -w net.ipv4.tcp_sack=1
   sudo sysctl -w net.ipv4.tcp_window_scaling=1
   sudo sysctl -w net.ipv4.tcp_low_latency=1
   
   # Monitor network stats
   netstat -s | grep -i retransmit
   ```

3. Network Interface
   ```bash
   # Optimize NIC settings
   sudo ethtool -G eth0 rx 4096 tx 4096
   sudo ethtool -K eth0 tso on gso on gro on
   
   # Monitor NIC stats
   ethtool -S eth0 | grep -iE "err|drop|fifo|collision"
   ```

## Application-Level Optimization

### Connection Management

1. Connection Pooling
   ```rust
   use quiccrunch::pool::{Pool, PoolConfig};

   let pool = Pool::new(PoolConfig {
       min_connections: 5,
       max_connections: 20,
       max_idle_time: Duration::from_secs(300),
       health_check_interval: Duration::from_secs(60),
   });
   
   // Monitor pool metrics
   println!("Active connections: {}", pool.active_connections());
   println!("Idle connections: {}", pool.idle_connections());
   ```

2. Stream Multiplexing
   ```rust
   // Use multiple streams efficiently
   let mut streams = Vec::new();
   
   // Pre-allocate streams
   for _ in 0..10 {
       streams.push(connection.open_bi().await?);
   }
   
   // Parallel processing
   let results = futures::future::join_all(
       streams.iter_mut().map(|stream| async {
           stream.send_message(message.clone()).await
       })
   ).await;
   ```

3. Connection Settings
   ```rust
   let config = TransportConfig {
       // Keep-alive configuration
       keep_alive_interval: Duration::from_secs(30),
       idle_timeout: Duration::from_secs(60),
       
       // Performance settings
       receive_window: 16_777_216,  // 16MB
       send_window: 16_777_216,     // 16MB
       max_stream_window: 1_048_576, // 1MB
   };
   ```

### Message Optimization

1. Message Batching
   ```rust
   // Efficient message batching
   let mut batch = MessageBatch::with_capacity(100);
   
   // Add messages with size tracking
   let mut batch_size = 0;
   for msg in messages {
       if batch_size + msg.len() > MAX_BATCH_SIZE {
           stream.send_batch(batch).await?;
           batch = MessageBatch::with_capacity(100);
           batch_size = 0;
       }
       batch.add(msg);
       batch_size += msg.len();
   }
   
   // Send remaining messages
   if !batch.is_empty() {
       stream.send_batch(batch).await?;
   }
   ```

2. Zero-Copy Messaging
   ```rust
   // Use zero-copy for large messages
   let message = LargeMessage::new_zero_copy(data);
   
   // Send with fallback
   if message.len() >= ZERO_COPY_THRESHOLD {
       stream.send_zero_copy(message).await?;
   } else {
       stream.send_message(message).await?;
   }
   ```

3. Compression
   ```rust
   // Smart compression based on message type
   let config = TransportConfig {
       enable_compression: true,
       compression_threshold: 1024, // bytes
       compression_level: 6,        // 0-9
       
       // Selective compression
       compress_text: true,
       compress_binary: false,
   };
   ```

## Protocol Tuning

### Flow Control

1. Window Management
   ```rust
   let config = TransportConfig {
       // Initial conservative values
       initial_window_size: 65535,
       max_window_size: 16777216,
       
       // Dynamic scaling
       enable_dynamic_window: true,
       window_update_threshold: 0.5,  // Update at 50% consumed
   };
   ```

2. Stream Control
   ```rust
   let config = TransportConfig {
       // Stream limits
       max_concurrent_streams: 2000,
       max_stream_data: 16777216,
       
       // Stream priorities
       enable_stream_priorities: true,
       default_priority: 100,
   };
   ```

### TLS Configuration

1. Session Management
   ```rust
   let tls_config = TlsConfig {
       // Session resumption
       enable_session_resumption: true,
       session_ticket_lifetime: Duration::from_secs(3600),
       
       // Connection reuse
       enable_0rtt: true,
       max_early_data: 16384,
   };
   ```

2. Security Settings
   ```rust
   let tls_config = TlsConfig {
       // Modern cipher suites
       cipher_suites: vec![
           CipherSuite::TLS13_AES_256_GCM_SHA384,
           CipherSuite::TLS13_AES_128_GCM_SHA256,
           CipherSuite::TLS13_CHACHA20_POLY1305_SHA256,
       ],
       
       // Key exchange groups
       groups: vec![
           Group::X25519,
           Group::SECP384R1,
       ],
   };
   ```

## Performance Monitoring

### Real-time Metrics

1. Enable Telemetry
   ```rust
   use quiccrunch::telemetry::{TelemetryConfig, Metrics};

   let telemetry = Telemetry::new(TelemetryConfig {
       // Core metrics
       collect_latency: true,
       collect_throughput: true,
       collect_resource_usage: true,
       
       // Advanced metrics
       collect_queue_stats: true,
       collect_memory_stats: true,
       collect_stream_stats: true,
   });
   ```

2. Custom Metrics
   ```rust
   // Record custom metrics
   metrics.record_value("message_size", message.len() as f64);
   metrics.increment_counter("messages_sent");
   
   // Record histograms
   metrics.record_histogram("processing_time", duration);
   
   // Export metrics
   let snapshot = metrics.snapshot();
   println!("Metrics: {:#?}", snapshot);
   ```

### Performance Analysis

1. Latency Profiling
   ```rust
   use quiccrunch::profiling::LatencyProfiler;

   let profiler = LatencyProfiler::new();
   
   // Detailed operation timing
   profiler.start_operation("message_processing");
   // Process message
   profiler.end_operation("message_processing");
   
   // Generate report
   let report = profiler.generate_report();
   println!("Latency Report:\n{}", report);
   ```

2. Resource Monitoring
   ```rust
   use quiccrunch::monitoring::{ResourceMonitor, ResourceMetrics};

   let monitor = ResourceMonitor::new();
   
   // Start monitoring
   monitor.start();
   
   // Get current metrics
   let metrics = monitor.get_metrics();
   println!("CPU Usage: {}%", metrics.cpu_usage);
   println!("Memory Usage: {} MB", metrics.memory_usage / 1024 / 1024);
   println!("Network I/O: {} MB/s", metrics.network_throughput / 1024 / 1024);
   ```

## Troubleshooting Guide

### Common Issues

1. High Latency
   - Symptoms:
     * Increased response times
     * High latency variance
     * Queue buildup
   - Solutions:
     * Check network conditions (ping, traceroute)
     * Monitor system load (top, htop)
     * Verify CPU governor settings
     * Check for resource contention
     * Monitor buffer sizes

2. Low Throughput
   - Symptoms:
     * Reduced message rate
     * Increased queuing
     * Poor bandwidth utilization
   - Solutions:
     * Verify window sizes
     * Check stream limits
     * Monitor bandwidth usage
     * Optimize message batching
     * Check compression settings

3. Resource Usage
   - Symptoms:
     * High CPU usage
     * Memory growth
     * Excessive system calls
   - Solutions:
     * Profile memory allocation
     * Monitor garbage collection
     * Check buffer sizes
     * Optimize system calls
     * Use async I/O

### Performance Testing

1. Baseline Testing
   ```bash
   # Run basic performance test
   cargo bench --bench transport
   
   # Monitor system during test
   vmstat 1
   iostat -x 1
   ```

2. Advanced Profiling
   ```bash
   # CPU profiling
   perf record -F 99 -g ./your_application
   perf report
   
   # Memory analysis
   valgrind --tool=massif ./your_application
   ms_print massif.out.*
   
   # Async profiling
   async-profiler -d 30 -f profile.html $(pgrep your_application)
   ```

### Best Practices

1. Monitoring Checklist
   - Regular metrics collection
   - Resource usage tracking
   - Performance baselines
   - Alert thresholds

2. Optimization Process
   - Measure before optimizing
   - Test single changes
   - Verify improvements
   - Document changes

3. Maintenance Tasks
   - Regular performance tests
   - System health checks
   - Configuration reviews
   - Metric analysis