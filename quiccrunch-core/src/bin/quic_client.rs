use quiccrunch_core::transport::adapter::TquicClient;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let args: Vec<String> = std::env::args().collect();
    let addr = if args.len() > 1 {
        args[1].as_str()
    } else {
        "127.0.0.1:4433"
    };

    let message_count = if args.len() > 2 {
        args[2].parse().unwrap_or(5)
    } else {
        5
    };

    let workers = if args.len() > 3 {
        args[3].parse().unwrap_or(2)
    } else {
        2
    };

    log::info!("Starting QUIC client");
    log::info!("Server address: {}", addr);
    log::info!("Messages to send: {}", message_count);
    log::info!("Workers: {}", workers);
    
    let client = TquicClient::new(Some(workers)).await?;
    
    log::info!("Connecting to {}", addr);
    let start = Instant::now();
    let connection = client.connect(addr).await?;
    let connect_duration = start.elapsed();
    
    log::info!("Connected in {:?}: connection_id={:?}", connect_duration, connection.conn_index);
    
    let mut stream = connection.open_stream().await?;
    log::info!("Stream opened: stream_id={}", stream.stream_id);
    
    let messages: Vec<String> = (1..=message_count)
        .map(|i| format!("Message #{}", i))
        .collect();
    
    log::info!("Sending {} messages", messages.len());
    
    let mut total_bytes_sent = 0usize;
    let mut total_bytes_received = 0usize;
    let send_start = Instant::now();
    
    for (i, msg) in messages.iter().enumerate() {
        let msg_bytes = msg.as_bytes();
        total_bytes_sent += msg_bytes.len();
        
        log::debug!("Sending message {}/{}: {} bytes", i + 1, message_count, msg_bytes.len());
        stream.send(msg_bytes, false).await?;
        
        match stream.receive(8192).await {
            Ok(data) if !data.is_empty() => {
                total_bytes_received += data.len();
                log::debug!("Received response {}/{}: {} bytes", i + 1, message_count, data.len());
                log::trace!("Response content: {}", String::from_utf8_lossy(&data));
            }
            Ok(_) => {
                log::warn!("Empty response from server for message {}", i + 1);
            }
            Err(e) => {
                log::error!("Error receiving message {}: {}", i + 1, e);
                break;
            }
        }
        
        // No artificial delay for benchmarking - run at max throughput
    }
    
    let send_duration = send_start.elapsed();
    
    log::debug!("Closing stream");
    stream.send(&[], true).await?;
    stream.close().await?;
    
    log::debug!("Closing connection");
    connection.close().await?;
    
    let throughput_mbps = (total_bytes_sent + total_bytes_received) as f64 
        / send_duration.as_secs_f64() 
        / 1_000_000.0 
        * 8.0;
    
    log::info!("Client completed successfully");
    log::info!("Messages: {}", message_count);
    log::info!("Bytes sent: {}", total_bytes_sent);
    log::info!("Bytes received: {}", total_bytes_received);
    log::info!("Duration: {:?}", send_duration);
    log::info!("Throughput: {:.2} Mbps", throughput_mbps);
    
    Ok(())
}
