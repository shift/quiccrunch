# Examples

This directory contains examples demonstrating QuicCrunch usage:

## Basic Examples

1. **basic.rs** - Basic client/server communication
2. **cert_utils.rs** - Certificate management utilities
3. **configuration.rs** - Transport configuration
4. **replace_grpc.rs** - gRPC replacement example
5. **tracing.rs** - Tracing and instrumentation

## Power Monitoring Examples

1. **power_monitoring.rs** - Power and efficiency monitoring
2. **power_scaling.rs** - Power scaling and frequency management
3. **power_profiling.rs** - Application power profiling

### Running Examples

```bash
# Run basic example
cargo run --example basic

# Run power monitoring example
cargo run --example power_monitoring

# Run power scaling example
cargo run --example power_scaling
```

### Power Monitoring Usage

```rust
use quiccrunch::{transport::Transport, power::PowerMonitor};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize transport and monitor
    let transport = Transport::new();
    let mut monitor = PowerMonitor::new()?;
    
    // Configure measurement interval
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    
    // Start power monitoring
    loop {
        interval.tick().await;
        
        // Get power measurements
        let package_power = monitor.get_package_power()?;
        let dram_power = monitor.get_dram_power()?;
        let cpu_freq = monitor.get_cpu_frequency()?;
        let cpu_temp = monitor.get_cpu_temperature()?;
        
        println!("System Status:");
        println!("  CPU Power: {:.2}W", package_power);
        println!("  DRAM Power: {:.2}W", dram_power);
        println!("  CPU Frequency: {:.0}MHz", cpu_freq);
        println!("  CPU Temperature: {:.1}°C", cpu_temp);
        println!("  Power Efficiency: {:.3}W/MHz", 
                (package_power + dram_power) / cpu_freq);
        
        // Update metrics
        monitor.update_metrics()?;
    }
}
```

### Power Scaling Example

Demonstrates how to adapt application behavior based on power state:

```rust
use quiccrunch::{PowerMonitor, Transport, TransportConfig};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut monitor = PowerMonitor::new()?;
    
    // Configure based on power state
    let config = match monitor.get_cpu_governor()?.as_str() {
        "powersave" => {
            TransportConfig::default()
                .max_concurrent_streams(100)
                .keep_alive_interval(Duration::from_secs(10))
        }
        "performance" => {
            TransportConfig::default()
                .max_concurrent_streams(1000)
                .keep_alive_interval(Duration::from_secs(5))
        }
        _ => TransportConfig::default()
    };
    
    let transport = Transport::with_config(config)?;
    
    // Monitor power metrics
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        interval.tick().await;
        monitor.update_metrics()?;
        
        let efficiency = (monitor.get_package_power()? + monitor.get_dram_power()?) /
                        monitor.get_cpu_frequency()?;
                        
        println!("Power efficiency: {:.3}W/MHz", efficiency);
    }
}
```

### Power Profiling Example

Shows how to profile application power consumption:

```rust
use quiccrunch::{transport::Transport, power::PowerMonitor};
use std::time::Instant;

async fn profile_operation<F, T>(
    monitor: &mut PowerMonitor,
    name: &str,
    operation: F
) -> T 
where
    F: FnOnce() -> T
{
    // Record initial state
    let start_power = monitor.get_package_power()?;
    let start_time = Instant::now();
    
    // Run operation
    let result = operation();
    
    // Calculate energy usage
    let duration = start_time.elapsed();
    let end_power = monitor.get_package_power()?;
    let energy = (end_power + monitor.get_dram_power()?) * 
                 duration.as_secs_f64();
    
    println!("Operation: {}", name);
    println!("  Duration: {:?}", duration);
    println!("  Energy: {:.3}J", energy);
    println!("  Avg Power: {:.2}W", energy / duration.as_secs_f64());
    
    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut monitor = PowerMonitor::new()?;
    let transport = Transport::new();
    
    // Profile connection setup
    profile_operation(&mut monitor, "Connection Setup", || {
        transport.connect("127.0.0.1:50051")
    }).await?;
    
    // Profile stream operations
    let conn = transport.connect("127.0.0.1:50051").await?;
    profile_operation(&mut monitor, "Stream Open", || {
        conn.open_bi()
    }).await?;
    
    Ok(())
}
```