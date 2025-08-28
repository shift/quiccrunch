use quiccrunch_core::{QuicTransport, Transport, TlsConfig};
use std::error::Error;

#[path = "cert_utils.rs"]
mod cert_utils;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate TLS certificates
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
    println!("Attempting to listen on: {}", server_addr);
    
    // Server ONLY - no client connection
    let _incoming = transport.listen(&server_addr).await?;
    println!("✅ Server successfully listening on {}", server_addr);
    
    Ok(())
}