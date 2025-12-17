use clap::{Arg, Command};
use tracing_subscriber;
use tracing::{info, error, warn};
use tokio::time::{sleep, Duration};

use quiccrunch_benchmark::{client, common::BenchmarkConfig, common::PayloadPattern};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("quiccrunch=info,quiccrunch_benchmark=debug")
        .init();

    let matches = Command::new("QUICCRUNCH Demo")
        .version("0.1.0")
        .about("Demonstration of QUICCRUNCH benchmark capabilities")
        .arg(
            Arg::new("address")
                .short('a')
                .long("address")
                .value_name("ADDRESS")
                .help("Server address to connect to")
                .default_value("https://127.0.0.1:8443"),
        )
        .arg(
            Arg::new("demo-type")
                .short('d')
                .long("demo-type")
                .value_name("TYPE")
                .help("Type of demo to run")
                .value_parser(["quick", "comprehensive", "stress"])
                .default_value("quick"),
        )
        .get_matches();

    let address = matches.get_one::<String>("address").unwrap().clone();
    let demo_type = matches.get_one::<String>("demo-type").unwrap();

    info!("Starting QUICCRUNCH demo");
    info!("Server: {}", address);
    info!("Demo type: {}", demo_type);

    match demo_type.as_str() {
        "quick" => run_quick_demo(&address).await?,
        "comprehensive" => run_comprehensive_demo(&address).await?,
        "stress" => run_stress_demo(&address).await?,
        _ => unreachable!(), // clap ensures valid values
    }

    Ok(())
}

async fn run_quick_demo(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== Quick Demo ===");
    info!("Running a quick demonstration of all QUICCRUNCH streaming types");

    let test_configs = vec![
        ("Unary Call", 64, 10),
        ("Client Streaming", 256, 50),
        ("Server Streaming", 128, 1),
        ("Bidirectional Streaming", 128, 20),
    ];

    for (test_name, message_size, num_messages) in test_configs {
        info!("\n--- {} ---", test_name);
        info!("Message size: {} bytes, Messages: {}", message_size, num_messages);

        let config = BenchmarkConfig {
            server_address: address.to_string(),
            message_size,
            num_messages,
            concurrent_connections: 1,
            test_duration_secs: 30,
            warmup_duration_secs: 1,
            payload_pattern: PayloadPattern::Repeating("QUICCRUNCH".to_string()),
        };

        let mut client = match client::BenchmarkClient::new(config).await {
            Ok(client) => client,
            Err(e) => {
                warn!("Failed to create client for {}: {}", test_name, e);
                continue;
            }
        };

        let result = match test_name {
            "Unary Call" => client.run_unary_test().await,
            "Client Streaming" => client.run_client_streaming_test().await,
            "Server Streaming" => client.run_server_streaming_test().await,
            "Bidirectional Streaming" => client.run_bidirectional_streaming_test().await,
            _ => continue,
        };

        if let Err(e) = result {
            error!("Demo {} failed: {}", test_name, e);
        } else {
            info!("Demo {} completed successfully", test_name);
        }

        sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

async fn run_comprehensive_demo(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== Comprehensive Demo ===");
    info!("Running comprehensive tests with different payload sizes and patterns");

    let payload_sizes = vec![64, 256, 1024, 4096];
    let payload_patterns = vec![
        ("Random Data", PayloadPattern::Random),
        ("Sequential Data", PayloadPattern::Sequential),
        ("Zeroes", PayloadPattern::Zeroes),
        ("Repeating Pattern", PayloadPattern::Repeating("QUICCRUNCH_BENCHMARK".to_string())),
    ];

    for &size in &payload_sizes {
        for (pattern_name, pattern) in &payload_patterns {
            info!("\n--- Size: {} bytes, Pattern: {} ---", size, pattern_name);

            let config = BenchmarkConfig {
                server_address: address.to_string(),
                message_size: size,
                num_messages: 100,
                concurrent_connections: 1,
                test_duration_secs: 30,
                warmup_duration_secs: 2,
                payload_pattern: pattern.clone(),
            };

            let mut client = match client::BenchmarkClient::new(config).await {
                Ok(client) => client,
                Err(e) => {
                    warn!("Failed to create client: {}", e);
                    continue;
                }
            };

            if let Err(e) = client.run_unary_test().await {
                error!("Unary test failed: {}", e);
            }

            sleep(Duration::from_millis(500)).await;
        }
    }

    Ok(())
}

async fn run_stress_demo(address: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("\n=== Stress Demo ===");
    info!("Running stress tests with high message counts and large payloads");

    let stress_configs = vec![
        ("High Frequency Small Messages", 64, 10000),
        ("Large Payload Messages", 65536, 100),
        ("Medium Load", 1024, 5000),
        ("Bidirectional Stress", 512, 2000),
    ];

    for (test_name, message_size, num_messages) in stress_configs {
        info!("\n--- {} ---", test_name);
        info!("Message size: {} bytes, Messages: {}", message_size, num_messages);

        let config = BenchmarkConfig {
            server_address: address.to_string(),
            message_size,
            num_messages,
            concurrent_connections: 1,
            test_duration_secs: 60,
            warmup_duration_secs: 5,
            payload_pattern: PayloadPattern::Random,
        };

        let mut client = match client::BenchmarkClient::new(config).await {
            Ok(client) => client,
            Err(e) => {
                warn!("Failed to create client for {}: {}", test_name, e);
                continue;
            }
        };

        let result = if test_name.contains("Bidirectional") {
            client.run_bidirectional_streaming_test().await
        } else {
            client.run_unary_test().await
        };

        if let Err(e) = result {
            error!("Stress test {} failed: {}", test_name, e);
        } else {
            info!("Stress test {} completed successfully", test_name);
        }

        sleep(Duration::from_secs(3)).await;
    }

    Ok(())
}