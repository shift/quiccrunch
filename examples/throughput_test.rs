use quiccrunch::{QuicTransport, Transport, TlsConfig};
use bytes::Bytes;
use std::time::{Duration, Instant};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("QuicCrunch High-Throughput Test");
    println!("===============================");
    println!("Optimized for maximum message throughput\n");

    // Generate certificates
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;

    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    let test_duration = Duration::from_secs(60); // 1 minute test
    let message_size = 64; // Small messages for max throughput
    let num_concurrent_clients = 16;

    println!("🚀 Starting high-throughput test...");
    println!("   Duration: {}s", test_duration.as_secs());
    println!("   Message size: {} bytes", message_size);
    println!("   Concurrent clients: {}", num_concurrent_clients);

    let results = run_throughput_test(
        tls_config,
        "127.0.0.1:50054",
        test_duration,
        message_size,
        num_concurrent_clients,
    ).await?;

    print_results(&results, test_duration);

    Ok(())
}

struct ThroughputResults {
    total_messages: u64,
    total_bytes: u64,
    min_latency: Duration,
    max_latency: Duration,
    avg_latency: Duration,
}

async fn run_throughput_test(
    tls_config: TlsConfig,
    addr: &str,
    duration: Duration,
    message_size: usize,
    num_clients: usize,
) -> Result<ThroughputResults, Box<dyn Error>> {
    let transport = QuicTransport::new(tls_config)?;
    
    // Shared counters
    let message_counter = Arc::new(AtomicU64::new(0));
    let byte_counter = Arc::new(AtomicU64::new(0));
    let min_latency = Arc::new(AtomicU64::new(u64::MAX));
    let max_latency = Arc::new(AtomicU64::new(0));
    let total_latency = Arc::new(AtomicU64::new(0));
    
    // Start echo server
    let mut incoming = transport.listen(addr).await?;
    tokio::spawn(async move {
        while let Ok(conn) = incoming.accept().await {
            tokio::spawn(async move {
                loop {
                    match conn.accept_bi().await {
                        Ok(mut stream) => {
                            tokio::spawn(async move {
                                while let Ok(msg) = stream.receive_message().await {
                                    let _ = stream.send_message(msg).await;
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("   Server ready, starting load generation...\n");

    // Record actual start time for all clients
    let start_time = Instant::now();
    
    // Spawn client workers
    let mut client_tasks = Vec::new();
    
    for _client_id in 0..num_clients {
        let transport_clone = transport.clone();
        let addr_clone = addr.to_string();
        
        let message_counter_clone = message_counter.clone();
        let byte_counter_clone = byte_counter.clone();
        let min_latency_clone = min_latency.clone();
        let max_latency_clone = max_latency.clone();
        let total_latency_clone = total_latency.clone();
        
        let task = tokio::spawn(async move {
            // Connect to server
            let client = transport_clone.connect(&addr_clone).await?;
            
            let message_data = vec![42u8; message_size];
            let message = Bytes::from(message_data);
            let mut local_message_count = 0u64;
            
            while start_time.elapsed() < duration {
                // Open new stream periodically to test stream creation overhead
                if let Ok(mut stream) = client.open_bi().await {
                    // Send batch of messages on this stream
                    let batch_size = 50;
                    
                    for _ in 0..batch_size {
                        if start_time.elapsed() >= duration {
                            break;
                        }
                        
                        let msg_start = Instant::now();
                        
                        if stream.send_message(message.clone()).await.is_ok() {
                            if let Ok(response) = stream.receive_message().await {
                                let latency = msg_start.elapsed();
                                
                                if response.len() == message.len() {
                                    // Update counters
                                    message_counter_clone.fetch_add(1, Ordering::Relaxed);
                                    byte_counter_clone.fetch_add(message_size as u64, Ordering::Relaxed);
                                    
                                    // Update latency stats
                                    let latency_nanos = latency.as_nanos() as u64;
                                    total_latency_clone.fetch_add(latency_nanos, Ordering::Relaxed);
                                    
                                    // Update min latency
                                    min_latency_clone.fetch_min(latency_nanos, Ordering::Relaxed);
                                    
                                    // Update max latency
                                    max_latency_clone.fetch_max(latency_nanos, Ordering::Relaxed);
                                    
                                    local_message_count += 1;
                                }
                            }
                        }
                    }
                }
            }
            
            Ok::<u64, Box<dyn Error + Send + Sync>>(local_message_count)
        });
        
        client_tasks.push(task);
    }
    
    // Progress reporting
    let progress_counter = message_counter.clone();
    let progress_task = tokio::spawn(async move {
        let mut last_count = 0u64;
        let progress_start = Instant::now();
        
        while progress_start.elapsed() < duration {
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            let current_count = progress_counter.load(Ordering::Relaxed);
            let elapsed = progress_start.elapsed();
            let rate = (current_count - last_count) as f64 / 5.0;
            
            println!("   Progress: {:.1}s, {} messages, {:.0} msg/s", 
                     elapsed.as_secs_f64(), current_count, rate);
            
            last_count = current_count;
        }
    });
    
    // Wait for all clients to complete
    for task in client_tasks {
        let _ = task.await?;
    }
    
    progress_task.abort();
    
    // Collect final results
    let total_messages = message_counter.load(Ordering::Relaxed);
    let total_bytes = byte_counter.load(Ordering::Relaxed);
    let min_lat_nanos = min_latency.load(Ordering::Relaxed);
    let max_lat_nanos = max_latency.load(Ordering::Relaxed);
    let total_lat_nanos = total_latency.load(Ordering::Relaxed);
    
    let avg_latency = if total_messages > 0 {
        Duration::from_nanos(total_lat_nanos / total_messages)
    } else {
        Duration::from_nanos(0)
    };
    
    Ok(ThroughputResults {
        total_messages,
        total_bytes,
        min_latency: Duration::from_nanos(min_lat_nanos),
        max_latency: Duration::from_nanos(max_lat_nanos),
        avg_latency,
    })
}

fn print_results(results: &ThroughputResults, duration: Duration) {
    println!("\n📊 High-Throughput Test Results:");
    println!("================================");
    println!("Test Duration: {:.2}s", duration.as_secs_f64());
    println!("Total Messages: {}", results.total_messages);
    println!("Total Bytes: {} ({:.2} MB)", results.total_bytes, results.total_bytes as f64 / 1_000_000.0);
    println!("");
    
    let msg_per_sec = results.total_messages as f64 / duration.as_secs_f64();
    let mbps = (results.total_bytes as f64 * 8.0) / (duration.as_secs_f64() * 1_000_000.0);
    
    println!("Throughput:");
    println!("  Messages/sec: {:.0}", msg_per_sec);
    println!("  Mbps:         {:.2}", mbps);
    println!("");
    
    println!("Latency:");
    println!("  Min:     {:.3}ms", results.min_latency.as_secs_f64() * 1000.0);
    println!("  Average: {:.3}ms", results.avg_latency.as_secs_f64() * 1000.0);
    println!("  Max:     {:.3}ms", results.max_latency.as_secs_f64() * 1000.0);
    
    if msg_per_sec > 1000.0 {
        println!("\n✅ Successfully achieved >1000 msg/s throughput!");
    } else {
        println!("\n⚠️  Throughput below 1000 msg/s - check for bottlenecks");
    }
}