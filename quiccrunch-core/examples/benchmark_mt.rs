//! Multi-threaded benchmark using async_bridge transport with worker_pool

use quiccrunch_core::transport::async_bridge::{AsyncTquicClient, AsyncTquicServer};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} [server|client] [port] [message_count] [message_size] [concurrency]",
            args[0]
        );
        std::process::exit(1);
    }

    match args[1].as_str() {
        "server" => run_server(args).await?,
        "client" => run_client(args).await?,
        _ => {
            eprintln!("Invalid mode. Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn run_server(args: Vec<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = args.get(2).unwrap_or(&"5433".to_string()).parse::<u16>()?;
    let addr = format!("127.0.0.1:{}", port);
    let bind_addr = addr.parse()?;
    let cert_file = "quiccrunch-core/tests/certs/cert.pem".to_string();
    let key_file = "quiccrunch-core/tests/certs/key.pem".to_string();

    log::info!("Starting multi-threaded async_bridge server on {}", addr);

    let server = AsyncTquicServer::new(bind_addr, cert_file, key_file).await?;
    let mut listener = server.listen().await?;

    log::info!("Server listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok(stream) => {
                log::debug!("Connection accepted");
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream).await {
                        log::error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                log::error!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_connection(
    mut stream: quiccrunch_core::QuicStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut message_count = 0;
    let mut total_bytes = 0;

    loop {
        match stream.recv().await {
            Ok(data) => {
                if data.is_empty() {
                    break;
                }
                message_count += 1;
                total_bytes += data.len();

                let response = format!("Echo: {}", String::from_utf8_lossy(&data));
                stream.send(response.as_bytes()).await?;
            }
            Err(e) => {
                log::error!("Stream receive error: {}", e);
                break;
            }
        }
    }

    log::debug!(
        "Processed {} messages, {} bytes total",
        message_count,
        total_bytes
    );
    stream.close().await?;
    Ok(())
}

async fn run_client(args: Vec<String>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = args.get(2).unwrap_or(&"5433".to_string()).parse::<u16>()?;
    let message_count = args
        .get(3)
        .unwrap_or(&"1000".to_string())
        .parse::<usize>()?;
    let message_size = args.get(4).unwrap_or(&"64".to_string()).parse::<usize>()?;
    let concurrency = args.get(5).unwrap_or(&"1".to_string()).parse::<usize>()?;

    let ca_cert = "quiccrunch-core/tests/certs/cert.pem".to_string();

    let addr = format!("127.0.0.1:{}", port);
    let test_message = "x".repeat(message_size);

    log::info!("Starting multi-threaded async_bridge client");
    log::info!(
        "Server: {}, Messages: {}, Size: {}, Concurrency: {}",
        addr,
        message_count,
        message_size,
        concurrency
    );

    let barrier = Arc::new(Barrier::new(concurrency));
    let start_time = Instant::now();

    let mut handles = Vec::new();

    for i in 0..concurrency {
        let client = AsyncTquicClient::new(Some(ca_cert.clone())).await?;
        let barrier = barrier.clone();
        let addr = addr.clone();
        let test_message = test_message.clone();

        let handle = tokio::spawn(async move {
            barrier.wait().await;

            let mut stream = client.connect(&addr).await?;
            log::debug!("Client {} connected", i);

            let mut sent = 0;
            let mut received = 0;

            for _ in 0..message_count {
                stream.send(test_message.as_bytes()).await?;
                sent += message_size;

                match stream.recv().await {
                    Ok(data) if !data.is_empty() => {
                        received += data.len();
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Client {} recv error: {}", i, e);
                        break;
                    }
                }
            }

            stream.close().await?;
            log::debug!(
                "Client {} finished: sent={}, received={}",
                i,
                sent,
                received
            );
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        });
        handles.push(handle);
    }

    // Wait for all clients
    for handle in handles {
        handle.await??;
    }

    let duration = start_time.elapsed();
    let total_messages = message_count * concurrency;
    let total_bytes = message_size * total_messages;
    let throughput = total_messages as f64 / duration.as_secs_f64();
    let throughput_mbps = (total_bytes as f64 * 8.0) / duration.as_secs_f64() / 1_000_000.0;

    log::info!("Client completed successfully");
    log::info!("Total messages: {}", total_messages);
    log::info!("Duration: {:?}", duration);
    log::info!(
        "Throughput: {:.2} msg/s ({:.2} Mbps)",
        throughput,
        throughput_mbps
    );
    log::info!(
        "Avg latency: {:.2} ms",
        duration.as_millis() as f64 / total_messages as f64
    );

    Ok(())
}
