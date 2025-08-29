use quiccrunch_core::{QuicServer, Result};
use clap::Parser;
use tracing::{info, error};

#[derive(Parser)]
#[command(name = "echo-server")]
#[command(about = "A simple echo service using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    address: String,
    
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .init();
    }
    
    info!("Starting echo server on {}", args.address);
    
    let server = QuicServer::new()?;
    let mut listener = server.listen(&args.address).await?;
    
    info!("Echo server listening on {}", listener.local_addr()?);
    println!("Echo server ready! Listening on {}", listener.local_addr()?);
    
    while let Ok(connection) = listener.accept().await {
        info!("New connection accepted");
        
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        let echo_message = format!("Echo: {}", String::from_utf8_lossy(&message));
                        
                        if let Err(e) = stream.send_message(echo_message.into()).await {
                            error!("Failed to send echo response: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}