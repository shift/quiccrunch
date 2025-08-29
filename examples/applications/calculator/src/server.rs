use quiccrunch_core::{QuicServer, MessageHandler, Result};
use quiccrunch_calculator::{CalculatorRequest, CalculatorResponse};
use bytes::Bytes;
use clap::Parser;
use tracing::{info, error};

#[derive(Parser)]
#[command(name = "calculator-server")]
#[command(about = "A calculator service using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    address: String,
    
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Clone)]
struct CalculatorHandler;

impl MessageHandler for CalculatorHandler {
    type Response = Bytes;
    
    async fn handle(&self, message: Bytes) -> Bytes {
        match serde_json::from_slice::<CalculatorRequest>(&message) {
            Ok(request) => {
                info!("Processing calculation: {:?}", request.operation);
                let response = request.calculate();
                match serde_json::to_vec(&response) {
                    Ok(bytes) => bytes.into(),
                    Err(e) => {
                        error!("Failed to serialize response: {}", e);
                        format!("{{\"error\":\"Serialization error: {}\"}}", e).into()
                    }
                }
            }
            Err(e) => {
                error!("Failed to parse request: {}", e);
                format!("{{\"error\":\"Invalid request format: {}\"}}", e).into()
            }
        }
    }
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
    
    info!("Starting calculator server on {}", args.address);
    
    let server = QuicServer::new()?;
    let mut listener = server.listen(&args.address).await?;
    
    info!("Calculator server listening on {}", listener.local_addr()?);
    println!("Calculator server ready! Listening on {}", listener.local_addr()?);
    
    while let Ok(connection) = listener.accept().await {
        info!("New connection accepted");
        let handler = CalculatorHandler;
        
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        let response = handler.handle(message).await;
                        if let Err(e) = stream.send_message(response).await {
                            error!("Failed to send response: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}