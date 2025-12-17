use clap::{Arg, Command};
use tracing_subscriber;
use tracing::{info, error};

use quiccrunch_benchmark::server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("quiccrunch=info,quiccrunch_benchmark=debug")
        .init();

    let matches = Command::new("QUICCRUNCH Benchmark Server")
        .version("0.1.0")
        .about("QUICCRUNCH benchmark server for performance comparison with gRPC")
        .arg(
            Arg::new("address")
                .short('a')
                .long("address")
                .value_name("ADDRESS")
                .help("Server address to bind to")
                .default_value("https://127.0.0.1:8443"),
        )
        .arg(
            Arg::new("payload-size")
                .short('p')
                .long("payload-size")
                .value_name("SIZE")
                .help("Response payload size in bytes")
                .default_value("1024"),
        )
        .arg(
            Arg::new("processing-delay")
                .short('d')
                .long("processing-delay")
                .value_name("MICROS")
                .help("Processing delay in microseconds")
                .default_value("100"),
        )
        .arg(
            Arg::new("cpu-work")
                .short('c')
                .long("cpu-work")
                .value_name("INTENSITY")
                .help("CPU work intensity (iterations)")
                .default_value("1000"),
        )
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap().clone();
    let payload_size: usize = matches.get_one::<String>("payload-size").unwrap().parse()?;
    let processing_delay_micros: u64 = matches.get_one::<String>("processing-delay").unwrap().parse()?;
    let cpu_work_intensity: u32 = matches.get_one::<String>("cpu-work").unwrap().parse()?;

    info!("Starting QUICCRUNCH benchmark server");
    info!("Address: {}", address);
    info!("Payload size: {} bytes", payload_size);
    info!("Processing delay: {} μs", processing_delay_micros);
    info!("CPU work intensity: {}", cpu_work_intensity);

    if let Err(e) = server::run_server(address).await {
        error!("Server failed: {}", e);
        std::process::exit(1);
    }

    Ok(())
}