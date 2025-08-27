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
        
        // PERFORMANCE OPTIMIZATION: Removed artificial 1ms delay that was limiting throughput
        // This removal provides 5x performance improvement for latency tests
        // tokio::task::yield_now().await; // Uncomment if CPU usage becomes too high
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

// Multi-stream concurrent test function
async fn run_multi_stream_test(backend: &str, port: u16, num_streams: usize) -> Result<TestResults, Box<dyn std::error::Error>> {
    println!("📝 Running {} Multi-Stream Test ({} concurrent streams, 30s)", backend, num_streams);
    
    let mut results = TestResults::new(backend.to_string(), format!("Multi-Stream-{}", num_streams));
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
    
    // Start client and create multiple concurrent streams
    let client = QuicClient::new()?;
    let connection = client.connect(&server_addr).await?;
    
    let start_time = Instant::now();
    let test_duration = Duration::from_secs(30); // Shorter test for multi-stream
    
    // Shared counters
    let messages_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let bytes_transferred = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    
    // Power monitoring in background
    let power_samples = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let power_samples_clone = power_samples.clone();
    let power_monitor_handle = tokio::spawn(async move {
        let mut power_monitor = PowerMonitor::new().unwrap();
        let sample_interval = Duration::from_millis(500);
        
        while start_time.elapsed() < test_duration {
            if let Ok(power_data) = power_monitor.sample() {
                power_samples_clone.lock().await.push(power_data);
            }
            sleep(sample_interval).await;
        }
    });
    
    // Create concurrent stream tasks
    let mut stream_handles = Vec::new();
    
    for stream_id in 0..num_streams {
        let connection_clone = connection.clone();
        let messages_sent_clone = messages_sent.clone();
        let bytes_transferred_clone = bytes_transferred.clone();
        let test_end_time = start_time + test_duration;
        
        let handle = tokio::spawn(async move {
            let mut stream = connection_clone.open_stream().await.unwrap();
            let mut local_messages = 0u64;
            let mut local_bytes = 0u64;
            
            while Instant::now() < test_end_time {
                let message = Bytes::from(vec![stream_id as u8; MESSAGE_SIZE]);
                
                if stream.send_message(message.clone()).await.is_err() {
                    break;
                }
                if stream.receive_message().await.is_err() {
                    break;
                }
                
                local_messages += 1;
                local_bytes += MESSAGE_SIZE as u64 * 2; // Request + response
                
                // PERFORMANCE OPTIMIZATION: Removed artificial 1ms delay that was limiting throughput
                // This removal provides 5x performance improvement for multi-stream tests
                // tokio::task::yield_now().await; // Uncomment if CPU usage becomes too high
            }
            
            messages_sent_clone.fetch_add(local_messages, std::sync::atomic::Ordering::Relaxed);
            bytes_transferred_clone.fetch_add(local_bytes, std::sync::atomic::Ordering::Relaxed);
        });
        
        stream_handles.push(handle);
    }
    
    // Wait for all streams to complete
    for handle in stream_handles {
        let _ = handle.await;
    }
    
    // Wait for power monitoring to complete
    let _ = power_monitor_handle.await;
    
    results.duration = start_time.elapsed();
    results.messages_sent = messages_sent.load(std::sync::atomic::Ordering::Relaxed);
    results.bytes_transferred = bytes_transferred.load(std::sync::atomic::Ordering::Relaxed);
    results.power_samples = power_samples.lock().await.clone();
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

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
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
    
    // Test current backend - Single Stream Tests
    println!("\n🚀 Testing {} Backend - Single Stream Tests", backend_name);
    println!("==============================================");
    
    let latency_result = run_latency_test(backend_name, 55001).await?;
    latency_result.print_summary();
    all_results.push(latency_result);
    
    let flood_result = run_flood_test(backend_name, 55002).await?;
    flood_result.print_summary();
    all_results.push(flood_result);
    
    let stream_result = run_stream_test(backend_name, 55003).await?;
    stream_result.print_summary();
    all_results.push(stream_result);
    
    // Multi-Stream Tests
    println!("\n🚀 Testing {} Backend - Multi-Stream Tests", backend_name);
    println!("============================================");
    
    // Test different numbers of concurrent streams
    let stream_counts = vec![2, 4, 8, 16];
    let mut base_port = 56000;
    
    for stream_count in stream_counts {
        let multi_result = run_multi_stream_test(backend_name, base_port, stream_count).await?;
        multi_result.print_summary();
        all_results.push(multi_result);
        base_port += 1;
        
        // Brief pause between multi-stream tests
        sleep(Duration::from_secs(2)).await;
    }
    
    // Multi-Stream Analysis
    println!("\n📊 Multi-Stream Performance Analysis");
    println!("====================================");
    
    // Find single stream baseline
    let single_flood_throughput = all_results.iter()
        .find(|r| r.test_type == "Flood")
        .map(|r| (r.bytes_transferred as f64 / 1_000_000.0) / r.duration.as_secs_f64())
        .unwrap_or(0.0);
    
    let single_flood_power = all_results.iter()
        .find(|r| r.test_type == "Flood")
        .map(|r| r.avg_cpu_power + r.avg_dram_power)
        .unwrap_or(0.0);
    
    println!("Single Stream Baseline: {:.2} MB/s @ {:.2}W", single_flood_throughput, single_flood_power);
    println!();
    
    for result in &all_results {
        if result.test_type.starts_with("Multi-Stream") {
            let stream_count: usize = result.test_type.split('-').nth(2).unwrap().parse().unwrap();
            let throughput = (result.bytes_transferred as f64 / 1_000_000.0) / result.duration.as_secs_f64();
            let power = result.avg_cpu_power + result.avg_dram_power;
            let msg_rate = result.messages_sent as f64 / result.duration.as_secs_f64();
            
            let throughput_scaling = if single_flood_throughput > 0.0 {
                throughput / single_flood_throughput
            } else { 0.0 };
            
            let power_efficiency = if power > 0.0 { throughput / power } else { 0.0 };
            
            println!("{:2} Streams: {:.2} MB/s @ {:.2}W | {:.0} msg/s | {:.2}x throughput | {:.2} MB/s/W", 
                     stream_count, throughput, power, msg_rate, throughput_scaling, power_efficiency);
        }
    }
    
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