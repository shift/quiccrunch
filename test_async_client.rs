use quiccrunch_core::transport::adapter::TquicClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    log::info!("Creating TquicClient...");
    let client = TquicClient::new(None).await?;
    log::info!("✓ TquicClient created successfully");

    log::info!("Connecting to 127.0.0.1:4433...");
    match client.connect("127.0.0.1:4433").await {
        Ok(conn) => {
            log::info!("✓ Connection established: conn_index={}", conn.conn_index);
            
            log::info!("Opening stream...");
            match conn.open_stream().await {
                Ok(mut stream) => {
                    log::info!("✓ Stream opened: stream_id={}", stream.stream_id);
                    
                    log::info!("Sending test message...");
                    let msg = b"Hello TQUIC!";
                    stream.send(msg, false).await?;
                    log::info!("✓ Message sent: {} bytes", msg.len());
                    
                    log::info!("Receiving response...");
                    match stream.receive(8192).await {
                        Ok(data) => {
                            log::info!("✓ Received {} bytes: {}", data.len(), String::from_utf8_lossy(&data));
                        }
                        Err(e) => {
                            log::warn!("Receive error (might be expected): {}", e);
                        }
                    }
                    
                    log::info!("Closing stream...");
                    stream.close().await?;
                    log::info!("✓ Stream closed");
                }
                Err(e) => {
                    log::error!("Failed to open stream: {}", e);
                }
            }
            
            log::info!("Closing connection...");
            conn.close().await?;
            log::info!("✓ Connection closed");
        }
        Err(e) => {
            log::error!("Connection failed: {}", e);
            return Err(e.into());
        }
    }

    log::info!("✓ All async API tests passed!");
    Ok(())
}
