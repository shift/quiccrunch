use clap::{Arg, Command, ArgGroup};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use grpc_benchmark::client::BenchmarkClient;
use grpc_benchmark::common::{BenchmarkConfig, PayloadPattern};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let matches = Command::new("grpc-client")
        .version("0.1.0")
        .about("gRPC Benchmark Client for QUICCRUNCH Comparison")
        .arg(
            Arg::new("server")
                .short('s')
                .long("server")
                .value_name("ADDRESS")
                .help("Server address to connect to")
                .default_value("http://127.0.0.1:50051"),
        )
        .arg(
            Arg::new("test-type")
                .short('t')
                .long("test-type")
                .value_name("TYPE")
                .help("Type of test to run")
                .value_parser([
                    "unary", "client-streaming", "server-streaming", 
                    "bidirectional", "latency", "flood", "all"
                ])
                .default_value("all"),
        )
        .arg(
            Arg::new("messages")
                .short('n')
                .long("messages")
                .value_name("COUNT")
                .help("Number of messages to send")
                .default_value("1000"),
        )
        .arg(
            Arg::new("message-size")
                .short('z')
                .long("message-size")
                .value_name("SIZE")
                .help("Size of each message in bytes")
                .default_value("1024"),
        )
        .arg(
            Arg::new("connections")
                .short('c')
                .long("connections")
                .value_name("COUNT")
                .help("Number of concurrent connections")
                .default_value("1"),
        )
        .arg(
            Arg::new("duration")
                .short('d')
                .long("duration")
                .value_name("SECONDS")
                .help("Test duration in seconds")
                .default_value("30"),
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
            Arg::new("repeating-pattern")
                .long("repeating-pattern")
                .value_name("STRING")
                .help("String to repeat for repeating pattern")
                .default_value("ABCDEF"),
        )
        .get_matches();

    let server_address = matches.get_one::<String>("server").unwrap();
    let test_type = matches.get_one::<String>("test-type").unwrap();
    let num_messages: usize = matches.get_one::<String>("messages").unwrap().parse()?;
    let message_size: usize = matches.get_one::<String>("message-size").unwrap().parse()?;
    let concurrent_connections: usize = matches.get_one::<String>("connections").unwrap().parse()?;
    let test_duration_secs: u64 = matches.get_one::<String>("duration").unwrap().parse()?;
    let payload_pattern_str = matches.get_one::<String>("payload-pattern").unwrap();
    let repeating_pattern = matches.get_one::<String>("repeating-pattern").unwrap();

    let payload_pattern = match payload_pattern_str {
        "random" => PayloadPattern::Random,
        "sequential" => PayloadPattern::Sequential,
        "zeroes" => PayloadPattern::Zeroes,
        "repeating" => PayloadPattern::Repeating(repeating_pattern.clone()),
        _ => PayloadPattern::Random,
    };

    let config = BenchmarkConfig {
        server_address: server_address.clone(),
        message_size,
        num_messages,
        concurrent_connections,
        test_duration_secs,
        warmup_duration_secs: 5,
        payload_pattern,
    };

    info!("Starting gRPC benchmark client");
    info!("Server: {}", server_address);
    info!("Test type: {}", test_type);
    info!("Messages: {}", num_messages);
    info!("Message size: {} bytes", message_size);
    info!("Connections: {}", concurrent_connections);

    let mut client = BenchmarkClient::new(config).await?;

    match test_type.as_str() {
        "unary" => client.run_unary_test().await?,
        "client-streaming" => client.run_client_streaming_test().await?,
        "server-streaming" => client.run_server_streaming_test().await?,
        "bidirectional" => client.run_bidirectional_streaming_test().await?,
        "latency" => client.run_latency_test().await?,
        "flood" => client.run_flood_test().await?,
        "all" => {
            info!("Running all test types");
            
            println!("\n🚀 Starting comprehensive gRPC benchmark suite\n");
            
            // Run each test type
            client.run_unary_test().await?;
            client.run_client_streaming_test().await?;
            client.run_server_streaming_test().await?;
            client.run_bidirectional_streaming_test().await?;
            client.run_latency_test().await?;
            client.run_flood_test().await?;
            
            println!("\n✅ All tests completed successfully!");
        }
        _ => {
            error!("Unknown test type: {}", test_type);
            return Err("Unknown test type".into());
        }
    }

    Ok(())
}