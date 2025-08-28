/// Quick Performance Comparison Test
/// Runs basic performance tests with and without optimizations

use quiccrunch::{
    QuicTransport, Transport, TlsConfig
};
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::time::timeout;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ QuicCrunch Quick Performance Test");
    println!("===================================\n");

    // Generate test certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;
    let tls_config = TlsConfig { cert, key, ca_cert };

    // Test small messages (typical RPC size)
    run_small_message_test(&tls_config).await?;
    
    // Test medium messages 
    run_medium_message_test(&tls_config).await?;

    println!("\n✅ Quick performance test complete!");
    Ok(())
}

async fn run_small_message_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("📨 Small Message Test (1KB messages, 1000 iterations)");
    println!("----------------------------------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Spawn server
    let server_handle = tokio::spawn(async move {
        let mut total_processed = 0;
        let start_time = Instant::now();
        
        loop {
            match incoming.accept().await {
                Ok(connection) => {
                    loop {
                        match connection.accept_bi().await {
                            Ok(mut stream) => {
                                match stream.receive_message().await {
                                    Ok(data) => {
                                        // Echo back the data
                                        if let Err(e) = stream.send_message(data).await {
                                            eprintln!("Server send error: {}", e);
                                            break;
                                        }
                                        total_processed += 1;
                                        
                                        if total_processed >= 1000 {
                                            let duration = start_time.elapsed();
                                            println!("  Server processed {} messages in {:?}", total_processed, duration);
                                            println!("  Throughput: {:.2} msg/sec", total_processed as f64 / duration.as_secs_f64());
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Server receive error: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Server accept_bi error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Server accept error: {}", e);
                    break;
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client test
    let client_transport = QuicTransport::new(tls_config.clone())?;
    let connection = timeout(Duration::from_secs(5), 
        client_transport.connect(&server_addr)
    ).await??;

    // Create test message (1KB)
    let test_data = vec![0x42u8; 1024];
    let test_message = Bytes::from(test_data);

    let start_time = Instant::now();
    let mut successful_roundtrips = 0;

    for i in 0..1000 {
        match connection.open_bi().await {
            Ok(mut stream) => {
                // Send message
                if let Err(e) = stream.send_message(test_message.clone()).await {
                    eprintln!("Client send error: {}", e);
                    continue;
                }
                
                // Wait for response
                match timeout(Duration::from_millis(100), stream.receive_message()).await {
                    Ok(Ok(_response)) => {
                        successful_roundtrips += 1;
                    }
                    Ok(Err(e)) => {
                        eprintln!("Client receive error: {}", e);
                    }
                    Err(_) => {
                        eprintln!("Client receive timeout");
                    }
                }
            }
            Err(e) => {
                eprintln!("Client open_bi error: {}", e);
            }
        }
        
        if i % 100 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }
    
    let duration = start_time.elapsed();
    println!("\n  Client completed {} successful roundtrips in {:?}", successful_roundtrips, duration);
    if successful_roundtrips > 0 {
        println!("  Average latency: {:.2} ms", duration.as_millis() as f64 / successful_roundtrips as f64);
    }
    
    server_handle.abort();
    Ok(())
}

async fn run_medium_message_test(tls_config: &TlsConfig) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📦 Medium Message Test (64KB messages, 100 iterations)");
    println!("-----------------------------------------------------");

    let transport = QuicTransport::new(tls_config.clone())?;
    let mut incoming = transport.listen("127.0.0.1:0").await?;
    let server_addr = incoming.local_addr()?;

    // Spawn server
    let server_handle = tokio::spawn(async move {
        let mut total_bytes = 0u64;
        let start_time = Instant::now();
        
        loop {
            match incoming.accept().await {
                Ok(connection) => {
                    loop {
                        match connection.accept_bi().await {
                            Ok(mut stream) => {
                                match stream.receive_message().await {
                                    Ok(data) => {
                                        total_bytes += data.len() as u64;
                                        
                                        // Echo back
                                        if let Err(e) = stream.send_message(data).await {
                                            eprintln!("Server send error: {}", e);
                                            break;
                                        }
                                        
                                        if total_bytes >= 100 * 64 * 1024 { // 100 messages of 64KB each
                                            let duration = start_time.elapsed();
                                            let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64();
                                            println!("  Server processed {:.2} MB in {:?}", total_bytes as f64 / 1024.0 / 1024.0, duration);
                                            println!("  Throughput: {:.2} MB/sec", throughput_mbps);
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Server receive error: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Server accept_bi error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Server accept error: {}", e);
                    break;
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client_transport = QuicTransport::new(tls_config.clone())?;
    let connection = timeout(Duration::from_secs(5), 
        client_transport.connect(&server_addr)
    ).await??;

    // Create 64KB test message
    let test_data = vec![0x42u8; 64 * 1024];
    let test_message = Bytes::from(test_data);

    let start_time = Instant::now();
    let mut successful_transfers = 0;

    for i in 0..100 {
        match connection.open_bi().await {
            Ok(mut stream) => {
                if let Err(e) = stream.send_message(test_message.clone()).await {
                    eprintln!("Client send error: {}", e);
                    continue;
                }
                
                match timeout(Duration::from_millis(500), stream.receive_message()).await {
                    Ok(Ok(_response)) => {
                        successful_transfers += 1;
                    }
                    Ok(Err(e)) => {
                        eprintln!("Client receive error: {}", e);
                    }
                    Err(_) => {
                        eprintln!("Client receive timeout");
                    }
                }
            }
            Err(e) => {
                eprintln!("Client open_bi error: {}", e);
            }
        }
        
        if i % 10 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }
    
    let duration = start_time.elapsed();
    let total_mb = (successful_transfers * 64) as f64 / 1024.0;
    println!("\n  Client transferred {:.2} MB in {:?}", total_mb, duration);
    if successful_transfers > 0 {
        println!("  Client throughput: {:.2} MB/sec", total_mb / duration.as_secs_f64());
    }
    
    server_handle.abort();
    Ok(())
}

