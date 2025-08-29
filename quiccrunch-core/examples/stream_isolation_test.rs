// Isolated test to verify QuicCrunch stream messaging works correctly
// This test isolates the core stream functionality from benchmark complexity

use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use quiccrunch_core::transport::TransportSettings;
use std::error::Error as StdError;
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::time::sleep;

#[path = "cert_utils.rs"]
mod cert_utils;

#[derive(Debug)]
struct TestResult {
    message_size: usize,
    success: bool,
    error: Option<String>,
    duration: Duration,
}

async fn test_single_message_exchange(
    transport: &QuicTransport, 
    message_size: usize
) -> Result<TestResult, Box<dyn StdError + Send + Sync>> {
    let start = Instant::now();
    
    // Create test payload
    let test_payload = vec![42u8; message_size];
    let expected_payload = test_payload.clone();
    
    println!("Testing message size: {} bytes", message_size);
    
    // Connect to server
    let conn = transport.connect("127.0.0.1:50051").await?;
    let mut stream = conn.open_bi().await?;
    
    println!("  Client: Sending {} byte message...", message_size);
    
    // Send message
    stream.send_message(Bytes::from(test_payload)).await.map_err(|e| {
        format!("Send failed: {}", e)
    })?;
    
    println!("  Client: Message sent, waiting for response...");
    
    // Receive response
    let response = stream.receive_message().await.map_err(|e| {
        format!("Receive failed: {}", e)
    })?;
    
    let duration = start.elapsed();
    
    println!("  Client: Received {} byte response in {:?}", response.len(), duration);
    
    // Verify response matches what we sent
    let success = response.as_ref() == expected_payload.as_slice();
    
    if !success {
        let error_msg = format!(
            "Response mismatch: sent {} bytes, received {} bytes, data matches: {}", 
            expected_payload.len(), 
            response.len(),
            response.as_ref() == expected_payload.as_slice()
        );
        println!("  ❌ {}", error_msg);
        return Ok(TestResult {
            message_size,
            success: false,
            error: Some(error_msg),
            duration,
        });
    }
    
    println!("  ✅ Message exchange successful");
    
    Ok(TestResult {
        message_size,
        success: true,
        error: None,
        duration,
    })
}

async fn run_server(transport: QuicTransport) -> Result<(), Box<dyn StdError + Send + Sync>> {
    println!("Server: Starting listener on 127.0.0.1:50051");
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    
    println!("Server: Waiting for connections...");
    
    while let Ok(conn) = incoming.accept().await {
        println!("Server: New connection accepted");
        
        tokio::spawn(async move {
            let conn_result: Result<(), Box<dyn StdError + Send + Sync>> = async {
                loop {
                    // Accept new stream
                    let stream_result = conn.accept_bi().await;
                    match stream_result {
                        Ok(mut stream) => {
                            tokio::spawn(async move {
                                loop {
                                    // Receive message
                                    match stream.receive_message().await {
                                        Ok(msg) => {
                                            println!("Server: Received {} byte message", msg.len());
                                            
                                            // Echo back the same message
                                            match stream.send_message(msg).await {
                                                Ok(_) => {
                                                    println!("Server: Echoed message successfully");
                                                }
                                                Err(e) => {
                                                    println!("Server: Echo failed: {}", e);
                                                    break;
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            println!("Server: Receive error: {}", e);
                                            break;
                                        }
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            println!("Server: Stream accept error: {}", e);
                            break;
                        }
                    }
                }
                Ok(())
            }.await;
            
            if let Err(e) = conn_result {
                println!("Server: Connection error: {}", e);
            }
        });
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError + Send + Sync>> {
    println!("🧪 QuicCrunch Stream Isolation Test");
    println!("Testing core stream messaging without benchmark complexity\n");
    
    // Generate certificates
    let (ca_cert_der, _ca_key, ca_cert_rcgen) = cert_utils::generate_ca_cert()
        .map_err(|e| format!("CA cert error: {}", e))?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_rcgen, "localhost")
        .map_err(|e| format!("Node cert error: {}", e))?;
    
    let tls_config = TlsConfig { 
        cert, 
        key, 
        ca_cert: ca_cert_der,
        cert_pem: None,
        key_pem: None, 
        ca_cert_pem: None,
    };
    
    // Use large window settings to eliminate size limits
    let settings = TransportSettings {
        stream_receive_window: 10 * 1024 * 1024, // 10MB
        connection_receive_window: 20 * 1024 * 1024, // 20MB
        ..Default::default()
    };
    
    let server_transport = QuicTransport::with_settings(tls_config.clone(), settings.clone())?;
    let client_transport = QuicTransport::with_settings(tls_config, settings)?;
    
    // Start server
    let server_handle = tokio::spawn(async move {
        run_server(server_transport).await
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(200)).await;
    
    // Test message sizes that were failing in the benchmark
    let test_sizes = vec![
        64,      // Small message that was failing
        256,     // Medium small
        1024,    // 1KB
        4096,    // 4KB
        16384,   // 16KB
        65536,   // 64KB
        262144,  // 256KB
        1048576, // 1MB
        2097152, // 2MB
    ];
    
    println!("Starting sequential tests (no concurrency)...\n");
    
    let mut results = Vec::new();
    let mut all_passed = true;
    
    for size in test_sizes {
        match test_single_message_exchange(&client_transport, size).await {
            Ok(result) => {
                if !result.success {
                    all_passed = false;
                }
                results.push(result);
            }
            Err(e) => {
                println!("❌ Test failed for size {}: {}", size, e);
                all_passed = false;
                results.push(TestResult {
                    message_size: size,
                    success: false,
                    error: Some(e.to_string()),
                    duration: Duration::from_secs(0),
                });
            }
        }
        
        // Small delay between tests
        sleep(Duration::from_millis(100)).await;
    }
    
    // Print summary
    println!("\n📊 Test Results Summary:");
    println!("╭─────────────┬─────────┬─────────────┬─────────────────╮");
    println!("│ Message Size│ Status  │ Duration    │ Error           │");
    println!("├─────────────┼─────────┼─────────────┼─────────────────┤");
    
    for result in &results {
        let status = if result.success { "✅ PASS" } else { "❌ FAIL" };
        let size_str = if result.message_size < 1024 {
            format!("{}B", result.message_size)
        } else if result.message_size < 1024 * 1024 {
            format!("{}KB", result.message_size / 1024)
        } else {
            format!("{}MB", result.message_size / (1024 * 1024))
        };
        
        let error_str = result.error.as_ref()
            .map(|e| e.chars().take(15).collect::<String>())
            .unwrap_or_else(|| "None".to_string());
        
        println!("│ {:^11} │ {} │ {:>9.2?} │ {:^15} │", 
                 size_str, status, result.duration, error_str);
    }
    println!("╰─────────────┴─────────┴─────────────┴─────────────────╯");
    
    if all_passed {
        println!("\n🎉 All tests PASSED! QuicCrunch core libraries are working correctly.");
        println!("The issue is likely in the benchmark's concurrent client handling or server loop.");
    } else {
        println!("\n⚠️  Some tests FAILED. The issue is in the core QuicCrunch libraries.");
        let failed_sizes: Vec<_> = results.iter()
            .filter(|r| !r.success)
            .map(|r| r.message_size)
            .collect();
        println!("Failed message sizes: {:?}", failed_sizes);
    }
    
    // Test concurrent clients if basic tests pass
    if all_passed {
        println!("\n🔄 Testing concurrent clients...");
        
        let concurrent_result = test_concurrent_clients(&client_transport).await;
        match concurrent_result {
            Ok(success) => {
                if success {
                    println!("✅ Concurrent client test PASSED");
                    println!("The benchmark issue is likely in the specific implementation details.");
                } else {
                    println!("❌ Concurrent client test FAILED");
                    println!("The issue appears when multiple clients access the server simultaneously.");
                }
            }
            Err(e) => {
                println!("❌ Concurrent test error: {}", e);
            }
        }
    }
    
    // Don't abort server
    server_handle.abort();
    
    Ok(())
}

async fn test_concurrent_clients(transport: &QuicTransport) -> Result<bool, Box<dyn StdError + Send + Sync>> {
    println!("Testing 4 concurrent clients with 64-byte messages...");
    
    let mut handles = Vec::new();
    
    for client_id in 0..4 {
        let transport = transport.clone();
        let handle = tokio::spawn(async move {
            let conn = transport.connect("127.0.0.1:50051").await?;
            let mut stream = conn.open_bi().await?;
            
            // Send 10 messages per client
            for msg_id in 0..10 {
                let payload = vec![42u8; 64];
                
                stream.send_message(Bytes::from(payload.clone())).await
                    .map_err(|e| format!("Client {} msg {} send failed: {}", client_id, msg_id, e))?;
                
                let response = stream.receive_message().await
                    .map_err(|e| format!("Client {} msg {} receive failed: {}", client_id, msg_id, e))?;
                
                if response.as_ref() != payload.as_slice() {
                    return Err(format!("Client {} msg {} data mismatch", client_id, msg_id).into());
                }
            }
            
            Ok::<_, Box<dyn StdError + Send + Sync>>(())
        });
        handles.push(handle);
    }
    
    // Wait for all clients to complete
    let mut all_success = true;
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(Ok(_)) => println!("  Client {} completed successfully", i),
            Ok(Err(e)) => {
                println!("  Client {} failed: {}", i, e);
                all_success = false;
            }
            Err(e) => {
                println!("  Client {} panicked: {}", i, e);
                all_success = false;
            }
        }
    }
    
    Ok(all_success)
}