/// Quick Performance Comparison Test
/// Runs basic performance tests with and without optimizations

use quiccrunch::{
    QuicTransport, Transport, TlsConfig, Connection, Frame, FrameType
};
use std::time::{Duration, Instant};
use bytes::Bytes;
use tokio::time::timeout;
use std::sync::Arc;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ QuicCrunch Quick Performance Test");
    println!("===================================\n");

    // Generate test certificates
    let (ca_cert, ca_key) = generate_ca_cert()?;
    let (cert, key) = generate_node_cert(&ca_cert, "localhost")?;
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
        
        while let Some(connection) = incoming.accept().await {
            let mut conn = connection.await.unwrap();
            while let Some(mut stream) = conn.accept_stream().await.unwrap() {
                if let Some(frame) = stream.recv().await.unwrap() {
                    // Echo back the frame
                    stream.send(frame).await.unwrap();
                    total_processed += 1;
                    
                    if total_processed >= 1000 {
                        let duration = start_time.elapsed();
                        println!("  Server processed {} messages in {:?}", total_processed, duration);
                        println!("  Throughput: {:.2} msg/sec", total_processed as f64 / duration.as_secs_f64());
                        return;
                    }
                }
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Client test
    let client_transport = QuicTransport::new(tls_config.clone())?;
    let mut connection = timeout(Duration::from_secs(5), 
        client_transport.connect(server_addr)
    ).await??;

    // Create test message (1KB)
    let test_data = vec![0x42u8; 1024];
    let test_frame = Frame {
        frame_type: FrameType::Data,
        data: Bytes::from(test_data),
    };

    let start_time = Instant::now();
    let mut successful_roundtrips = 0;

    for i in 0..1000 {
        let mut stream = connection.open_stream().await?;
        
        // Send message
        stream.send(test_frame.clone()).await?;
        
        // Wait for response
        if let Some(_response) = timeout(Duration::from_millis(100), stream.recv()).await? {
            successful_roundtrips += 1;
        }
        
        if i % 100 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }
    
    let duration = start_time.elapsed();
    println!("\n  Client completed {} successful roundtrips in {:?}", successful_roundtrips, duration);
    println!("  Average latency: {:.2} ms", duration.as_millis() as f64 / successful_roundtrips as f64);
    
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
        
        while let Some(connection) = incoming.accept().await {
            let mut conn = connection.await.unwrap();
            while let Some(mut stream) = conn.accept_stream().await.unwrap() {
                if let Some(frame) = stream.recv().await.unwrap() {
                    total_bytes += frame.data.len() as u64;
                    
                    // Echo back
                    stream.send(frame).await.unwrap();
                    
                    if total_bytes >= 100 * 64 * 1024 { // 100 messages of 64KB each
                        let duration = start_time.elapsed();
                        let throughput_mbps = (total_bytes as f64 / 1024.0 / 1024.0) / duration.as_secs_f64();
                        println!("  Server processed {:.2} MB in {:?}", total_bytes as f64 / 1024.0 / 1024.0, duration);
                        println!("  Throughput: {:.2} MB/sec", throughput_mbps);
                        return;
                    }
                }
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client_transport = QuicTransport::new(tls_config.clone())?;
    let mut connection = timeout(Duration::from_secs(5), 
        client_transport.connect(server_addr)
    ).await??;

    // Create 64KB test message
    let test_data = vec![0x42u8; 64 * 1024];
    let test_frame = Frame {
        frame_type: FrameType::Data,
        data: Bytes::from(test_data),
    };

    let start_time = Instant::now();
    let mut successful_transfers = 0;

    for i in 0..100 {
        let mut stream = connection.open_stream().await?;
        
        stream.send(test_frame.clone()).await?;
        
        if let Some(_response) = timeout(Duration::from_millis(500), stream.recv()).await? {
            successful_transfers += 1;
        }
        
        if i % 10 == 0 {
            print!(".");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }
    }
    
    let duration = start_time.elapsed();
    let total_mb = (successful_transfers * 64) as f64 / 1024.0;
    println!("\n  Client transferred {:.2} MB in {:?}", total_mb, duration);
    println!("  Client throughput: {:.2} MB/sec", total_mb / duration.as_secs_f64());
    
    server_handle.abort();
    Ok(())
}

fn generate_ca_cert() -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, DistinguishedName};
    
    let mut params = CertificateParams::new(vec!["QuicCrunch-CA".to_string()]);
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "QuicCrunch-CA");
    
    let ca_cert = Certificate::from_params(params)?;
    let ca_cert_der = ca_cert.serialize_der()?;
    let ca_key_der = ca_cert.serialize_private_key_der();
    
    Ok((ca_cert_der, ca_key_der))
}

fn generate_node_cert(ca_cert_der: &[u8], hostname: &str) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
    use rcgen::{Certificate, CertificateParams, DistinguishedName};
    
    let ca_cert = Certificate::from_der(ca_cert_der)?;
    
    let mut params = CertificateParams::new(vec![hostname.to_string()]);
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, hostname);
    
    let cert = Certificate::from_params(params)?;
    let cert_der = cert.serialize_der_with_signer(&ca_cert)?;
    let key_der = cert.serialize_private_key_der();
    
    Ok((cert_der, key_der))
}