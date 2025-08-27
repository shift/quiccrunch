use quiccrunch::{QuicTransport, Transport, TlsConfig};
use std::time::{Duration, Instant};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("QUIC Backend Comparison Test");
    println!("===========================");
    println!("Comparing Quinn vs tQUIC backend performance\n");

    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Test 1: Transport Creation Performance
    println!("🔧 Test 1: Transport Creation Speed");
    
    #[cfg(feature = "backend-quinn")]
    {
        let start = Instant::now();
        let _transport = QuicTransport::new(tls_config.clone())?;
        let quinn_creation = start.elapsed();
        println!("   Quinn:  {:.3}ms", quinn_creation.as_secs_f64() * 1000.0);
    }
    
    #[cfg(feature = "backend-tquic")]
    {
        let start = Instant::now();
        let _transport = QuicTransport::new(tls_config.clone())?;
        let tquic_creation = start.elapsed();
        println!("   tQUIC:  {:.3}ms", tquic_creation.as_secs_f64() * 1000.0);
    }

    // Test 2: Listen Socket Performance
    println!("\n🌐 Test 2: Listen Socket Creation");
    let transport = QuicTransport::new(tls_config.clone())?;
    
    let mut listen_times = Vec::new();
    for i in 0..5 {
        let addr = format!("127.0.0.1:{}", 50080 + i);
        let start = Instant::now();
        
        match transport.listen(&addr).await {
            Ok(_) => {
                let listen_time = start.elapsed();
                listen_times.push(listen_time);
                println!("   Listen {}: {:.3}ms", i + 1, listen_time.as_secs_f64() * 1000.0);
            }
            Err(e) => {
                println!("   Listen {} failed: {}", i + 1, e);
            }
        }
        
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    if !listen_times.is_empty() {
        let avg_listen = listen_times.iter().sum::<Duration>() / listen_times.len() as u32;
        println!("   Average: {:.3}ms", avg_listen.as_secs_f64() * 1000.0);
        println!("   Rate:    {:.0} binds/sec", 1000.0 / (avg_listen.as_secs_f64() * 1000.0));
    }

    // Test 3: Connection Establishment Attempt
    println!("\n🤝 Test 3: Connection Establishment");
    
    let server_addr = "127.0.0.1:50090";
    let incoming = transport.listen(server_addr).await?;
    
    // Simple connection acceptance
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            println!("   Server: Connection accepted");
            // Don't attempt streaming since it's not fully implemented
            drop(conn);
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connection attempt
    let start = Instant::now();
    match transport.connect(server_addr).await {
        Ok(client) => {
            let connect_time = start.elapsed();
            println!("   Connection established: {:.2}ms", connect_time.as_secs_f64() * 1000.0);
            drop(client);
        }
        Err(e) => {
            let attempt_time = start.elapsed();
            println!("   Connection failed after {:.2}ms: {}", attempt_time.as_secs_f64() * 1000.0, e);
        }
    }

    // Test 4: Resource Usage
    println!("\n📊 Test 4: Backend Resource Footprint");
    
    let start_time = Instant::now();
    let mut transports = Vec::new();
    
    // Create multiple transports to test resource usage
    for i in 0..10 {
        match QuicTransport::new(tls_config.clone()) {
            Ok(t) => transports.push(t),
            Err(e) => {
                println!("   Transport {} creation failed: {}", i, e);
                break;
            }
        }
    }
    
    let creation_total = start_time.elapsed();
    println!("   Created {} transports in {:.2}ms", transports.len(), creation_total.as_secs_f64() * 1000.0);
    println!("   Avg per transport: {:.3}ms", creation_total.as_secs_f64() * 1000.0 / transports.len() as f64);

    // Summary
    println!("\n🎯 Backend Comparison Summary:");
    
    #[cfg(feature = "backend-quinn")]
    println!("   🏃‍♂️ Running Quinn backend");
    
    #[cfg(feature = "backend-tquic")]
    println!("   🏃‍♂️ Running tQUIC backend");
    
    #[cfg(all(not(feature = "backend-quinn"), not(feature = "backend-tquic")))]
    println!("   ❌ No backend enabled");

    println!("   📈 Performance data collected for current backend");
    println!("   🔄 Run with different --features to compare backends:");
    println!("      cargo run --features backend-quinn --no-default-features --example backend_comparison");
    println!("      cargo run --features backend-tquic --no-default-features --example backend_comparison");

    Ok(())
}