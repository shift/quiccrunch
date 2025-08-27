use quiccrunch::power::{PowerMonitor, PowerData};
use quiccrunch::server::QuicServer;
use quiccrunch::client::QuicClient;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use bytes::Bytes;

const TEST_DURATION: Duration = Duration::from_secs(60);
const MESSAGE_SIZE: usize = 1024;

// Detect which backend is actually compiled
fn get_backend_name() -> &'static str {
    #[cfg(feature = "backend-tquic")]
    {
        "tQUIC"
    }
    #[cfg(all(feature = "backend-quinn", not(feature = "backend-tquic")))]
    {
        "Quinn"
    }
    #[cfg(not(any(feature = "backend-tquic", feature = "backend-quinn")))]
    {
        "Unknown"
    }
}

#[derive(Debug)]
struct TestResults {
    backend: String,
    test_type: String,
    duration: Duration,
    messages_sent: u64,
    bytes_transferred: u64,
    power_samples: Vec<PowerData>,
    avg_cpu_power: f64,
    avg_dram_power: f64,
    avg_frequency: f64,
    avg_temperature: f64,
}

impl TestResults {
    fn new(backend: String, test_type: String) -> Self {
        Self {
            backend,
            test_type,
            duration: Duration::ZERO,
            messages_sent: 0,
            bytes_transferred: 0,
            power_samples: Vec::new(),
            avg_cpu_power: 0.0,
            avg_dram_power: 0.0,
            avg_frequency: 0.0,
            avg_temperature: 0.0,
        }
    }

    fn calculate_averages(&mut self) {
        if self.power_samples.is_empty() {
            return;
        }

        let count = self.power_samples.len() as f64;
        self.avg_cpu_power = self.power_samples.iter().map(|s| s.cpu_power).sum::<f64>() / count;
        self.avg_dram_power = self.power_samples.iter().map(|s| s.dram_power).sum::<f64>() / count;
        self.avg_frequency = self.power_samples.iter().map(|s| s.cpu_frequency).sum::<f64>() / count;
        self.avg_temperature = self.power_samples.iter().map(|s| s.cpu_temperature).sum::<f64>() / count;
    }

    fn print_summary(&self) {
        println!("\n🔋 {} {} Test Results", self.backend, self.test_type);
        println!("=========================================");
        println!("⏱️  Test Duration: {:.2}s", self.duration.as_secs_f64());
        println!("📊 Performance Metrics:");
        println!("   Messages Sent: {}", self.messages_sent);
        println!("   Bytes Transferred: {} bytes ({:.2} MB)", 
                 self.bytes_transferred, 
                 self.bytes_transferred as f64 / 1_000_000.0);
        println!("   Message Rate: {:.1} msg/s", 
                 self.messages_sent as f64 / self.duration.as_secs_f64());
        println!("   Throughput: {:.2} MB/s", 
                 (self.bytes_transferred as f64 / 1_000_000.0) / self.duration.as_secs_f64());
        println!("🔌 Power Consumption:");
        println!("   Avg CPU Package Power: {:.2} W", self.avg_cpu_power);
        println!("   Avg DRAM Power: {:.2} W", self.avg_dram_power);
        println!("   Total Avg Power: {:.2} W", self.avg_cpu_power + self.avg_dram_power);
        println!("🌡️  System Status:");
        println!("   Avg CPU Frequency: {:.0} MHz", self.avg_frequency);
        println!("   Avg CPU Temperature: {:.1}°C", self.avg_temperature);
        println!("⚡ Efficiency Metrics:");
        if self.avg_frequency > 0.0 {
            println!("   Power Efficiency: {:.3} W/MHz", 
                     (self.avg_cpu_power + self.avg_dram_power) / self.avg_frequency);
        }
        if self.messages_sent > 0 {
            let energy_per_msg = (self.avg_cpu_power + self.avg_dram_power) * self.duration.as_secs_f64() / self.messages_sent as f64;
            println!("   Energy per Message: {:.6} J/msg", energy_per_msg);
        }
        let throughput_mbps = (self.bytes_transferred as f64 / 1_000_000.0) / self.duration.as_secs_f64();
        if throughput_mbps > 0.0 {
            println!("   Power per Throughput: {:.3} W/(MB/s)", 
                     (self.avg_cpu_power + self.avg_dram_power) / throughput_mbps);
        }
        println!("=========================================\n");
    }
}

async fn run_latency_test(backend: &str, port: u16) -> Result<TestResults, Box<dyn std::error::Error>> {
    println!("📝 Running {} Latency Test (60s of small frequent messages, single stream)", backend);
    
    let mut results = TestResults::new(backend.to_string(), "Latency".to_string());
    let mut power_monitor = PowerMonitor::new()?;
    
    // Start server
    let server = QuicServer::new()?;
    let server_addr = format!("127.0.0.1:{}", port);
    let server_addr_clone = server_addr.clone();
    
    tokio::spawn(async move {
        let mut incoming = server.listen(&server_addr_clone).await.unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_stream().await {
                    tokio::spawn(async move {
                        while let Ok(msg) = stream.receive_message().await {
                            let response = format!("Echo: {}", String::from_utf8_lossy(&msg));
                            let _ = stream.send_message(response.into()).await;
                        }
                    });
                }
            });
        }
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(500)).await;
    
    // Start client
    let client = QuicClient::new()?;
    let connection = client.connect(&server_addr).await?;
    
    let start_time = Instant::now();
    let mut messages_sent = 0u64;
    let mut bytes_transferred = 0u64;
    
    // Run latency test for the specified duration with a single reused stream
    let test_end_time = start_time + TEST_DURATION;
    let mut iteration = 0;
    let mut stream = connection.open_stream().await?;
    
    while Instant::now() < test_end_time {
        let message = Bytes::from(vec![0u8; MESSAGE_SIZE]);
        
        stream.send_message(message.clone()).await?;
        let _response = stream.receive_message().await?;
        
        messages_sent += 1;
        bytes_transferred += MESSAGE_SIZE as u64 * 2; // Request + response
        
        // Sample power every 100 iterations
        if iteration % 100 == 0 {
            if let Ok(power_data) = power_monitor.sample() {
                results.power_samples.push(power_data);
            }
        }
        
        iteration += 1;
        
        // Very small delay to prevent overwhelming the system but allow high message rates
        // For pure latency testing, we want minimal artificial delays
        sleep(Duration::from_millis(1)).await;
    }
    
    results.duration = start_time.elapsed();
    results.messages_sent = messages_sent;
    results.bytes_transferred = bytes_transferred;
    results.calculate_averages();
    
    Ok(results)
}

async fn run_flood_test(backend: &str, port: u16) -> Result<TestResults, Box<dyn std::error::Error>> {
    println!("📝 Running {} Flood Test (60s of high throughput, single stream)", backend);
    
    let mut results = TestResults::new(backend.to_string(), "Flood".to_string());
    let mut power_monitor = PowerMonitor::new()?;
    
    // Start server
    let server = QuicServer::new()?;
    let server_addr = format!("127.0.0.1:{}", port);
    let server_addr_clone = server_addr.clone();
    
    tokio::spawn(async move {
        let mut incoming = server.listen(&server_addr_clone).await.unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_stream().await {
                    tokio::spawn(async move {
                        while let Ok(msg) = stream.receive_message().await {
                            let response = format!("Echo: {}", String::from_utf8_lossy(&msg));
                            let _ = stream.send_message(response.into()).await;
                        }
                    });
                }
            });
        }
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(500)).await;
    
    // Start client
    let client = QuicClient::new()?;
    let connection = client.connect(&server_addr).await?;
    
    let start_time = Instant::now();
    let mut messages_sent = 0u64;
    let mut bytes_transferred = 0u64;
    
    // Power monitoring
    let mut power_samples = Vec::new();
    let sample_interval = Duration::from_millis(500);
    let mut last_sample = Instant::now();
    
    // Flood test - send messages as fast as possible using a single stream
    let mut stream = connection.open_stream().await?;
    
    while start_time.elapsed() < TEST_DURATION {
        let message = Bytes::from(vec![0u8; MESSAGE_SIZE]);
        
        stream.send_message(message.clone()).await?;
        let _response = stream.receive_message().await?;
        
        messages_sent += 1;
        bytes_transferred += MESSAGE_SIZE as u64 * 2;
        
        // Sample power periodically
        if last_sample.elapsed() >= sample_interval {
            if let Ok(power_data) = power_monitor.sample() {
                power_samples.push(power_data);
            }
            last_sample = Instant::now();
        }
    }
    
    results.duration = start_time.elapsed();
    results.messages_sent = messages_sent;
    results.bytes_transferred = bytes_transferred;
    results.power_samples = power_samples;
    results.calculate_averages();
    
    Ok(results)
}

async fn run_stream_test(backend: &str, port: u16) -> Result<TestResults, Box<dyn std::error::Error>> {
    println!("📝 Running {} Stream Test (Large data transfer, max 60s)", backend);
    
    let mut results = TestResults::new(backend.to_string(), "Stream".to_string());
    let mut power_monitor = PowerMonitor::new()?;
    
    // Start server
    let server = QuicServer::new()?;
    let server_addr = format!("127.0.0.1:{}", port);
    let server_addr_clone = server_addr.clone();
    
    tokio::spawn(async move {
        let mut incoming = server.listen(&server_addr_clone).await.unwrap();
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                while let Ok(mut stream) = conn.accept_stream().await {
                    tokio::spawn(async move {
                        while let Ok(_msg) = stream.receive_message().await {
                            let response = b"ACK";
                            let _ = stream.send_message(Bytes::from(response.to_vec())).await;
                        }
                    });
                }
            });
        }
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(500)).await;
    
    // Start client
    let client = QuicClient::new()?;
    let connection = client.connect(&server_addr).await?;
    
    let start_time = Instant::now();
    let mut messages_sent = 0u64;
    let mut bytes_transferred = 0u64;
    
    // Large chunk size for streaming
    let chunk_size = 65536; // 64KB chunks
    let target_bytes = 100 * 1024 * 1024; // 100MB total
    let mut stream = connection.open_stream().await?;
    
    // Power monitoring
    let mut power_samples = Vec::new();
    let sample_interval = Duration::from_millis(200);
    let mut last_sample = Instant::now();
    
    // Stream large data (up to target_bytes or TEST_DURATION, whichever comes first)
    let mut total_sent = 0;
    while total_sent < target_bytes && start_time.elapsed() < TEST_DURATION {
        let chunk = Bytes::from(vec![0u8; chunk_size]);
        stream.send_message(chunk.clone()).await?;
        let _ack = stream.receive_message().await?;
        
        messages_sent += 1;
        bytes_transferred += chunk_size as u64;
        total_sent += chunk_size;
        
        // Sample power
        if last_sample.elapsed() >= sample_interval {
            if let Ok(power_data) = power_monitor.sample() {
                power_samples.push(power_data);
            }
            last_sample = Instant::now();
        }
    }
    
    results.duration = start_time.elapsed();
    results.messages_sent = messages_sent;
    results.bytes_transferred = bytes_transferred;
    results.power_samples = power_samples;
    results.calculate_averages();
    
    Ok(results)
}

fn compare_results(tquic_results: &[TestResults], quinn_results: &[TestResults]) {
    println!("\n🔬 Power Efficiency Comparison: tQUIC vs Quinn");
    println!("================================================");
    
    for (tquic, quinn) in tquic_results.iter().zip(quinn_results.iter()) {
        println!("\n{} Test Comparison:", tquic.test_type);
        println!("  tQUIC Power: {:.2}W | Quinn Power: {:.2}W", 
                 tquic.avg_cpu_power + tquic.avg_dram_power,
                 quinn.avg_cpu_power + quinn.avg_dram_power);
        
        let tquic_efficiency = if tquic.messages_sent > 0 {
            (tquic.avg_cpu_power + tquic.avg_dram_power) * tquic.duration.as_secs_f64() / tquic.messages_sent as f64
        } else { 0.0 };
        
        let quinn_efficiency = if quinn.messages_sent > 0 {
            (quinn.avg_cpu_power + quinn.avg_dram_power) * quinn.duration.as_secs_f64() / quinn.messages_sent as f64
        } else { 0.0 };
        
        println!("  Energy per Message: tQUIC {:.6}J | Quinn {:.6}J", 
                 tquic_efficiency, quinn_efficiency);
        
        if quinn_efficiency > 0.0 && tquic_efficiency > 0.0 {
            let efficiency_ratio = tquic_efficiency / quinn_efficiency;
            if efficiency_ratio < 1.0 {
                println!("  → tQUIC is {:.1}x more energy efficient", 1.0 / efficiency_ratio);
            } else {
                println!("  → Quinn is {:.1}x more energy efficient", efficiency_ratio);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend_name = get_backend_name();
    let is_default_backend = cfg!(feature = "backend-tquic");
    
    println!("🔋 QuicCrunch Power Monitoring Benchmark Comparison");
    println!("==================================================");
    
    if is_default_backend {
        println!("🚀 Testing Default Backend: {} (High-Performance)", backend_name);
    } else {
        println!("🔄 Testing Comparison Backend: {}", backend_name);
    }
    
    // Check if power monitoring is available
    if let Ok(mut monitor) = PowerMonitor::new() {
        match monitor.sample() {
            Ok(_) => println!("🔋 Power monitoring available - starting comprehensive tests"),
            Err(_) => println!("⚠️  Power monitoring requires sudo - frequency/temperature only"),
        }
    } else {
        println!("❌ Power monitoring not available on this system");
        return Ok(());
    }
    
    let mut all_results = Vec::new();
    
    // Test current backend
    println!("\n🚀 Testing {} Backend", backend_name);
    println!("===================================");
    let latency_result = run_latency_test(backend_name, 55001).await?;
    latency_result.print_summary();
    all_results.push(latency_result);
    
    let flood_result = run_flood_test(backend_name, 55002).await?;
    flood_result.print_summary();
    all_results.push(flood_result);
    
    let stream_result = run_stream_test(backend_name, 55003).await?;
    stream_result.print_summary();
    all_results.push(stream_result);
    
    // Provide instructions for backend comparison
    if is_default_backend {
        println!("\n🚀 Testing Quinn Backend (Comparison)");
        println!("=====================================");
        println!("⚠️  Quinn backend testing requires compilation with --features backend-quinn");
        println!("   To compare: cargo run --example power_benchmark_comparison --no-default-features --features backend-quinn");
    } else {
        println!("\n🚀 Testing tQUIC Backend (Default)");
        println!("==================================");
        println!("⚠️  tQUIC backend testing requires compilation with default features");
        println!("   To compare: cargo run --example power_benchmark_comparison");
    }
    
    println!("\n🎉 Power monitoring benchmark completed!");
    println!("💡 To get actual power readings, run with: sudo nix develop -c cargo run --example power_benchmark_comparison");
    println!("📊 {} backend results shown above", backend_name);
    
    Ok(())
}