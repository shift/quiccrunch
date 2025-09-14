use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::*;

mod commands;


mod utils;



#[derive(Parser)]
#[command(name = "quiccrunch")]
#[command(about = "QuicCrunch RPC framework CLI tools")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
    
    /// Suppress all output except errors
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate RPC service code from Cap'n Proto schemas
    Generate {
        /// Input Cap'n Proto schema file
        #[arg(short, long)]
        input: String,
        
        /// Output directory for generated code
        #[arg(short, long, default_value = "src/generated")]
        output: String,
        
        /// Programming language to generate (rust, typescript, go)
        #[arg(short, long, default_value = "rust")]
        language: String,
        
        /// Service name (if not specified, derived from schema)
        #[arg(short, long)]
        service: Option<String>,
    },
    
    /// Test RPC services
    Test {
        /// Service endpoint (e.g., "localhost:8080")
        #[arg(short, long)]
        endpoint: String,
        
        /// Service name to test
        #[arg(short, long)]
        service: String,
        
        /// Method to call
        #[arg(short, long)]
        method: String,
        
        /// JSON parameters for the method
        #[arg(short, long, default_value = "{}")]
        params: String,
        
        /// Number of concurrent requests
        #[arg(short, long, default_value_t = 1)]
        concurrency: usize,
        
        /// Number of total requests
        #[arg(short, long, default_value_t = 1)]
        requests: usize,
    },
    
    /// Call a specific RPC method
    Call {
        /// Service endpoint
        endpoint: String,
        
        /// Service method in format "ServiceName.method_name"
        method: String,
        
        /// JSON parameters
        #[arg(default_value = "{}")]
        params: String,
        
        /// Request timeout in seconds
        #[arg(short, long, default_value_t = 30.0)]
        timeout: f64,
    },
    
    /// Start a development server for testing
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
        
        /// Service implementation file
        #[arg(short, long)]
        service: String,
        
        /// Enable hot reloading
        #[arg(long)]
        watch: bool,
    },
    
    /// Benchmark RPC service performance
    Benchmark {
        /// Service endpoint
        endpoint: String,
        
        /// Service method to benchmark
        method: String,
        
        /// JSON parameters
        #[arg(default_value = "{}")]
        params: String,
        
        /// Duration of benchmark in seconds
        #[arg(short, long, default_value_t = 10)]
        duration: u64,
        
        /// Number of concurrent connections
        #[arg(short, long, default_value_t = 10)]
        connections: usize,
        
        /// Output format (text, json, csv)
        #[arg(short, long, default_value = "text")]
        output: String,
    },
    
    /// Initialize a new QuicCrunch project
    Init {
        /// Project name
        name: String,
        
        /// Project template (basic, full, microservice)
        #[arg(short, long, default_value = "basic")]
        template: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Set up logging based on verbosity
    setup_logging(cli.verbose, cli.quiet);
    
    match cli.command {
        Commands::Generate { input, output, language, service } => {
            commands::handle_generate(input, output, language, service).await
        }
        Commands::Test { endpoint, service, method, params, concurrency, requests } => {
            commands::handle_test(endpoint, service, method, params, concurrency, requests).await
        }
        Commands::Call { endpoint, method, params, timeout } => {
            commands::handle_call(endpoint, method, params, timeout).await
        }
        Commands::Serve { port, service, watch } => {
            commands::handle_serve(port, service, watch).await
        }
        Commands::Benchmark { endpoint, method, params, duration, connections, output } => {
            commands::handle_benchmark(endpoint, method, params, duration, connections, output).await
        }
        Commands::Init { name, template } => {
            commands::handle_init(name, template).await
        }
    }
}

fn setup_logging(verbose: bool, quiet: bool) {
    if quiet {
        // Suppress all output except errors
        return;
    }
    
    if verbose {
        println!("{}", "Verbose mode enabled".dimmed());
    }
    
    // Simple logging setup - in a full implementation, you might use tracing
    std::env::set_var("RUST_LOG", if verbose { "debug" } else { "info" });
}

/// Print a success message
pub fn print_success(message: &str) {
    println!("{} {}", "✓".green().bold(), message);
}

/// Print an error message
pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message);
}

/// Print an info message
pub fn print_info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message);
}

/// Print a warning message
pub fn print_warning(message: &str) {
    println!("{} {}", "⚠".yellow().bold(), message);
}