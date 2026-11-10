//! tonic gRPC echo server (HTTP/2 over TLS on TCP, ALPN h2).

use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:50051")]
    addr: SocketAddr,
    #[arg(long, default_value = "comparison/certs/server.pem")]
    cert: String,
    #[arg(long, default_value = "comparison/certs/server.key")]
    key: String,
    /// Number of tonic/tokio runtime worker threads. Matches `workers` on the
    /// QuicCrunch side (one event loop per worker).
    #[arg(long, default_value_t = 4)]
    workers: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_millis()
        .init();

    let args = Args::parse();
    let cert = std::fs::read(&args.cert)?;
    let key = std::fs::read(&args.key)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.workers.max(1))
        .enable_all()
        .build()?;

    log::warn!(
        "grpc-server: addr={} workers={} tls=rustls(alpn=h2) log_level={}",
        args.addr,
        args.workers,
        log::max_level()
    );

    rt.block_on(async move {
        let tls = comparison::grpc::tls_identity(cert, key)?;
        comparison::grpc::server_builder()
            .tls_config(tls)?
            .add_service(comparison::grpc::make_server())
            .serve(args.addr)
            .await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}
