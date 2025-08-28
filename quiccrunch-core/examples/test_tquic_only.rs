use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("tQUIC Backend Performance Test");
    println!("==============================");

    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Use random port to avoid conflicts
    let port = 60000 + (fastrand::u16(..1000));
    let addr = format!("127.0.0.1:{}", port);
    
    println!("Testing tQUIC backend on {}...", addr);
    match test_tquic_latency(tls_config, &addr).await {
        Ok(latency) => {
            println!("✅ tQUIC Backend Results:");
            println!("  Average latency: {:.2}ms", latency.as_secs_f64() * 1000.0);
            println!("  Throughput (64-byte messages): {:.0} msg/sec", 1000.0 / (latency.as_secs_f64() * 1000.0));
        },
        Err(e) => {
            println!("❌ tQUIC Backend Failed: {}", e);
        }
    }

    Ok(())
}

async fn test_tquic_latency(tls_config: TlsConfig, addr: &str) -> Result<Duration, Box<dyn Error>> {
    let transport = QuicTransport::new(tls_config)?;
    
    // Start server
    let mut incoming = transport.listen(addr).await?;
    
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                if let Ok(mut stream) = conn.accept_bi().await {
                    while let Ok(msg) = stream.receive_message().await {
                        let _ = stream.send_message(msg).await;
                    }
                }
            });
        }
    });
    
    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Run latency test
    let iterations = 5;
    let message_data = vec![42u8; 64];
    let message = Bytes::from(message_data);
    let mut total_time = Duration::new(0, 0);
    
    for i in 0..iterations {
        let client = transport.connect(addr).await?;
        let mut stream = client.open_bi().await?;
        
        let start = Instant::now();
        stream.send_message(message.clone()).await?;
        let response = stream.receive_message().await?;
        let elapsed = start.elapsed();
        
        // Verify echo
        if response.len() != message.len() {
            return Err(format!("Echo test failed on iteration {}", i).into());
        }
        
        total_time += elapsed;
        
        // Small delay between tests
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    Ok(total_time / iterations)
}