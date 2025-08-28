use quiccrunch::{QuicTransport, Transport, TlsConfig};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate TLS certificates (in production you'd load real certs)
    let (ca_cert, _ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Try a random high port to avoid conflicts
    let random_port = 60000 + (fastrand::u16(..5000));
    let server_addr = format!("127.0.0.1:{}", random_port);
    println!("Attempting to bind to: {}", server_addr);
    
    // Server
    let mut incoming = transport.listen(&server_addr).await?;
    println!("Server listening on {}", server_addr);
    
    // Client
    let client = transport.connect(&server_addr).await?;
    println!("Client connected to {}", server_addr);
    let mut stream = client.open_bi().await?;
    
    // Send message
    let message = b"Hello, World!";
    stream.send_message(bytes::Bytes::from(&message[..])).await?;
    
    // Receive on server
    let conn = incoming.accept().await?;
    let mut server_stream = conn.accept_bi().await?;
    let received = server_stream.receive_message().await?;
    
    assert_eq!(&received[..], message);
    println!("Message received: {:?}", std::str::from_utf8(&received)?);
    
    Ok(())
}