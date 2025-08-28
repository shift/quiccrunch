use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use std::error::Error;


#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate TLS certificates (in production you'd load real certs)
    let (ca_cert, ca_key, ca_cert_obj) = cert_utils::generate_ca_cert()?;
    let (cert, key) = cert_utils::generate_node_cert(&ca_cert_obj, "localhost")?;
    
    let tls_config = TlsConfig {
        cert,
        key,
        ca_cert,
    };

    // Create transport
    let transport = QuicTransport::new(tls_config)?;
    
    // Use different ports for different backends to avoid conflicts
    #[cfg(feature = "backend-quinn")]
    let server_addr = "127.0.0.1:50051";
    #[cfg(feature = "backend-tquic")]
    let server_addr = "127.0.0.1:55555"; // Use a much higher port for tQUIC
    
    // Server
    let mut incoming = transport.listen(server_addr).await?;
    
    // Client
    let client = transport.connect(server_addr).await?;
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