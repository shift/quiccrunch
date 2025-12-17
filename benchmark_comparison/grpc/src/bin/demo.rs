use clap::{Arg, Command};
use tracing::{info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod mock_benchmark;
use mock_benchmark::{MockBenchmarkRunner, BenchmarkConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let matches = Command::new("grpc-benchmark-demo")
        .version("0.1.0")
        .about("gRPC Benchmark Demo for QUICCRUNCH Comparison")
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
            Arg::new("test-type")
                .short('t')
                .long("test-type")
                .value_name("TYPE")
                .help("Type of test to run")
                .value_parser([
                    "unary", "client-streaming", "server-streaming", 
                    "bidirectional", "all"
                ])
                .default_value("all"),
        )
        .get_matches();

    let test_type = matches.get_one::<String>("test-type").unwrap();
    let num_messages: usize = matches.get_one::<String>("messages").unwrap().parse()?;
    let message_size: usize = matches.get_one::<String>("message-size").unwrap().parse()?;

    let config = BenchmarkConfig {
        message_size,
        num_messages,
        ..Default::default()
    };

    info!("Starting gRPC benchmark demo");
    info!("Test type: {}", test_type);
    info!("Messages: {}", num_messages);
    info!("Message size: {} bytes", message_size);

    let runner = MockBenchmarkRunner::new(config);

    match test_type.as_str() {
        "unary" => {
            let metrics = runner.run_unary_test().await;
            runner.print_metrics(&metrics);
        }
        "client-streaming" => {
            let metrics = runner.run_client_streaming_test().await;
            runner.print_metrics(&metrics);
        }
        "server-streaming" => {
            let metrics = runner.run_server_streaming_test().await;
            runner.print_metrics(&metrics);
        }
        "bidirectional" => {
            let metrics = runner.run_bidirectional_streaming_test().await;
            runner.print_metrics(&metrics);
        }
        "all" => {
            info!("Running all test types");
            
            println!("\n🚀 Starting comprehensive gRPC benchmark demo\n");
            
            // Run each test type
            let metrics1 = runner.run_unary_test().await;
            runner.print_metrics(&metrics1);
            
            let metrics2 = runner.run_client_streaming_test().await;
            runner.print_metrics(&metrics2);
            
            let metrics3 = runner.run_server_streaming_test().await;
            runner.print_metrics(&metrics3);
            
            let metrics4 = runner.run_bidirectional_streaming_test().await;
            runner.print_metrics(&metrics4);
            
            println!("\n✅ All tests completed successfully!");
            
            // Print summary comparison
            println!("\n📊 Performance Comparison Summary");
            println!("=================================");
            println!("Test Type              | Latency (μs) | Throughput (msg/s)");
            println!("-----------------------|---------------|------------------");
            println!("Unary                  | {:11.0} | {:14.0}", metrics1.latency_stats.mean_us, metrics1.throughput_stats.messages_per_second);
            println!("Client Streaming        | {:11.0} | {:14.0}", metrics2.latency_stats.mean_us, metrics2.throughput_stats.messages_per_second);
            println!("Server Streaming        | {:11.0} | {:14.0}", metrics3.latency_stats.mean_us, metrics3.throughput_stats.messages_per_second);
            println!("Bidirectional           | {:11.0} | {:14.0}", metrics4.latency_stats.mean_us, metrics4.throughput_stats.messages_per_second);
        }
        _ => {
            eprintln!("Unknown test type: {}", test_type);
            return Err("Unknown test type".into());
        }
    }

    Ok(())
}