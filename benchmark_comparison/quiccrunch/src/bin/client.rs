use clap::{Arg, Command};
use tracing_subscriber;
use tracing::{info, error};

use quiccrunch_benchmark::{client, common::BenchmarkConfig, common::PayloadPattern};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("quiccrunch=info,quiccrunch_benchmark=debug")
        .init();

    let matches = Command::new("QUICCRUNCH Benchmark Client")
        .version("0.1.0")
        .about("QUICCRUNCH benchmark client for performance comparison with gRPC")
        .arg(
            Arg::new("address")
                .short('a')
                .long("address")
                .value_name("ADDRESS")
                .help("Server address to connect to")
                .default_value("https://127.0.0.1:8443"),
        )
        .arg(
            Arg::new("test-type")
                .short('t')
                .long("test-type")
                .value_name("TYPE")
                .help("Type of test to run")
                .value_parser(["unary", "client-streaming", "server-streaming", "bidirectional", "latency", "flood"])
                .default_value("unary"),
        )
        .arg(
            Arg::new("message-size")
                .short('s')
                .long("message-size")
                .value_name("SIZE")
                .help("Message size in bytes")
                .default_value("1024"),
        )
        .arg(
            Arg::new("num-messages")
                .short('n')
                .long("num-messages")
                .value_name("COUNT")
                .help("Number of messages to send")
                .default_value("1000"),
        )
        .arg(
            Arg::new("payload-pattern")
                .short('p')
                .long("payload-pattern")
                .value_name("PATTERN")
                .help("Payload generation pattern")
                .value_parser(["random", "sequential", "zeroes", "repeating"])
                .default_value("random"),
        )
        .arg(
            Arg::new("pattern-data")
                .long("pattern-data")
                .value_name("DATA")
                .help("Data for repeating pattern (used with --payload-pattern repeating)")
                .default_value("QUICCRUNCH"),
        )
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap().clone();
    let test_type = matches.get_one::<String>("test-type").unwrap().clone();
    let message_size: usize = matches.get_one::<String>("message-size").unwrap().parse()?;
    let num_messages: usize = matches.get_one::<String>("num-messages").unwrap().parse()?;
    let payload_pattern_str = matches.get_one::<String>("payload-pattern").unwrap();
    let pattern_data = matches.get_one::<String>("pattern-data").unwrap();

    let payload_pattern = match payload_pattern_str.as_str() {
        "random" => PayloadPattern::Random,
        "sequential" => PayloadPattern::Sequential,
        "zeroes" => PayloadPattern::Zeroes,
        "repeating" => PayloadPattern::Repeating(pattern_data.clone()),
        _ => unreachable!(), // clap ensures valid values
    };

    let config = BenchmarkConfig {
        server_address: address,
        message_size,
        num_messages,
        concurrent_connections: 1, // Single connection for client tests
        test_duration_secs: 30,
        warmup_duration_secs: 5,
        payload_pattern,
    };

    info!("Starting QUICCRUNCH benchmark client");
    info!("Server: {}", config.server_address);
    info!("Test type: {}", test_type);
    info!("Message size: {} bytes", config.message_size);
    info!("Number of messages: {}", config.num_messages);
    info!("Payload pattern: {:?}", config.payload_pattern);

    let mut client = client::BenchmarkClient::new(config).await?;

    let result = match test_type.as_str() {
        "unary" => client.run_unary_test().await,
        "client-streaming" => client.run_client_streaming_test().await,
        "server-streaming" => client.run_server_streaming_test().await,
        "bidirectional" => client.run_bidirectional_streaming_test().await,
        "latency" => client.run_latency_test().await,
        "flood" => client.run_flood_test().await,
        _ => {
            error!("Unknown test type: {}", test_type);
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        error!("Test failed: {}", e);
        std::process::exit(1);
    }

    Ok(())
}