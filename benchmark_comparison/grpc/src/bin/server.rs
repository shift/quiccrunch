use clap::{Arg, Command};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use grpc_benchmark::server::run_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let matches = Command::new("grpc-server")
        .version("0.1.0")
        .about("gRPC Benchmark Server for QUICCRUNCH Comparison")
        .arg(
            Arg::new("address")
                .short('a')
                .long("address")
                .value_name("ADDRESS")
                .help("Server address to bind to")
                .default_value("127.0.0.1:50051"),
        )
        .arg(
            Arg::new("metrics-port")
                .short('m')
                .long("metrics-port")
                .value_name("PORT")
                .help("Port for Prometheus metrics")
                .default_value("9090"),
        )
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap();
    let metrics_port = matches.get_one::<String>("metrics-port").unwrap();

    info!("Starting gRPC benchmark server");
    info!("Address: {}", address);
    info!("Metrics port: {}", metrics_port);

    if let Err(e) = run_server(format!("http://{}", address)).await {
        error!("Server failed: {}", e);
        return Err(e);
    }

    Ok(())
}