use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 Simple Network Performance Test");
    println!("==================================");

    // Start a simple TCP echo server
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    println!("📡 TCP echo server listening on {}", addr);

    // Spawn server
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut socket, _)) => {
                    tokio::spawn(async move {
                        let mut buf = [0; 1024];
                        loop {
                            match socket.read(&mut buf).await {
                                Ok(0) => return, // Connection closed
                                Ok(n) => {
                                    if socket.write_all(&buf[..n]).await.is_err() {
                                        return;
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Give server time to start
    tokio::sleep(Duration::from_millis(100)).await;

    // Run client benchmark
    let successful_requests = Arc::new(AtomicU64::new(0));
    let total_latency_ns = Arc::new(AtomicU64::new(0));

    let start_time = Instant::now();
    let mut handles = Vec::new();

    // Spawn 10 concurrent clients
    for _ in 0..10 {
        let successful_requests = Arc::clone(&successful_requests);
        let total_latency_ns = Arc::clone(&total_latency_ns);
        let addr_string = addr.to_string();

        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let req_start = Instant::now();

                match TcpStream::connect(&addr_string).await {
                    Ok(mut stream) => {
                        let message = b"Hello, World!";
                        if stream.write_all(message).await.is_ok() {
                            let mut buf = [0; 1024];
                            if let Ok(n) = stream.read(&mut buf).await {
                                if n > 0 {
                                    let latency_ns = req_start.elapsed().as_nanos() as u64;
                                    successful_requests.fetch_add(1, Ordering::Relaxed);
                                    total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
                                }
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all clients
    for handle in handles {
        let _ = handle.await;
    }

    let duration = start_time.elapsed();
    let successful = successful_requests.load(Ordering::Relaxed);
    let total_latency = total_latency_ns.load(Ordering::Relaxed);

    let rps = successful as f64 / duration.as_secs_f64();
    let avg_latency_ms = if successful > 0 {
        (total_latency as f64 / successful as f64) / 1_000_000.0
    } else {
        0.0
    };

    println!();
    println!("📊 NETWORK PERFORMANCE RESULTS");
    println!("================================");
    println!("✅ Successful requests: {}", successful);
    println!("⚡ Requests/second: {:.0}", rps);
    println!("⏱️  Average latency: {:.2}ms", avg_latency_ms);
    println!("⏱️  Test duration: {:.2}s", duration.as_secs_f64());
    println!();
    println!("💡 This shows REAL network performance with TCP!");

    Ok(())
}