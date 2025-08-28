use quiccrunch_core::power::PowerMonitor;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔋 Testing Power Monitoring Framework");
    println!("=====================================");
    
    // Test power monitor creation
    let mut power_monitor = PowerMonitor::new()?;
    println!("✅ PowerMonitor created successfully");
    
    // Test initial sample
    let initial_sample = power_monitor.sample()?;
    println!("📊 Initial Power Sample:");
    println!("   CPU Power: {:.2}W", initial_sample.cpu_power);
    println!("   DRAM Power: {:.2}W", initial_sample.dram_power);
    println!("   Temperature: {:.1}°C", initial_sample.temperature);
    
    // Verify readings are reasonable (not overflowed)
    if initial_sample.cpu_power > 600.0 {
        println!("❌ CPU power reading seems too high (possible overflow)");
    } else {
        println!("✅ CPU power reading looks reasonable");
    }
    
    if initial_sample.dram_power > 100.0 {
        println!("❌ DRAM power reading seems too high (possible overflow)");
    } else {
        println!("✅ DRAM power reading looks reasonable");
    }
    
    if initial_sample.temperature < 20.0 || initial_sample.temperature > 100.0 {
        println!("❌ Temperature reading seems unusual: {:.1}°C", initial_sample.temperature);
    } else {
        println!("✅ Temperature reading looks reasonable");
    }
    
    // Test sampling over time
    println!("\n🕐 Testing sampling over 3 seconds...");
    sleep(Duration::from_secs(1)).await;
    
    let second_sample = power_monitor.sample()?;
    println!("📊 Second Sample:");
    println!("   CPU Power: {:.2}W", second_sample.cpu_power);
    println!("   DRAM Power: {:.2}W", second_sample.dram_power);
    println!("   Temperature: {:.1}°C", second_sample.temperature);
    
    sleep(Duration::from_secs(1)).await;
    
    let third_sample = power_monitor.sample()?;
    println!("📊 Third Sample:");
    println!("   CPU Power: {:.2}W", third_sample.cpu_power);
    println!("   DRAM Power: {:.2}W", third_sample.dram_power);
    println!("   Temperature: {:.1}°C", third_sample.temperature);
    
    // Verify values changed (indicating live readings)
    if (initial_sample.cpu_power - second_sample.cpu_power).abs() > 0.1 ||
       (second_sample.cpu_power - third_sample.cpu_power).abs() > 0.1 {
        println!("✅ Power readings are changing (live monitoring)");
    } else {
        println!("⚠️  Power readings are static (may be expected under low load)");
    }
    
    println!("\n🎉 Power monitoring framework test completed!");
    Ok(())
}