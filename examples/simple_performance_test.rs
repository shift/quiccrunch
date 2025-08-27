use quiccrunch::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[derive(Debug)]
struct SimpleMetrics {
    successful_connections: u64,
    failed_connections: u64,
    total_time: Duration,
    connection_times: Vec<Duration>,
}

impl SimpleMetrics {
    fn new() -> Self {
        Self {
            successful_connections: 0,
            failed_connections: 0,
            total_time: Duration::new(0, 0),
            connection_times: Vec::new(),
        }
    }

    fn print(&self, backend_name: &str, test_duration: Duration) {
        println!("\n{} Performance Results ({}s test):", backend_name, test_duration.as_secs());
        println!("=====================================");
        println!("Total connections attempted: {}", self.successful_connections + self.failed_connections);
        println!("Successful connections: {}", self.successful_connections);
        println!("Failed connections: {}", self.failed_connections);
        println!("Success rate: {:.1}%", 
                 (self.successful_connections as f64 / (self.successful_connections + self.failed_connections) as f64) * 100.0);
        
        if !self.connection_times.is_empty() {
            let avg_time = self.connection_times.iter().sum::<Duration>() / self.connection_times.len() as u32;
            let mut sorted_times = self.connection_times.clone();
            sorted_times.sort();
            let p50 = sorted_times[self.connection_times.len() / 2];
            let p95 = sorted_times[(self.connection_times.len() as f64 * 0.95) as usize];
            
            println!("Connection times:");
            println!("  Average: {:.2}ms", avg_time.as_secs_f64() * 1000.0);
            println!("  P50:     {:.2}ms", p50.as_secs_f64() * 1000.0);
            println!("  P95:     {:.2}ms", p95.as_secs_f64() * 1000.0);
        }
        
        let conn_per_sec = self.successful_connections as f64 / test_duration.as_secs_f64();
        println!("Throughput: {:.1} connections/sec", conn_per_sec);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("QuicCrunch Simple Performance Test");
    println!("=================================");
    
    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    let test_duration = Duration::from_secs(300); // 5 minutes

    // Test Quinn backend
    println!("Testing Quinn backend for {} seconds...", test_duration.as_secs());
    let quinn_metrics = test_connection_performance(
        tls_config.clone(),
        "127.0.0.1:50052",
        test_duration,
        "Quinn"
    ).await?;

    quinn_metrics.print("Quinn Backend", test_duration);

    // Test tQUIC backend if available
    #[cfg(feature = "backend-tquic")]
    {
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("\nTesting tQUIC backend for {} seconds...", test_duration.as_secs());
        let tquic_metrics = test_connection_performance_tquic(
            tls_config.clone(),
            "127.0.0.1:50053", 
            test_duration,
            "tQUIC"
        ).await?;

        tquic_metrics.print("tQUIC Backend", test_duration);
        
        // Compare results
        println!("\n📊 Performance Comparison:");
        println!("Quinn: {:.1} conn/s", quinn_metrics.successful_connections as f64 / test_duration.as_secs_f64());
        #[cfg(feature = "backend-tquic")]
        println!("tQUIC: {:.1} conn/s", tquic_metrics.successful_connections as f64 / test_duration.as_secs_f64());
    }

    #[cfg(not(feature = "backend-tquic"))]
    {
        println!("\n⚠️  tQUIC backend not available (feature not enabled)");
        println!("To test tQUIC, run with: --features backend-tquic --no-default-features");
    }

    Ok(())
}

async fn test_connection_performance(
    tls_config: TlsConfig,
    addr: &str,
    duration: Duration,
    backend_name: &str,
) -> Result<SimpleMetrics, Box<dyn Error>> {
    let mut metrics = SimpleMetrics::new();
    let transport = QuicTransport::new(tls_config)?;
    
    println!("  Starting {} server...", backend_name);
    
    // Start server
    let mut incoming = transport.listen(addr).await?;
    
    tokio::spawn(async move {
        while let Ok(_conn) = incoming.accept().await {
            // Just accept connections, no stream processing needed for this test
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    println!("  Beginning connection test...");
    let start_time = Instant::now();
    
    while start_time.elapsed() < duration {
        let connect_start = Instant::now();
        
        match transport.connect(addr).await {
            Ok(_conn) => {
                let connect_time = connect_start.elapsed();
                metrics.successful_connections += 1;
                metrics.connection_times.push(connect_time);
            }
            Err(_) => {
                metrics.failed_connections += 1;
            }
        }
        
        // Print progress every 30 seconds
        if start_time.elapsed().as_secs() % 30 == 0 && start_time.elapsed().as_secs() > 0 {
            let elapsed = start_time.elapsed().as_secs();
            let remaining = duration.as_secs() - elapsed;
            println!("    Progress: {}s elapsed, {}s remaining, {} connections so far", 
                     elapsed, remaining, metrics.successful_connections);
        }
        
        // Small delay to prevent overwhelming
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    metrics.total_time = start_time.elapsed();
    Ok(metrics)
}

#[cfg(feature = "backend-tquic")]
async fn test_connection_performance_tquic(
    tls_config: TlsConfig,
    addr: &str,
    duration: Duration,
    backend_name: &str,
) -> Result<SimpleMetrics, Box<dyn Error>> {
    let mut metrics = SimpleMetrics::new();
    let transport = QuicTransport::new(tls_config)?;
    
    println!("  Starting {} server...", backend_name);
    
    // Start server
    let mut incoming = transport.listen(addr).await?;
    
    tokio::spawn(async move {
        while let Ok(_conn) = incoming.accept().await {
            // Just accept connections
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    println!("  Beginning connection test...");
    let start_time = Instant::now();
    
    while start_time.elapsed() < duration {
        let connect_start = Instant::now();
        
        match transport.connect(addr).await {
            Ok(_conn) => {
                let connect_time = connect_start.elapsed();
                metrics.successful_connections += 1;
                metrics.connection_times.push(connect_time);
            }
            Err(_) => {
                metrics.failed_connections += 1;
            }
        }
        
        // Print progress every 30 seconds
        if start_time.elapsed().as_secs() % 30 == 0 && start_time.elapsed().as_secs() > 0 {
            let elapsed = start_time.elapsed().as_secs();
            let remaining = duration.as_secs() - elapsed;
            println!("    Progress: {}s elapsed, {}s remaining, {} connections so far", 
                     elapsed, remaining, metrics.successful_connections);
        }
        
        // Small delay to prevent overwhelming
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    metrics.total_time = start_time.elapsed();
    Ok(metrics)
}