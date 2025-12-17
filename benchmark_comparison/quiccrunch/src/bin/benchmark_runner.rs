use clap::{Arg, Command};
use tracing_subscriber;
use tracing::{info, error};
use tokio::time::{sleep, Duration};

use quiccrunch_benchmark::{client, common::BenchmarkConfig, common::PayloadPattern};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("quiccrunch=info,quiccrunch_benchmark=debug")
        .init();

    let matches = Command::new("QUICCRUNCH Benchmark Runner")
        .version("0.1.0")
        .about("Run comprehensive QUICCRUNCH benchmark tests")
        .arg(
            Arg::new("address")
                .short('a')
                .long("address")
                .value_name("ADDRESS")
                .help("Server address to connect to")
                .default_value("https://127.0.0.1:8443"),
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
                .help("Number of messages per test")
                .default_value("10000"),
        )
        .arg(
            Arg::new("test-types")
                .short('t')
                .long("test-types")
                .value_name("TYPES")
                .help("Comma-separated list of test types")
                .value_delimiter(',')
                .default_value("unary,client-streaming,server-streaming,bidirectional,latency,flood"),
        )
        .arg(
            Arg::new("delay-between-tests")
                .short('d')
                .long("delay-between-tests")
                .value_name("SECONDS")
                .help("Delay between tests in seconds")
                .default_value("2"),
        )
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap().clone();
    let message_size: usize = matches.get_one::<String>("message-size").unwrap().parse()?;
    let num_messages: usize = matches.get_one::<String>("num-messages").unwrap().parse()?;
    let test_types: Vec<String> = matches.get_many::<String>("test-types")
        .unwrap_or_default()
        .map(|s| s.clone())
        .collect();
    let delay_between_tests: u64 = matches.get_one::<String>("delay-between-tests").unwrap().parse()?;

    info!("Starting QUICCRUNCH benchmark runner");
    info!("Server: {}", address);
    info!("Message size: {} bytes", message_size);
    info!("Messages per test: {}", num_messages);
    info!("Test types: {:?}", test_types);

    for (i, test_type) in test_types.iter().enumerate() {
        if i > 0 {
            info!("Waiting {} seconds before next test...", delay_between_tests);
            sleep(Duration::from_secs(delay_between_tests)).await;
        }

        info!("\n=== Running {} test ===", test_type);

        let config = BenchmarkConfig {
            server_address: address.clone(),
            message_size,
            num_messages,
            concurrent_connections: 50,  // Use connection pooling for better performance
            test_duration_secs: 30,
            warmup_duration_secs: 5,
            payload_pattern: PayloadPattern::Random,
        };

        let mut client = match client::BenchmarkClient::new(config).await {
            Ok(client) => client,
            Err(e) => {
                error!("Failed to create client for {} test: {}", test_type, e);
                continue;
            }
        };

        let result = match test_type.as_str() {
            "unary" => client.run_unary_test().await,
            "client-streaming" => client.run_client_streaming_test().await,
            "server-streaming" => client.run_server_streaming_test().await,
            "bidirectional" => client.run_bidirectional_streaming_test().await,
            "latency" => client.run_latency_test().await,
            "flood" => client.run_flood_test().await,
            _ => {
                error!("Unknown test type: {}", test_type);
                continue;
            }
        };

        if let Err(e) = result {
            error!("Test {} failed: {}", test_type, e);
        } else {
            info!("Test {} completed successfully", test_type);
        }
    }

    info!("All benchmark tests completed");
    Ok(())
}