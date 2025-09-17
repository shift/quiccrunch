use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::time::timeout;
use bytes::Bytes;

use quiccrunch_core::{QuicTransport, Transport, QuicClient, QuicServer};
use quiccrunch_core::transport::tquic::TlsConfig;

#[tokio::main]
async fn main() -> Result<(), quiccrunch_core::TransportError> {
    println!("🌐 QuicCrunch Real Network Performance Test");
    println!("==========================================");
    
    let server_addr = "127.0.0.1:59944";
    let test_duration = Duration::from_secs(5);
    let concurrent_clients = 4;
    let message_size = 1024;
    
    println!("Server: {}", server_addr);
    println!("Clients: {}", concurrent_clients);
    println!("Message size: {} bytes", message_size);
    println!("Duration: {:?}", test_duration);
    println!();

    // Start server
    let server_addr_clone = server_addr.to_string();
    let server_handle = tokio::spawn(async move {
        match run_echo_server(&server_addr_clone).await {
            Ok(_) => println!("Server completed"),
            Err(e) => eprintln!("Server error: {:?}", e),
        }
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Run performance test
    let start_time = Instant::now();
    let total_requests = Arc::new(AtomicU64::new(0));
    let total_bytes = Arc::new(AtomicU64::new(0));
    let failed_requests = Arc::new(AtomicU64::new(0));
    
    let mut client_handles = Vec::new();
    
    for client_id in 0..concurrent_clients {
        let server_addr = server_addr.to_string();
        let total_requests = Arc::clone(&total_requests);
        let total_bytes = Arc::clone(&total_bytes);
        let failed_requests = Arc::clone(&failed_requests);
        
        let handle = tokio::spawn(async move {
            match run_client_test(
                client_id,
                &server_addr,
                test_duration,
                message_size,
                total_requests,
                total_bytes,
                failed_requests,
            ).await {
                Ok(_) => {},
                Err(e) => eprintln!("Client {} error: {:?}", client_id, e),
            }
        });
        
        client_handles.push(handle);
    }

    // Wait for all clients to complete
    for handle in client_handles {
        if let Err(e) = handle.await {
            eprintln!("Client task failed: {:?}", e);
        }
    }

    let actual_duration = start_time.elapsed();
    let total_reqs = total_requests.load(Ordering::Relaxed);
    let total_data = total_bytes.load(Ordering::Relaxed);
    let failed_reqs = failed_requests.load(Ordering::Relaxed);
    
    // Calculate metrics
    let requests_per_second = total_reqs as f64 / actual_duration.as_secs_f64();
    let throughput_mbps = (total_data as f64 / actual_duration.as_secs_f64()) / (1024.0 * 1024.0);
    let success_rate = if total_reqs > 0 {
        ((total_reqs - failed_reqs) as f64 / total_reqs as f64) * 100.0
    } else {
        0.0
    };
    
    println!();
    println!("📊 QUIC NETWORK PERFORMANCE RESULTS");
    println!("===================================");
    println!("Duration: {:.2}s", actual_duration.as_secs_f64());
    println!("Total requests: {}", total_reqs);
    println!("Failed requests: {}", failed_reqs);
    println!("Success rate: {:.1}%", success_rate);
    println!("Requests/sec: {:.0} RPS", requests_per_second);
    println!("Throughput: {:.2} MB/s", throughput_mbps);
    println!("Data transferred: {:.2} MB", total_data as f64 / (1024.0 * 1024.0));
    
    // Cleanup
    server_handle.abort();
    
    Ok(())
}

async fn run_echo_server(addr: &str) -> Result<(), quiccrunch_core::TransportError> {
    println!("🖥️  Starting server on {}", addr);
    
    let server = QuicServer::new().await?;
    let mut listener = server.listen(addr).await?;
    
    while let Ok(mut connection) = listener.accept().await {
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_bi().await {
                tokio::spawn(async move {
                    loop {
                        match stream.receive_message().await {
                            Ok(data) => {
                                if stream.send_message(data).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}

async fn run_client_test(
    client_id: usize,
    server_addr: &str,
    duration: Duration,
    message_size: usize,
    total_requests: Arc<AtomicU64>,
    total_bytes: Arc<AtomicU64>,
    failed_requests: Arc<AtomicU64>,
) -> Result<(), quiccrunch_core::TransportError> {
    let client = QuicClient::new().await?;
    let mut connection = client.connect(server_addr).await?;
    
    let payload = Bytes::from(vec![0u8; message_size]);
    let start_time = Instant::now();
    let mut requests = 0u64;
    let mut bytes_sent = 0u64;
    
    while start_time.elapsed() < duration {
        match timeout(Duration::from_millis(1000), async {
            let mut stream = connection.open_bi().await?;
            stream.send_message(payload.clone()).await?;
            let _response = stream.receive_message().await?;
            Ok::<(), quiccrunch_core::TransportError>(())
        }).await {
            Ok(Ok(())) => {
                requests += 1;
                bytes_sent += message_size as u64 * 2; // sent + received
                
                if client_id == 0 && requests % 100 == 0 {
                    print!(".");
                    use std::io::Write;
                    std::io::stdout().flush().ok();
                }
            }
            Ok(Err(_)) | Err(_) => {
                failed_requests.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    
    total_requests.fetch_add(requests, Ordering::Relaxed);
    total_bytes.fetch_add(bytes_sent, Ordering::Relaxed);
    
    println!("\nClient {} completed: {} requests", client_id, requests);
    
    Ok(())
}