use quiccrunch::{QuicTransport, Transport, TlsConfig};
use std::error::Error;


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

    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Server
    let mut incoming = transport.listen("127.0.0.1:50051").await?;
    
    // Client
    let client = transport.connect("127.0.0.1:50051").await?;
    let mut stream = client.open_bi().await?;
    
    // Send message
    let message = b"Hello, World!";
    stream.send_message(bytes::Bytes::from(&message[..])).await?;
    
    // Receive on server
    let conn = incoming.accept().await?;
    let mut server_stream = conn.open_bi().await?;
    let received = server_stream.receive_message().await?;
    
    assert_eq!(&received[..], message);
    println!("Message received: {:?}", std::str::from_utf8(&received)?);
    
    Ok(())
}