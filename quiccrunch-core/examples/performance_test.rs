//! Simple Performance Test
//!
//! Direct performance test without criterion overhead

use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicServer};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== QuicCrunch Performance Test ===");

    // Test 1: Basic operations
    test_basic_operations();

    // Test 2: Connection establishment
    test_connection_establishment().await?;

    println!("=== Performance Test Complete ===");
    Ok(())
}

fn test_basic_operations() {
    println!("\n--- Basic Operations ---");

    let iterations = 1_000_000;

    // Test addition
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = 42 + 42;
    }
    let duration = start.elapsed();
    println!(
        "Addition ({} ops): {:.2} ns/op",
        iterations,
        duration.as_nanos() as f64 / iterations as f64
    );

    // Test string operations
    let start = Instant::now();
    let s = "Hello, World!";
    for _ in 0..iterations {
        let _ = s.len();
    }
    let duration = start.elapsed();
    println!(
        "String length ({} ops): {:.2} ns/op",
        iterations,
        duration.as_nanos() as f64 / iterations as f64
    );

    // Test vector operations
    let start = Instant::now();
    for _ in 0..iterations {
        let v: Vec<i32> = (0..100).collect();
        let _ = v.iter().sum::<i32>();
    }
    let duration = start.elapsed();
    println!(
        "Vector sum ({} ops): {:.2} ns/op",
        iterations,
        duration.as_nanos() as f64 / iterations as f64
    );
}

async fn test_connection_establishment() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n--- Connection Establishment ---");

    // Setup test server
    let bind_addr = "127.0.0.1:0".parse()?;
    let cert_file = "/tmp/cert.pem".to_string();
    let key_file = "/tmp/key.pem".to_string();

    // Generate test certificates if needed
    if !std::path::Path::new(&cert_file).exists() {
        generate_test_certificates(&cert_file, &key_file)?;
    }

    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(2)).await?;
    let addr = server.local_addr();
    let mut listener = server.listen(&addr.to_string()).await?;

    println!("Server listening on: {}", addr);

    // Start server in background
    let server_handle = tokio::spawn(async move {
        if let Ok(conn) = listener.accept().await {
            println!("Server: Connection accepted");
            if let Ok(mut stream) = conn.accept_stream().await {
                println!("Server: Stream accepted");
                let mut data = vec![0u8; 1024];
                loop {
                    match stream.receive(1024).await {
                        Ok(received) if !received.is_empty() => {
                            println!("Server: Received {} bytes, echoing back", received.len());
                            let _ = stream.send(&received, false).await;
                        }
                        Ok(_) => {
                            println!("Server: Empty receive, closing");
                            break;
                        }
                        Err(e) => {
                            println!("Server: Error: {}", e);
                            break;
                        }
                    }
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Test connection establishment time
    let connection_tests = 10;
    let mut total_time = tokio::time::Duration::ZERO;

    for i in 0..connection_tests {
        let start = Instant::now();

        let client = TquicClient::with_options(RuntimeOptions {
            // Loopback self-signed test certificate: verification is explicitly off.
            verify_server_cert: false,
            ..RuntimeOptions::default()
        })
        .await?;
        let connection = client.connect(&addr.to_string()).await?;
        let mut stream = connection.open_stream().await?;

        // Send test message
        let test_data = b"Hello, Server!";
        let _ = stream.send(test_data, false).await;
        let _ = stream.receive(1024).await;

        stream.close().await?;
        connection.close().await?;

        let duration = start.elapsed();
        total_time += duration;
        println!("Connection {}: {:?}", i + 1, duration);
    }

    let avg_time = total_time / connection_tests;
    println!("Average connection time: {:?}", avg_time);

    // Wait for server
    let _ = server_handle.await;

    Ok(())
}

fn generate_test_certificates(
    cert_file: &str,
    key_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, KeyPair};
    use std::fs::File;
    use std::io::Write;

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "localhost");

    let cert = Certificate::from_params(params)?;
    let cert_pem = cert.serialize_pem()?;
    let key_pem = cert.serialize_private_key_pem()?;

    let mut cert_file = File::create(cert_file)?;
    cert_file.write_all(cert_pem.as_bytes())?;

    let mut key_file = File::create(key_file)?;
    key_file.write_all(key_pem.as_bytes())?;

    Ok(())
}
