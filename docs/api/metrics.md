# Metrics API Reference

The metrics module provides instrumentation and monitoring capabilities for QuicCrunch.

## Metrics

Core metrics collection and reporting functionality.

### Methods

#### `fn new() -> Self`

Creates a new metrics collector.

Returns:
- `Metrics`: New metrics instance

#### `fn record_operation(&mut self, op: Operation, duration: Duration)`

Records timing for an operation.

Parameters:
- `op`: Operation type
- `duration`: Operation duration

Example:
```rust
metrics.record_operation(Operation::Connect, duration);
```

#### `fn get_operation_stats(&self, op: Operation) -> Option<Stats>`

Gets statistics for an operation type.

Parameters:
- `op`: Operation type

Returns:
- `Option<Stats>`: Statistics if available

### Operation Types

```rust
pub enum Operation {
    Connect,
    Listen,
    Accept,
    OpenStream,
    SendMessage,
    ReceiveMessage,
}
```

### Statistics

```rust
pub struct Stats {
    pub count: u64,
    pub total_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub avg_duration: Duration,
}
```

## Usage Examples

### Basic Metrics Collection
```rust
let mut metrics = Metrics::new();

// Record operation timing
let start = Instant::now();
connection.open_bi().await?;
metrics.record_operation(Operation::OpenStream, start.elapsed());

// Get statistics
if let Some(stats) = metrics.get_operation_stats(Operation::OpenStream) {
    println!("Stream operations: {}", stats.count);
    println!("Average duration: {:?}", stats.avg_duration);
}
```

### Performance Monitoring
```rust
let mut metrics = Metrics::new();

// Monitor message operations
async fn send_with_metrics(
    stream: &mut QuicStream,
    message: bytes::Bytes,
    metrics: &mut Metrics
) -> Result<()> {
    let start = Instant::now();
    let result = stream.send_message(message).await;
    metrics.record_operation(Operation::SendMessage, start.elapsed());
    result
}
```

### Statistics Reporting
```rust
fn print_metrics(metrics: &Metrics) {
    for op in [
        Operation::Connect,
        Operation::SendMessage,
        Operation::ReceiveMessage
    ] {
        if let Some(stats) = metrics.get_operation_stats(op) {
            println!("{:?} operations:", op);
            println!("  Count: {}", stats.count);
            println!("  Average: {:?}", stats.avg_duration);
            println!("  Min: {:?}", stats.min_duration);
            println!("  Max: {:?}", stats.max_duration);
        }
    }
}
```

## Best Practices

1. **Collection Strategy**
   - Collect metrics consistently
   - Use appropriate operation types
   - Consider performance impact
   - Implement sampling if needed

2. **Monitoring**
   - Monitor key operations
   - Track timing distributions
   - Set up alerts on thresholds
   - Log unusual patterns

3. **Analysis**
   - Review metrics regularly
   - Look for performance trends
   - Compare across operations
   - Use metrics for optimization

4. **Resource Usage**
   - Monitor memory usage
   - Track operation counts
   - Watch for resource leaks
   - Clean up old metrics

## Integration Examples

### With Logging
```rust
use tracing::{info, warn};

fn log_metrics(metrics: &Metrics) {
    for op in Operation::iter() {
        if let Some(stats) = metrics.get_operation_stats(op) {
            info!(
                operation = ?op,
                count = stats.count,
                avg_duration = ?stats.avg_duration,
                "Operation statistics"
            );

            if stats.max_duration > Duration::from_secs(1) {
                warn!(
                    operation = ?op,
                    max_duration = ?stats.max_duration,
                    "Operation exceeded 1s threshold"
                );
            }
        }
    }
}
```

### With Prometheus
```rust
use prometheus::{Registry, Histogram};

struct PrometheusMetrics {
    registry: Registry,
    operation_duration: Histogram,
}

impl PrometheusMetrics {
    fn new() -> Self {
        let registry = Registry::new();
        let operation_duration = Histogram::new(
            "operation_duration_seconds",
            "Operation duration in seconds",
            vec!["operation"]
        )?;
        
        registry.register(Box::new(operation_duration.clone()))?;
        
        Self {
            registry,
            operation_duration,
        }
    }

    fn record(&self, op: Operation, duration: Duration) {
        self.operation_duration
            .with_label_values(&[op.as_str()])
            .observe(duration.as_secs_f64());
    }
}
```

### Periodic Reporting
```rust
async fn metrics_reporter(metrics: Arc<Mutex<Metrics>>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        let metrics = metrics.lock().await;
        
        // Report all operation stats
        for op in Operation::iter() {
            if let Some(stats) = metrics.get_operation_stats(op) {
                info!(
                    "Operation {:?}: count={}, avg={:?}",
                    op,
                    stats.count,
                    stats.avg_duration
                );
            }
        }
    }
}
```