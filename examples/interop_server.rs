use std::time::Duration;
use clap::Parser;
use tokio::time::timeout;
use quiccrunch::Transport; // bring trait into scope for listen()
use quiccrunch::test_utils::certs; // use shared cert helper

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:0")] // bind ephemeral by default
    addr: String,
    #[arg(long, default_value = "60")] // seconds
    duration: u64,
}

#[tokio::main]
async fn main() -> quiccrunch::Result<()> {
    tracing_subscriber::fmt::try_init().ok();

    let args = Args::parse();

    // Build self-signed certs using examples helper
    let tls_cfg = crate::certs::generate_node_cert("quic-server"); // from examples/certs.rs
    let tls = quiccrunch::transport::TlsConfig { cert: tls_cfg.cert, key: tls_cfg.key, ca_cert: tls_cfg.ca_cert };

    // Use advanced settings; when compiled with backend-tquic or backend-quinn, tests control backend
    let adv = quiccrunch::config::QuicTransportSettings { backend: quiccrunch::config::Backend::Quinn, ..Default::default() };
    let transport = quiccrunch::transport::QuicTransport::with_settings_advanced(tls, adv)?;

    let mut incoming = transport.listen(&args.addr).await?;
    let bound = incoming.local_addr()?;
    println!("BOUND {}", bound);

    let end_at = tokio::time::Instant::now() + Duration::from_secs(args.duration);

    loop {
        let remaining = end_at.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() { break; }
        let conn = match timeout(remaining, incoming.accept()).await {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => { eprintln!("ACCEPT_ERR {e}"); continue; }
            Err(_) => break,
        };
        tokio::spawn(async move {
            // Echo server: accept streams and echo payloads back
            loop {
                let mut stream = match conn.accept_bi().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    loop {
                        match stream.receive_message().await {
                            Ok(buf) => {
                                if let Err(e) = stream.send_message(buf).await { eprintln!("ECHO_SEND_ERR {e}"); break; }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });
    }

    Ok(())
}
