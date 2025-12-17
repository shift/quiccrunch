use quiccrunch_core::transport::adapter::{TquicServer, TquicConnection};
use quiccrunch_core::TransportError;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let addr = if args.len() > 1 {
        args[1].as_str()
    } else {
        "127.0.0.1:4433"
    };

    let workers = if args.len() > 2 {
        args[2].parse().unwrap_or(4)
    } else {
        4
    };
    
    let cert_file = if args.len() > 3 {
        args[3].clone()
    } else {
        "quiccrunch-core/tests/certs/cert.pem".to_string()
    };
    
    let key_file = if args.len() > 4 {
        args[4].clone()
    } else {
        "quiccrunch-core/tests/certs/key.pem".to_string()
    };

    log::info!("Starting QUIC server");
    log::info!("Address: {}", addr);
    log::info!("Workers: {}", workers);

    let bind_addr: std::net::SocketAddr = addr.parse()?;
    let server = TquicServer::new(bind_addr, cert_file, key_file, Some(workers)).await?;
    let mut listener = server.listen(addr).await?;
    
    log::info!("Server listening on {}", addr);

    let connection_counter = Arc::new(AtomicU64::new(0));
    let active_connections = Arc::new(AtomicU64::new(0));

    loop {
        match listener.accept().await {
            Ok(connection) => {
                let conn_id = connection_counter.fetch_add(1, Ordering::Relaxed);
                active_connections.fetch_add(1, Ordering::Relaxed);
                
                log::debug!("Connection #{} accepted: {:?}", conn_id, connection.conn_index);
                
                let active_conns = Arc::clone(&active_connections);
                
                tokio::spawn(async move {
                    match handle_connection(connection, conn_id).await {
                        Ok(_) => {
                            log::debug!("Connection #{} completed successfully", conn_id);
                        }
                        Err(e) => {
                            log::error!("Connection #{} error: {}", conn_id, e);
                        }
                    }
                    active_conns.fetch_sub(1, Ordering::Relaxed);
                });
            }
            Err(e) => {
                log::error!("Accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_connection(connection: TquicConnection, conn_id: u64) -> Result<(), TransportError> {
    let mut stream = connection.accept_stream().await?;
    log::debug!("Connection #{}: stream {} opened", conn_id, stream.stream_id);
    
    let mut message_count = 0u64;
    let mut total_bytes = 0usize;
    
    loop {
        let data = stream.receive(8192).await?;
        
        if data.is_empty() {
            break;
        }
        
        message_count += 1;
        total_bytes += data.len();
        
        log::trace!("Connection #{}: received {} bytes (message #{})", 
                   conn_id, data.len(), message_count);
        
        let response = format!("Echo: {}", String::from_utf8_lossy(&data));
        stream.send(response.as_bytes(), false).await?;
    }
    
    log::debug!("Connection #{}: processed {} messages, {} bytes total", 
               conn_id, message_count, total_bytes);
    
    stream.close().await?;
    Ok(())
}
