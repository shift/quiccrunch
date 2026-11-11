use bytes::Bytes;
use quiccrunch_core::transport::async_bridge::AsyncTquicServer;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let addr = if args.len() > 1 {
        args[1].as_str()
    } else {
        "127.0.0.1:4433"
    };

    let cert_file = if args.len() > 2 {
        args[2].clone()
    } else {
        "quiccrunch-core/tests/certs/cert.pem".to_string()
    };

    let key_file = if args.len() > 3 {
        args[3].clone()
    } else {
        "quiccrunch-core/tests/certs/key.pem".to_string()
    };

    log::info!("Starting Async QUIC server");
    log::info!("Address: {}", addr);

    let bind_addr: std::net::SocketAddr = addr.parse()?;
    let server = AsyncTquicServer::new(bind_addr, cert_file, key_file).await?;
    let mut listener = server.listen().await?;

    log::info!("Server listening on {}", addr);

    let connection_counter = Arc::new(AtomicU64::new(0));

    loop {
        match listener.accept().await {
            Ok(mut stream) => {
                let conn_id = connection_counter.fetch_add(1, Ordering::Relaxed);

                log::debug!("Connection #{} accepted", conn_id);

                tokio::spawn(async move {
                    let mut message_count = 0u64;
                    let mut total_bytes = 0usize;

                    loop {
                        match stream.receive_message().await {
                            Ok(data) => {
                                if data.is_empty() {
                                    break;
                                }

                                message_count += 1;
                                total_bytes += data.len();

                                log::trace!(
                                    "Connection #{}: received {} bytes (message #{})",
                                    conn_id,
                                    data.len(),
                                    message_count
                                );

                                let response = format!("Echo: {}", String::from_utf8_lossy(&data))
                                    .into_bytes();
                                if let Err(e) = stream.send_message(Bytes::from(response)).await {
                                    log::error!("Connection #{}: send error: {}", conn_id, e);
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("Connection #{}: receive error: {}", conn_id, e);
                                break;
                            }
                        }
                    }

                    log::debug!(
                        "Connection #{}: processed {} messages, {} bytes total",
                        conn_id,
                        message_count,
                        total_bytes
                    );

                    let _ = stream.close().await;
                });
            }
            Err(e) => {
                log::error!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}
