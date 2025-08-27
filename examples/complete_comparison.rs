use quiccrunch::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("QuicCrunch Performance Comparison");
    println!("==================================");

    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    println!("Testing Quinn backend...");
    match test_backend_quinn(tls_config.clone()).await {
        Ok(latency) => {
            println!("✅ Quinn Backend Results:");
            println!("  Average latency: {:.2}ms", latency.as_secs_f64() * 1000.0);
            println!("  Throughput (64-byte messages): {:.0} msg/sec", 1000.0 / (latency.as_secs_f64() * 1000.0));
        },
        Err(e) => {
            println!("❌ Quinn Backend Failed: {}", e);
        }
    }

    println!("\nTesting tQUIC backend...");
    match test_backend_tquic(tls_config.clone()).await {
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

#[cfg(feature = "backend-quinn")]
async fn test_backend_quinn(tls_config: TlsConfig) -> Result<Duration, Box<dyn Error>> {
    let transport = QuicTransport::new(tls_config)?;
    test_latency_impl(transport, "127.0.0.1:50061").await
}

#[cfg(not(feature = "backend-quinn"))]
async fn test_backend_quinn(_tls_config: TlsConfig) -> Result<Duration, Box<dyn Error>> {
    Err("Quinn backend not enabled".into())
}

#[cfg(feature = "backend-tquic")]
async fn test_backend_tquic(tls_config: TlsConfig) -> Result<Duration, Box<dyn Error>> {
    let transport = QuicTransport::new(tls_config)?;
    test_latency_impl(transport, "127.0.0.1:55555").await
}

#[cfg(not(feature = "backend-tquic"))]
async fn test_backend_tquic(_tls_config: TlsConfig) -> Result<Duration, Box<dyn Error>> {
    Err("tQUIC backend not enabled".into())
}

async fn test_latency_impl(transport: QuicTransport, addr: &str) -> Result<Duration, Box<dyn Error>> {
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
    let iterations = 5; // Small number for testing
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