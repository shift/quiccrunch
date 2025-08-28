use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use std::error::Error;
use bytes::Bytes;
use tokio::sync::mpsc;

use serde::{Serialize, Deserialize};

#[path = "cert_utils.rs"]
mod cert_utils;

// Example service trait
#[async_trait::async_trait]
trait GreetingService {
    async fn greet(&self, request: GreetRequest) -> Result<GreetResponse, Box<dyn Error + Send + Sync>>;
}

// Example messages
#[derive(Debug, Serialize, Deserialize)]
struct GreetRequest {
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GreetResponse {
    message: String,
}

// Service implementation
struct GreetingServiceImpl;

#[async_trait::async_trait]
impl GreetingService for GreetingServiceImpl {
    async fn greet(&self, request: GreetRequest) -> Result<GreetResponse, Box<dyn Error + Send + Sync>> {
        Ok(GreetResponse {
            message: format!("Hello, {}!", request.name),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate TLS certificates
    let (ca_cert, _) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen("127.0.0.1:50051").await?;

    // Run server
    let service = GreetingServiceImpl;
    let (tx, mut rx) = mpsc::channel::<quiccrunch::Connection>(32);
    
    let server_handle = tokio::spawn(async move {
        while let Some(conn) = rx.recv().await {
            let mut stream = conn.open_bi().await?;
            
            loop {
                let msg = match stream.receive_message().await {
                    Ok(msg) => msg,
                    Err(_) => break,
                };
                
                let request: GreetRequest = serde_json::from_slice(&msg)?;
                let response = service.greet(request).await?;
                let response_bytes = serde_json::to_vec(&response)?;
                
                stream.send_message(Bytes::from(response_bytes)).await?;
            }
        }
        Ok::<_, Box<dyn Error + Send + Sync>>(())
    });

    // Accept connections
    tokio::spawn(async move {
        loop {
            if let Ok(conn) = incoming.accept().await {
                let _ = tx.send(conn).await;
            }
        }
    });

    // Client
    let client = transport.connect("127.0.0.1:50051").await?;
    let mut stream = client.open_bi().await?;
    
    let request = GreetRequest {
        name: "QUIC".into(),
    };
    
    let request_bytes = serde_json::to_vec(&request)?;
    stream.send_message(Bytes::from(request_bytes)).await?;
    
    let response_bytes = stream.receive_message().await?;
    let response: GreetResponse = serde_json::from_slice(&response_bytes)?;
    
    println!("Got response: {}", response.message);
    
    // Wait for server to finish
    let _ = server_handle.await;
    
    Ok(())
}