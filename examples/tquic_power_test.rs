// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! tQUIC Backend Power Monitoring Test
//! 
//! This example tests the tQUIC backend with comprehensive power monitoring,
//! measuring power consumption, efficiency, and performance metrics.

use quiccrunch::{QuicTransport, Transport, TlsConfig, power::PowerMonitor};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "cert_utils.rs"]
mod cert_utils;

#[derive(Debug, Clone)]
struct PowerMetrics {
    start_time: Instant,
    package_power_w: f64,
    dram_power_w: f64,
    cpu_frequency_mhz: f64,
    cpu_temperature_c: f64,
    messages_sent: u64,
    bytes_transferred: u64,
    efficiency_w_per_mhz: f64,
    energy_per_message_j: f64,
}

impl PowerMetrics {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            package_power_w: 0.0,
            dram_power_w: 0.0,
            cpu_frequency_mhz: 0.0,
            cpu_temperature_c: 0.0,
            messages_sent: 0,
            bytes_transferred: 0,
            efficiency_w_per_mhz: 0.0,
            energy_per_message_j: 0.0,
        }
    }

    fn update(&mut self, monitor: &mut PowerMonitor, messages: u64, bytes: u64) -> Result<(), Box<dyn Error>> {
        self.package_power_w = monitor.get_package_power()?;
        self.dram_power_w = monitor.get_dram_power()?;
        self.cpu_frequency_mhz = monitor.get_cpu_frequency()?;
        self.cpu_temperature_c = monitor.get_cpu_temperature()?;
        self.messages_sent = messages;
        self.bytes_transferred = bytes;
        
        // Calculate efficiency metrics
        let total_power = self.package_power_w + self.dram_power_w;
        self.efficiency_w_per_mhz = if self.cpu_frequency_mhz > 0.0 {
            total_power / self.cpu_frequency_mhz
        } else {
            0.0
        };
        
        let elapsed_secs = self.start_time.elapsed().as_secs_f64();
        self.energy_per_message_j = if self.messages_sent > 0 && elapsed_secs > 0.0 {
            (total_power * elapsed_secs) / self.messages_sent as f64
        } else {
            0.0
        };
        
        Ok(())
    }

    fn print_summary(&self) {
        let elapsed = self.start_time.elapsed();
        let total_power = self.package_power_w + self.dram_power_w;
        let throughput_mbps = if elapsed.as_secs() > 0 {
            (self.bytes_transferred as f64) / (1024.0 * 1024.0) / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let message_rate = if elapsed.as_secs() > 0 {
            self.messages_sent as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        println!("\n🔋 tQUIC Backend Power Monitoring Summary");
        println!("=========================================");
        println!("⏱️  Test Duration: {:.2}s", elapsed.as_secs_f64());
        println!("📊 Performance Metrics:");
        println!("   Messages Sent: {}", self.messages_sent);
        println!("   Bytes Transferred: {} bytes ({:.2} MB)", self.bytes_transferred, self.bytes_transferred as f64 / (1024.0 * 1024.0));
        println!("   Message Rate: {:.1} msg/s", message_rate);
        println!("   Throughput: {:.2} MB/s", throughput_mbps);
        println!("🔌 Power Consumption:");
        println!("   CPU Package Power: {:.2} W", self.package_power_w);
        println!("   DRAM Power: {:.2} W", self.dram_power_w);
        println!("   Total Power: {:.2} W", total_power);
        println!("🌡️  System Status:");
        println!("   CPU Frequency: {:.0} MHz", self.cpu_frequency_mhz);
        println!("   CPU Temperature: {:.1}°C", self.cpu_temperature_c);
        println!("⚡ Efficiency Metrics:");
        println!("   Power Efficiency: {:.3} W/MHz", self.efficiency_w_per_mhz);
        println!("   Energy per Message: {:.6} J/msg", self.energy_per_message_j);
        println!("   Power per Throughput: {:.3} W/(MB/s)", if throughput_mbps > 0.0 { total_power / throughput_mbps } else { 0.0 });
        println!("=========================================\n");
    }
}

async fn run_tquic_power_test() -> Result<(), Box<dyn Error>> {
    println!("🚀 Starting tQUIC Backend Power Monitoring Test");
    
    // Initialize power monitor
    let mut power_monitor = PowerMonitor::new()?;
    println!("✅ Power monitor initialized");
    
    // Generate TLS certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };
    println!("✅ TLS certificates generated");

    // Create tQUIC transport
    let transport = QuicTransport::new(tls_config)?;
    println!("✅ tQUIC transport created");
    
    // Use random port to avoid conflicts
    let server_port = 60000 + (fastrand::u16(..2000));
    let server_addr = format!("127.0.0.1:{}", server_port);
    
    println!("🌐 Server listening on: {}", server_addr);
    
    // Start server
    let mut incoming = transport.listen(&server_addr).await?;
    println!("✅ tQUIC server started");
    
    // Shared counters for tracking messages and bytes
    let messages_sent = Arc::new(AtomicU64::new(0));
    let bytes_transferred = Arc::new(AtomicU64::new(0));
    let messages_sent_clone = messages_sent.clone();
    let bytes_transferred_clone = bytes_transferred.clone();
    
    // Server task
    let server_task = tokio::spawn(async move {
        while let Ok(connection) = incoming.accept().await {
            let messages_sent = messages_sent_clone.clone();
            let bytes_transferred = bytes_transferred_clone.clone();
            
            tokio::spawn(async move {
                while let Ok(mut stream) = connection.accept_bi().await {
                    let messages_sent = messages_sent.clone();
                    let bytes_transferred = bytes_transferred.clone();
                    
                    tokio::spawn(async move {
                        while let Ok(message) = stream.receive_message().await {
                            let response = format!("tQUIC Echo: {}", String::from_utf8_lossy(&message));
                            let response_bytes = response.as_bytes();
                            
                            messages_sent.fetch_add(1, Ordering::Relaxed);
                            bytes_transferred.fetch_add((message.len() + response_bytes.len()) as u64, Ordering::Relaxed);
                            
                            if let Err(_) = stream.send_message(response.into()).await {
                                break;
                            }
                        }
                    });
                }
            });
        }
    });
    
    // Give server time to start
    time::sleep(Duration::from_millis(100)).await;
    
    // Connect client
    let client = transport.connect(&server_addr).await?;
    println!("✅ tQUIC client connected");
    
    // Initialize power metrics
    let mut metrics = PowerMetrics::new();
    
    // Power monitoring task
    let messages_for_monitor = messages_sent.clone();
    let bytes_for_monitor = bytes_transferred.clone();
    let monitor_task = tokio::spawn(async move {
        let mut monitor = PowerMonitor::new().unwrap();
        let mut interval = time::interval(Duration::from_secs(1));
        let mut last_metrics = PowerMetrics::new();
        
        for i in 0..30 { // Monitor for 30 seconds
            interval.tick().await;
            
            let current_messages = messages_for_monitor.load(Ordering::Relaxed);
            let current_bytes = bytes_for_monitor.load(Ordering::Relaxed);
            
            if let Err(e) = last_metrics.update(&mut monitor, current_messages, current_bytes) {
                eprintln!("⚠️  Power monitoring error: {}", e);
                continue;
            }
            
            println!("📊 [{}s] Power: CPU={:.1}W DRAM={:.1}W Freq={:.0}MHz Temp={:.1}°C Msgs={} Bytes={}", 
                i + 1,
                last_metrics.package_power_w,
                last_metrics.dram_power_w, 
                last_metrics.cpu_frequency_mhz,
                last_metrics.cpu_temperature_c,
                current_messages,
                current_bytes
            );
        }
        
        last_metrics
    });
    
    // Client workload - send messages with different patterns
    println!("🔄 Starting client workload...");
    
    // Test Pattern 1: Small frequent messages
    println!("📝 Test Pattern 1: Small frequent messages");
    for i in 0..100 {
        let mut stream = client.open_bi().await?;
        let message = format!("Small message {}", i);
        stream.send_message(message.into()).await?;
        let _response = stream.receive_message().await?;
        time::sleep(Duration::from_millis(10)).await;
    }
    
    // Test Pattern 2: Medium batched messages  
    println!("📝 Test Pattern 2: Medium batched messages");
    for i in 0..50 {
        let mut stream = client.open_bi().await?;
        let message = format!("Medium message with more content - iteration {} - {}", i, "x".repeat(500));
        stream.send_message(message.into()).await?;
        let _response = stream.receive_message().await?;
        time::sleep(Duration::from_millis(20)).await;
    }
    
    // Test Pattern 3: Large messages
    println!("📝 Test Pattern 3: Large messages");
    for i in 0..20 {
        let mut stream = client.open_bi().await?;
        let message = format!("Large message {} - {}", i, "X".repeat(10000));
        stream.send_message(message.into()).await?;
        let _response = stream.receive_message().await?;
        time::sleep(Duration::from_millis(50)).await;
    }
    
    // Test Pattern 4: Concurrent streams
    println!("📝 Test Pattern 4: Concurrent streams");
    let mut handles = Vec::new();
    for i in 0..10 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            for j in 0..20 {
                if let Ok(mut stream) = client_clone.open_bi().await {
                    let message = format!("Concurrent stream {} message {}", i, j);
                    if stream.send_message(message.into()).await.is_ok() {
                        let _ = stream.receive_message().await;
                    }
                    time::sleep(Duration::from_millis(25)).await;
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all concurrent streams
    for handle in handles {
        let _ = handle.await;
    }
    
    println!("✅ Client workload completed");
    
    // Wait for monitoring to complete and get final metrics
    let final_metrics = monitor_task.await?;
    final_metrics.print_summary();
    
    // Clean shutdown
    server_task.abort();
    
    println!("🎉 tQUIC power monitoring test completed successfully!");
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Check if we have power monitoring capabilities
    match PowerMonitor::new() {
        Ok(_) => {
            println!("🔋 Power monitoring available - starting test");
            run_tquic_power_test().await
        }
        Err(e) => {
            println!("⚠️  Power monitoring not available: {}", e);
            println!("💡 Try running with sudo for full power monitoring capabilities");
            println!("🔄 Running basic tQUIC test without power monitoring...");
            
            // Run basic test without power monitoring
            basic_tquic_test().await
        }
    }
}

async fn basic_tquic_test() -> Result<(), Box<dyn Error>> {
    println!("🚀 Starting basic tQUIC test (no power monitoring)");
    
    // Generate TLS certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Create tQUIC transport
    let transport = QuicTransport::new(tls_config)?;
    
    let server_port = 60000 + (fastrand::u16(..2000));
    let server_addr = format!("127.0.0.1:{}", server_port);
    
    println!("🌐 Server listening on: {}", server_addr);
    
    // Start server
    let mut incoming = transport.listen(&server_addr).await?;
    println!("✅ tQUIC server started");
    
    // Server task
    let server_task = tokio::spawn(async move {
        if let Ok(connection) = incoming.accept().await {
            if let Ok(mut stream) = connection.accept_bi().await {
                if let Ok(message) = stream.receive_message().await {
                    let response = format!("tQUIC Echo: {}", String::from_utf8_lossy(&message));
                    let _ = stream.send_message(response.into()).await;
                }
            }
        }
    });
    
    // Give server time to start
    time::sleep(Duration::from_millis(100)).await;
    
    // Connect client and send test message
    let client = transport.connect(&server_addr).await?;
    println!("✅ tQUIC client connected");
    
    let mut stream = client.open_bi().await?;
    let test_message = "Hello, tQUIC World!";
    stream.send_message(test_message.into()).await?;
    let response = stream.receive_message().await?;
    
    println!("📨 Sent: {}", test_message);
    println!("📥 Received: {}", String::from_utf8_lossy(&response));
    
    // Clean shutdown
    server_task.abort();
    
    println!("🎉 Basic tQUIC test completed successfully!");
    
    Ok(())
}