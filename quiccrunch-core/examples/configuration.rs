use quiccrunch_core::{QuicTransport, Transport};
use quiccrunch_core::transport::{TlsConfig, TransportSettings};
use std::error::Error;
use std::time::Duration;


#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate TLS certificates (in production you'd load real certs)
    let (ca_cert, _) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Configure transport parameters
    let settings = TransportSettings {
        max_idle_timeout_ms: Some(30_000),
        initial_rtt_ms: 100,
        max_concurrent_uni_streams: 100,
        ..TransportSettings::default()
    };

    // Create configured transport
    let transport = QuicTransport::with_settings(tls_config, settings)?;
    
    // Use transport as normal...
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    
    // Client
    let client = transport.connect("127.0.0.1:50051").await?;
    let mut stream = client.open_bi().await?;
    
    // Send message
    stream.send_message(bytes::Bytes::from("Hello")).await?;
    
    // Receive on server
    let conn = incoming.accept().await?;
    let mut server_stream = conn.open_bi().await?;
    let received = server_stream.receive_message().await?;
    
    println!("Message received: {:?}", std::str::from_utf8(&received)?);
    
    Ok(())
}