use quiccrunch_core::{QuicClient, Result};
use clap::Parser;

#[derive(Parser)]
#[command(name = "echo-client")]
#[command(about = "A simple echo client using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    server: String,
    
    #[arg(short, long)]
    message: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    let client = QuicClient::new()?;
    let connection = client.connect(&args.server).await?;
    
    let message = args.message.unwrap_or_else(|| "Hello from QuicCrunch!".to_string());
    
    let response = connection.request(&message).await?;
    
    println!("Sent: {}", message);
    println!("Received: {}", String::from_utf8_lossy(&response));
    
    Ok(())
}