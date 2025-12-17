mod error;
mod types;

use tokio::process::Command;
use error::{BenchError, Result};
use crate::config::QuicTransportSettings as TransportSettings;


pub use types::*;

pub struct FrequencyScaling {
    current_governor: String,
    original_governor: String,
}

impl FrequencyScaling {
    pub async fn new() -> Result<Self> {
        println!("Initializing frequency scaling...");
        
        // Read current governor
        println!("Running cpupower to get current governor...");
        let output = Command::new("sudo")
            .args([
                "cpupower",
                "-c", "all",
                "frequency-info",
                "-p"
            ])
            .output()
            .await
            .map_err(|e| {
                eprintln!("Failed to execute cpupower: {}", e);
                BenchError::System(format!("Failed to run cpupower: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            eprintln!("cpupower stderr:\n{}", stderr);
            eprintln!("cpupower stdout:\n{}", stdout);
            return Err(BenchError::System(format!("cpupower failed:\nstderr: {}\nstdout: {}", stderr, stdout)));
        }
            
        println!("Parsing cpupower output...");
        let stdout = String::from_utf8(output.stdout)
            .map_err(|_| BenchError::System("Invalid UTF-8 in cpupower output".to_string()))?;
            
        println!("cpupower output:\n{}", stdout);
            
        let current_governor = stdout
            .lines()
            .find(|line| line.contains("The governor"))
            .ok_or_else(|| BenchError::System("No governor information found".to_string()))?
            .split('"')
            .nth(1)
            .ok_or_else(|| BenchError::System("Unexpected cpupower output format".to_string()))?
            .to_string();
            
        println!("Current governor: {}", current_governor);
            
        // Verify we have control over CPU frequency
        println!("Verifying CPU frequency control...");
        let verify = Command::new("sudo")
            .args([
                "cpupower",
                "-c", "all",
                "frequency-info",
                "--governors"
            ])
            .output()
            .await
            .map_err(|e| BenchError::System(format!("Failed to verify CPU control: {}", e)))?;

        if verify.status.success() {
            let stdout = String::from_utf8_lossy(&verify.stdout);
            println!("Available governors:\n{}", stdout);
        }
            
        Ok(Self {
            current_governor: current_governor.clone(),
            original_governor: current_governor,
        })
    }
    
    pub async fn set_governor(&mut self, governor: String) -> Result<()> {
        println!("Setting governor to: {}", governor);
        let output = Command::new("sudo")
            .args([
                "cpupower",
                "-c", "all",  // Apply to all CPU cores
                "frequency-set",
                "--governor",
                &governor
            ])
            .output()
            .await
            .map_err(|e| BenchError::System(format!("Failed to set governor: {}", e)))?;
            
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            eprintln!("cpupower stderr:\n{}", stderr);
            eprintln!("cpupower stdout:\n{}", stdout);
            return Err(BenchError::System(format!("cpupower failed:\nstderr: {}\nstdout: {}", stderr, stdout)));
        }
            
        // Verify the governor was set
        println!("Verifying governor setting...");
        let verify = Command::new("sudo")
            .args([
                "cpupower",
                "-c", "all",
                "frequency-info",
                "-p"
            ])
            .output()
            .await
            .map_err(|e| BenchError::System(format!("Failed to verify governor: {}", e)))?;

        if verify.status.success() {
            let stdout = String::from_utf8_lossy(&verify.stdout);
            println!("Current governor settings:\n{}", stdout);
        }
            
        self.current_governor = governor;
        Ok(())
    }
    
    pub async fn set_frequency(&self, freq_mhz: Option<u32>) -> Result<()> {
        if let Some(freq) = freq_mhz {
            println!("Setting frequency to: {} MHz", freq);

            // Set the minimum and maximum frequency to the target frequency
            let min_freq = freq * 1000;  // Convert to kHz
            let max_freq = freq * 1000;  // Convert to kHz

            println!("Setting frequency bounds to {}-{} kHz", min_freq, max_freq);

            // Set minimum frequency
            let output = Command::new("sudo")
                .args([
                    "cpupower",
                    "-c", "all",
                    "frequency-set",
                    "--min",
                    &format!("{}KHz", min_freq),
                ])
                .output()
                .await
                .map_err(|e| BenchError::System(format!("Failed to set min frequency: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                eprintln!("cpupower stderr:\n{}", stderr);
                eprintln!("cpupower stdout:\n{}", stdout);
                return Err(BenchError::System(format!("Failed to set min frequency: stderr: {}\nstdout: {}", stderr, stdout)));
            }

            // Set maximum frequency
            let output = Command::new("sudo")
                .args([
                    "cpupower",
                    "-c", "all",
                    "frequency-set",
                    "--max",
                    &format!("{}KHz", max_freq),
                ])
                .output()
                .await
                .map_err(|e| BenchError::System(format!("Failed to set max frequency: {}", e)))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                eprintln!("cpupower stderr:\n{}", stderr);
                eprintln!("cpupower stdout:\n{}", stdout);
                return Err(BenchError::System(format!("Failed to set max frequency: stderr: {}\nstdout: {}", stderr, stdout)));
            }

            // Verify the frequency settings
            println!("Verifying frequency settings...");
            let verify = Command::new("sudo")
                .args([
                    "cpupower",
                    "-c", "all",
                    "frequency-info",
                    "-fm",  // Show frequency range
                ])
                .output()
                .await
                .map_err(|e| BenchError::System(format!("Failed to verify frequency: {}", e)))?;

            if verify.status.success() {
                let stdout = String::from_utf8_lossy(&verify.stdout);
                println!("Current frequency settings:\n{}", stdout);
            }
        }
            
        Ok(())
    }
    
    pub async fn run_scaling_tests(&mut self) -> Result<Vec<ScalingTestResult>> {
        println!("Starting scaling tests...");
        let mut results = Vec::new();
        
        let configs = self.generate_test_configs();
        println!("Generated {} test configurations", configs.len());
        
        let original_governor = self.original_governor.clone();

        for (i, config) in configs.iter().enumerate() {
            println!("\nTesting configuration {}/{}:", i+1, configs.len());
            println!("  Frequency: {:?} MHz", config.frequency_mhz);
            println!("  Governor: {}", config.governor);

            // Apply configuration
            self.set_governor(config.governor.clone()).await?;
            self.set_frequency(config.frequency_mhz).await?;
            
            // Run performance test and collect metrics
            let result = self.run_single_test(config).await?;
            results.push(result);
        }
        
        // Restore original settings
        println!("\nRestoring original settings...");
        self.set_governor(original_governor).await?;
        
        println!("Testing completed successfully");
        Ok(results)
    }
    
    fn generate_test_configs(&self) -> Vec<ScalingConfig> {
        let mut configs = Vec::new();
        
        // Start with performance governor tests
        configs.push(ScalingConfig {
            frequency_mhz: None,
            governor: "performance".to_string(),
        });

        // Then test powersave governor
        configs.push(ScalingConfig {
            frequency_mhz: None,
            governor: "powersave".to_string(),
        });

        // Test with restricted frequency ranges under performance governor
        let frequencies = vec![
            (1200, 1600),  // Low range
            (2000, 2400),  // Mid range
            (2800, 3200),  // High range
        ];

        for (min_freq, max_freq) in frequencies {
            configs.push(ScalingConfig {
                frequency_mhz: Some(min_freq),
                governor: "performance".to_string(),
            });
            configs.push(ScalingConfig {
                frequency_mhz: Some(max_freq),
                governor: "performance".to_string(),
            });
        }
        
        configs
    }
    
    async fn run_single_test(&self, config: &ScalingConfig) -> Result<ScalingTestResult> {
                use std::time::Duration;
        use crate::{transport::{QuicTransport, Transport}, test_utils::certs};
        
        println!("Running QUIC performance test...");
        
        // Set up certificates
        println!("Generating TLS certificates...");
        let tls_config = certs::generate_node_cert("test-node");
        
        println!("Configuring transport settings (backend-neutral)...");
        let _transport_settings = TransportSettings::default();
        
        println!("Initializing QUIC transport...");
        let _transport_settings = TransportSettings::default();
        let transport = QuicTransport::new(tls_config.into()).await.map_err(|e| {
            eprintln!("Transport setup failed: {}", e);
            BenchError::System(format!("Transport setup failed: {}", e))
        })?;
        println!("QUIC transport initialized successfully");
        
        // Use a dynamic port range starting from 50000
        let mut port = 50000;
        let mut addr = format!("127.0.0.1:{}", port);
        let mut incoming = None;
        
        // Try ports until we find an available one
        for _ in 0..100 {  // Try up to 100 ports
            println!("Attempting to start server on {}", addr);
            match transport.listen(&addr).await {
                Ok(i) => {
                    incoming = Some(i);
                    println!("Server listening successfully on {}", addr);
                    break;
                }
                Err(e) => {
                    println!("Port {} in use, trying next port. Error: {}", port, e);
                    port += 1;
                    addr = format!("127.0.0.1:{}", port);
                }
            }
        }
        
        let mut incoming = incoming.ok_or_else(|| BenchError::System("Could not find available port".to_string()))?;
        
        // Use same address for client connection
        let server_addr = addr.clone();

        // Server handler
        println!("Spawning server handler task...");
        let server_handle = tokio::spawn(async move {
            println!("Server: Waiting for incoming connection...");
            let mut conn = crate::transport::Listener::accept(&mut incoming).await.expect("Accept failed");
            println!("Server: Accepted connection, opening bidirectional stream...");
            let mut stream = conn.open_bi().await.expect("Open stream failed");
            println!("Server: Bidirectional stream established");
            
            let mut total_bytes = 0u64;
            let mut messages = 0u64;
            let start = std::time::Instant::now();
            
            while start.elapsed() < Duration::from_secs(10) {
                match tokio::time::timeout(Duration::from_secs(5), stream.receive_message()).await {
                    Ok(Ok(msg)) => {
                        println!("Server: Received message of size {}", msg.len());
                        total_bytes += msg.len() as u64;
                        messages += 1;
                        if let Err(e) = stream.send_message(msg).await {
                            eprintln!("Server send error: {}", e);
                            break;
                        }
                        println!("Server: Sent response");
                    }
                    Ok(Err(e)) => {
                        eprintln!("Server receive error: {}", e);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Server receive timeout: {}", e);
                        break;
                    }
                }
            }
            (total_bytes, messages, start.elapsed())
        });
        
        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        // Client connection
        let addr_str = server_addr.to_string();
        println!("Attempting to connect to server at: {}", addr_str);
        
        // Try connection with retry
        let mut retry_count = 0;
        let max_retries = 3;
        let mut client = loop {
            println!("Client: Attempting connection {}/{} to {}...", retry_count + 1, max_retries, addr_str);
            match transport.connect(&addr_str).await {
                Ok(c) => {
                    println!("Client: Successfully connected to server");
                    break c;
                },
                Err(e) => {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        eprintln!("Connection failed after {} retries: {}", max_retries, e);
                        eprintln!("Last error details: {:?}", e);
                        return Err(BenchError::System(format!("Connect failed after {} retries: {}", max_retries, e)));
                    }
                    eprintln!("Connection attempt {} failed: {}, retrying in 1 second...", retry_count, e);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        };
        let mut client = client.open_bi().await.map_err(|e| BenchError::System(format!("Open stream failed: {}", e)))?;
        
        // Test message sizes with their corresponding latencies and bandwidths
        let sizes = vec![
            128,        // 128B - initial test
            1024,       // 1KB - latency test
            4096,       // 4KB - medium size
            16384,      // 16KB - larger size
        ];

        let mut total_latency = 0.0;
        let mut latency_samples = 0u32;
        let mut total_bandwidth = 0.0;
        let mut bandwidth_samples = 0u32;

        println!("Testing various message sizes...");
        for &size in &sizes {
            // Multiple samples per size
            for _ in 0..5 {
                let data = bytes::Bytes::from(vec![0u8; size]);
                println!("Client: Sending message of size {}", size);
                let start = std::time::Instant::now();
                
                client.send_message(data.clone()).await
                    .map_err(|e| BenchError::System(format!("Send failed: {}", e)))?;
                println!("Client: Message sent, waiting for response");
                
                let received_data = tokio::time::timeout(
                    Duration::from_secs(5),
                    client.receive_message()
                ).await
                    .map_err(|e| BenchError::System(format!("Receive timeout: {}", e)))?
                    .map_err(|e| BenchError::System(format!("Receive failed: {}", e)))?;
                
                println!("Client: Received response of size {}", received_data.len());
                
                if received_data.len() != size {
                    return Err(BenchError::System(format!(
                        "Response size mismatch: expected {}, got {}", 
                        size, 
                        received_data.len()
                    )));
                }
                
                let elapsed = start.elapsed();
                if size <= 1024 {
                    let latency = elapsed.as_micros() as f64;
                    total_latency += latency;
                    latency_samples += 1;
                } else {
                    let bandwidth = (size as f64 * 2.0) / (elapsed.as_secs_f64() * 1024.0 * 1024.0); // Convert to GB/s
                    total_bandwidth += bandwidth;
                    bandwidth_samples += 1;
                }
            }
        }
        
        // Get server stats
        let (total_bytes, messages, duration) = server_handle.await
            .map_err(|e| BenchError::System(format!("Server error: {}", e)))?;
        
        // Calculate average metrics
        let avg_latency = if latency_samples > 0 {
            total_latency / latency_samples as f64
        } else {
            0.0
        };
        
        let avg_bandwidth = if bandwidth_samples > 0 {
            total_bandwidth / bandwidth_samples as f64
        } else {
            0.0
        };
        
        let ops_per_sec = if duration.as_secs_f64() > 0.0 {
            messages as f64 / duration.as_secs_f64()
        } else {
            0.0
        };
        
        println!("Performance test completed:");
        println!("  Average latency: {:.2} µs", avg_latency);
        println!("  Average bandwidth: {:.2} GB/s", avg_bandwidth);
        println!("  Operations/sec: {:.2}", ops_per_sec);
        println!("  Total bytes transferred: {} MB", total_bytes / (1024 * 1024));
        println!("  Total messages: {}", messages);
        println!("  Test duration: {:.2}s", duration.as_secs_f64());
        
        let perf = PerformanceMetrics {
            bandwidth_gbps: avg_bandwidth,
            latency_us: avg_latency,
            operations_per_sec: ops_per_sec as u64,
        };
        
        println!("Measuring power consumption...");
        let power = self.measure_power().await?;
        
        println!("Measuring temperatures...");
        let temps = self.measure_temperatures().await?;
        
        // Calculate efficiency metrics with safety checks
        let total_power = power.avg_package_power + power.avg_dram_power;
        let efficiency = EfficiencyMetrics {
            efficiency_score: if total_power > 0.0 {
                perf.bandwidth_gbps / total_power
            } else {
                0.0
            },
            performance_per_watt: if power.avg_package_power > 0.0 {
                perf.operations_per_sec as f64 / power.avg_package_power
            } else {
                0.0
            },
            energy_per_operation: if perf.operations_per_sec > 0 {
                (power.avg_package_power * 1000.0) / perf.operations_per_sec as f64
            } else {
                0.0
            },
        };
        
        Ok(ScalingTestResult {
            config: config.clone(),
            performance: perf,
            power,
            temperatures: temps,
            efficiency,
        })
    }
    
    async fn measure_power(&self) -> Result<PowerMetrics> {
        println!("Measuring power consumption...");
        let samples = 10;
        let interval = std::time::Duration::from_millis(100);
        
        let mut avg_package: f64 = 0.0;
        let mut avg_dram: f64 = 0.0;
        let mut peak_package: f64 = 0.0;
        let mut peak_dram: f64 = 0.0;
        
        for i in 0..samples {
            println!("Taking power sample {}/{}...", i+1, samples);
            
            // Read power consumption using RAPL interface
            let output = Command::new("sudo")
                .args([
                    "cat",
                    "/sys/class/powercap/intel-rapl/intel-rapl:0/energy_uj",  // Package energy
                    "/sys/class/powercap/intel-rapl/intel-rapl:0/intel-rapl:0:0/energy_uj",  // DRAM energy
                ])
                .output()
                .await
                .map_err(|e| BenchError::System(format!("Failed to read power: {}", e)))?;
                
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let values: Vec<f64> = stdout
                    .lines()
                    .filter_map(|line| line.parse::<f64>().ok())
                    .collect();
                    
                if values.len() >= 2 {
                    let package_power = values[0] / 1_000_000.0;  // Convert µJ to J
                    let dram_power = values[1] / 1_000_000.0;
                    
                    avg_package += package_power;
                    avg_dram += dram_power;
                    peak_package = peak_package.max(package_power);
                    peak_dram = peak_dram.max(dram_power);
                }
            }
            
            tokio::time::sleep(interval).await;
        }
        
        avg_package /= samples as f64;
        avg_dram /= samples as f64;
        
        Ok(PowerMetrics {
            avg_package_power: avg_package,
            avg_dram_power: avg_dram,
            peak_package_power: peak_package,
            peak_dram_power: peak_dram,
        })
    }
    
    async fn measure_temperatures(&self) -> Result<TemperatureMetrics> {
        println!("Reading CPU and memory temperatures...");
        
        // Read temperatures using sensors command
        let output = Command::new("sensors")
            .output()
            .await
            .map_err(|e| BenchError::System(format!("Failed to read temperatures: {}", e)))?;
            
        let mut cpu_package_temp: f64 = 0.0;
        let mut max_core_temp: f64 = 0.0;
        let mut memory_temp: f64 = 0.0;
        
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Parse coretemp output
            for line in stdout.lines() {
                if line.contains("Package id") && line.contains("°C") {
                    if let Some(temp) = line
                        .split_whitespace()
                        .find(|s| s.ends_with("°C"))
                        .and_then(|s| s.trim_end_matches("°C").parse::<f64>().ok())
                    {
                        cpu_package_temp = temp;
                    }
                } else if line.contains("Core") && line.contains("°C") {
                    if let Some(temp) = line
                        .split_whitespace()
                        .find(|s| s.ends_with("°C"))
                        .and_then(|s| s.trim_end_matches("°C").parse::<f64>().ok())
                    {
                        max_core_temp = max_core_temp.max(temp);
                    }
                } else if (line.contains("SODIMM") || line.contains("DIMM")) && line.contains("°C") {
                    if let Some(temp) = line
                        .split_whitespace()
                        .find(|s| s.ends_with("°C"))
                        .and_then(|s| s.trim_end_matches("°C").parse::<f64>().ok())
                    {
                        memory_temp = temp;
                    }
                }
            }
        }
        
        Ok(TemperatureMetrics {
            cpu_package_temp,
            max_core_temp,
            memory_temp,
        })
    }
}