# Power Monitoring API Reference

The power module provides capabilities for monitoring and analyzing power consumption and system performance characteristics.

## PowerMonitor

The main interface for power monitoring and measurement.

### Creation

```rust
use quiccrunch::PowerMonitor;

// Create a new power monitor instance
let monitor = PowerMonitor::new()?;
```

### Methods

#### `fn new() -> io::Result<Self>`

Creates a new PowerMonitor instance.

Returns:
- `io::Result<PowerMonitor>`: New monitor instance or error

#### `fn get_package_power(&mut self) -> io::Result<f64>`

Gets CPU package power consumption.

Returns:
- `io::Result<f64>`: Power consumption in watts

#### `fn get_dram_power(&mut self) -> io::Result<f64>`

Gets DRAM power consumption.

Returns:
- `io::Result<f64>`: Power consumption in watts

#### `fn get_cpu_frequency(&self) -> io::Result<f64>`

Gets current CPU frequency.

Returns:
- `io::Result<f64>`: Frequency in MHz

#### `fn get_cpu_temperature(&self) -> io::Result<f64>`

Gets CPU temperature.

Returns:
- `io::Result<f64>`: Temperature in Celsius

#### `fn get_cpu_governor(&self) -> io::Result<String>`

Gets current CPU frequency governor mode.

Returns:
- `io::Result<String>`: Governor mode (e.g., "performance", "powersave")

#### `fn update_metrics(&mut self) -> Result<(), PowerError>`

Updates all power-related metrics.

Returns:
- `Result<(), PowerError>`: Success or error

### Metrics

The PowerMonitor records the following metrics:

1. Power Metrics
   - `cpu.package.power`: CPU package power consumption (watts)
   - `memory.power`: DRAM power consumption (watts)
   - `power.efficiency`: Power efficiency (watts per MHz)
   - `power.samples`: Number of power measurements taken

2. CPU Metrics
   - `cpu.frequency`: Current CPU frequency (MHz)
   - `cpu.temperature`: CPU temperature (Celsius)
   - `cpu.governor`: Current frequency governor mode

### Error Handling

```rust
pub enum PowerError {
    IoError(io::Error),
    MetricError(MetricError),
    InvalidValue(String),
}
```

## Usage Examples

### Basic Power Monitoring

```rust
use quiccrunch::PowerMonitor;
use std::time::Duration;

let mut monitor = PowerMonitor::new()?;

// Get current power readings
let package_power = monitor.get_package_power()?;
let dram_power = monitor.get_dram_power()?;
println!("CPU Power: {:.2}W, DRAM Power: {:.2}W", package_power, dram_power);

// Get CPU metrics
let frequency = monitor.get_cpu_frequency()?;
let temperature = monitor.get_cpu_temperature()?;
println!("CPU Freq: {:.0}MHz, Temp: {:.1}°C", frequency, temperature);
```

### Continuous Monitoring

```rust
use tokio::time;

async fn monitor_system() -> Result<(), PowerError> {
    let mut monitor = PowerMonitor::new()?;
    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;
        monitor.update_metrics()?;

        let package_power = monitor.get_package_power()?;
        let dram_power = monitor.get_dram_power()?;
        let efficiency = if let Ok(freq) = monitor.get_cpu_frequency() {
            (package_power + dram_power) / freq
        } else {
            0.0
        };

        println!("Power Efficiency: {:.3} W/MHz", efficiency);
    }
}
```

### Power Profiling

```rust
use std::time::Instant;

async fn profile_operation<F, T>(monitor: &mut PowerMonitor, operation: F) -> T 
where
    F: FnOnce() -> T
{
    // Record initial state
    let start_power = monitor.get_package_power()?;
    let start_time = Instant::now();

    // Run operation
    let result = operation();

    // Record final state
    let end_power = monitor.get_package_power()?;
    let duration = start_time.elapsed();
    let energy = (end_power + monitor.get_dram_power()?) * duration.as_secs_f64();

    println!("Operation Energy: {:.2}J", energy);
    monitor.update_metrics()?;

    result
}
```

## Best Practices

1. **Sampling Strategy**
   - Sample at appropriate intervals (1-5 seconds recommended)
   - Consider workload characteristics
   - Balance accuracy vs overhead
   - Use consistent sampling periods

2. **Resource Impact**
   - Monitor overhead is minimal but non-zero
   - Batch metric updates when possible
   - Consider sampling rate vs accuracy tradeoffs
   - Clean up resources properly

3. **Integration**
   - Combine with performance metrics
   - Record environmental conditions
   - Track system configuration
   - Correlate with workload metrics

4. **Analysis**
   - Consider moving averages for stability
   - Account for measurement uncertainty
   - Compare against baseline measurements
   - Look for efficiency trends

## Common Patterns

### Power-Aware Operation

```rust
use quiccrunch::{PowerMonitor, TransportConfig};

async fn configure_power_aware_transport() -> Result<Transport, Error> {
    let mut monitor = PowerMonitor::new()?;
    let mut config = TransportConfig::default();

    // Adjust based on power state
    if let Ok(governor) = monitor.get_cpu_governor() {
        match governor.as_str() {
            "powersave" => {
                config.max_concurrent_streams(100)
                      .keep_alive_interval(Duration::from_secs(10));
            }
            "performance" => {
                config.max_concurrent_streams(1000)
                      .keep_alive_interval(Duration::from_secs(5));
            }
            _ => {}
        }
    }

    Transport::with_config(config)
}
```

### Efficiency Monitoring

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

async fn monitor_efficiency(transport: Arc<Transport>) {
    let monitor = Arc::new(Mutex::new(PowerMonitor::new()?));
    let mut interval = time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;
        let mut monitor = monitor.lock().await;
        monitor.update_metrics()?;

        // Calculate messages per watt
        let package_power = monitor.get_package_power()?;
        let dram_power = monitor.get_dram_power()?;
        let total_power = package_power + dram_power;

        if let Ok(metrics) = transport.get_metrics() {
            let msg_rate = metrics.message_rate();
            println!(
                "Efficiency: {:.2} msgs/watt", 
                msg_rate / total_power
            );
        }
    }
}
```