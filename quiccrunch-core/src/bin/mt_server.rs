use quiccrunch_core::transport::adapter::{TquicServer, TquicConnection};
use quiccrunch_core::TransportError;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    
    eprintln!("[mt_server] Starting...");

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

    let worker_count = if args.len() > 4 {
        args[4].parse().unwrap_or(4)
    } else {
        4
    };

    log::info!("Starting Multi-threaded QUIC server (single Endpoint + worker pool)");
    log::info!("Address: {}", addr);
    log::info!("Workers: {}", worker_count);

    let bind_addr: std::net::SocketAddr = addr.parse()?;
    
    // Create the server
    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(1)).await?;
    let mut listener = server.listen(addr).await?;
    
    log::info!("Server listening on {}", addr);

    // Channel for distributing connections to workers
    let (conn_tx, conn_rx) = mpsc::channel::<TquicConnection>(worker_count * 2);
    let conn_rx = Arc::new(tokio::sync::Mutex::new(conn_rx));
    
    // Spawn worker tasks
    let mut worker_handles = Vec::new();
    for worker_id in 0..worker_count {
        let conn_rx = conn_rx.clone();
        let handle = tokio::spawn(async move {
            log::info!("Worker {} started", worker_id);
            loop {
                let conn = {
                    let mut rx = conn_rx.lock().await;
                    match rx.recv().await {
                        Some(conn) => conn,
                        None => {
                            log::info!("Worker {} shutting down (channel closed)", worker_id);
                            break;
                        }
                    }
                };
                
                log::info!("Worker {} handling connection", worker_id);
                
                let mut message_count = 0u64;
                let mut total_bytes = 0usize;
                
                // Handle this connection
                loop {
                    // Accept a stream from the connection
                    let mut stream = match conn.accept_stream().await {
                        Ok(s) => s,
                        Err(e) => {
                            log::error!("Worker {}: accept_stream error: {}", worker_id, e);
                            break;
                        }
                    };
                    
                    log::info!("Worker {}: stream opened (stream_id={})", worker_id, stream.stream_id);
                    
                    // Buffer for accumulating data from the stream
                    let mut recv_buffer = Vec::new();
                    
                    loop {
                        // Receive data from the stream
                        match stream.recv().await {
                            Ok(data) => {
                                if data.is_empty() {
                                    break;
                                }
                                
                                recv_buffer.extend_from_slice(&data);
                                
                                // Process all complete length-prefixed messages in the buffer
                                while recv_buffer.len() >= 4 {
                                    let msg_len = u32::from_be_bytes([
                                        recv_buffer[0], recv_buffer[1],
                                        recv_buffer[2], recv_buffer[3]
                                    ]) as usize;
                                    
                                    if recv_buffer.len() < 4 + msg_len {
                                        break; // Wait for more data
                                    }
                                    
                                    // Extract the message data
                                    let msg_data = &recv_buffer[4..4 + msg_len];
                                    message_count += 1;
                                    total_bytes += msg_len;
                                    
                                    log::trace!("Worker {}: received {} bytes (message #{})",
                                               worker_id, msg_len, message_count);
                                    
                                    // Echo back the same length-prefixed message
                                    let mut response = Vec::with_capacity(4 + msg_len);
                                    response.extend_from_slice(&(msg_len as u32).to_be_bytes());
                                    response.extend_from_slice(msg_data);
                                    
                                    if let Err(e) = stream.send(&response, false).await {
                                        log::error!("Worker {}: send error: {}", worker_id, e);
                                        break;
                                    }
                                    
                                    // Remove processed message from buffer
                                    recv_buffer.drain(..4 + msg_len);
                                }
                            }
                            Err(e) => {
                                log::error!("Worker {}: receive error: {}", worker_id, e);
                                break;
                            }
                        }
                    }
                    
                    // Close the stream
                    if let Err(e) = stream.close().await {
                        log::error!("Worker {}: stream close error: {}", worker_id, e);
                    }
                }
                
                log::info!("Worker {} finished connection ({} messages, {} bytes)", 
                          worker_id, message_count, total_bytes);
            }
        });
        worker_handles.push(handle);
    }
    
    log::info!("Worker pool started with {} workers", worker_count);
    
    log::info!("Server listening on {}", addr);

    let connection_counter = Arc::new(AtomicU64::new(0));

    loop {
        match listener.accept().await {
            Ok(connection) => {
                let conn_id = connection_counter.fetch_add(1, Ordering::Relaxed);
                log::info!("Connection #{} accepted", conn_id);
                
                // Send connection to worker pool
                if let Err(e) = conn_tx.send(connection).await {
                    log::error!("Failed to send connection to worker pool: {}", e);
                }
            }
            Err(e) => {
                log::error!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}