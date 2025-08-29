use quiccrunch_core::{QuicClient, Result};
use quiccrunch_calculator::{CalculatorRequest, CalculatorResponse, Operation};
use clap::Parser;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "calculator-client")]
#[command(about = "A calculator client using QuicCrunch RPC")]
struct Args {
    #[arg(short, long, default_value = "127.0.0.1:50051")]
    server: String,
    
    #[arg(short, long)]
    interactive: bool,
    
    #[arg(long)]
    operation: Option<String>,
    
    #[arg(long)]
    a: Option<f64>,
    
    #[arg(long)]
    b: Option<f64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    let client = QuicClient::new()?;
    let connection = client.connect(&args.server).await?;
    
    println!("Connected to calculator server at {}", args.server);
    
    if args.interactive {
        run_interactive_mode(&connection).await?;
    } else if let Some(op_str) = args.operation {
        run_single_operation(&connection, &op_str, args.a, args.b).await?;
    } else {
        println!("Usage: calculator-client --interactive or specify --operation <op> --a <num> [--b <num>]");
        println!("Operations: add, sub, mul, div, pow, sqrt");
    }
    
    Ok(())
}

async fn run_interactive_mode(connection: &quiccrunch_core::Connection) -> Result<()> {
    println!("Calculator Interactive Mode");
    println!("Commands: add A B, sub A B, mul A B, div A B, pow A B, sqrt A, quit");
    
    loop {
        print!("> ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            break;
        }
        
        let input = input.trim();
        if input == "quit" || input == "exit" {
            break;
        }
        
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        
        let operation = match parts[0] {
            "add" if parts.len() == 3 => {
                if let (Ok(a), Ok(b)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    Some(Operation::Add(a, b))
                } else {
                    println!("Invalid numbers");
                    continue;
                }
            }
            "sub" if parts.len() == 3 => {
                if let (Ok(a), Ok(b)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    Some(Operation::Subtract(a, b))
                } else {
                    println!("Invalid numbers");
                    continue;
                }
            }
            "mul" if parts.len() == 3 => {
                if let (Ok(a), Ok(b)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    Some(Operation::Multiply(a, b))
                } else {
                    println!("Invalid numbers");
                    continue;
                }
            }
            "div" if parts.len() == 3 => {
                if let (Ok(a), Ok(b)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    Some(Operation::Divide(a, b))
                } else {
                    println!("Invalid numbers");
                    continue;
                }
            }
            "pow" if parts.len() == 3 => {
                if let (Ok(a), Ok(b)) = (parts[1].parse::<f64>(), parts[2].parse::<f64>()) {
                    Some(Operation::Power(a, b))
                } else {
                    println!("Invalid numbers");
                    continue;
                }
            }
            "sqrt" if parts.len() == 2 => {
                if let Ok(a) = parts[1].parse::<f64>() {
                    Some(Operation::Sqrt(a))
                } else {
                    println!("Invalid number");
                    continue;
                }
            }
            _ => {
                println!("Invalid command. Use: add/sub/mul/div/pow A B, sqrt A, or quit");
                continue;
            }
        };
        
        if let Some(op) = operation {
            if let Err(e) = send_calculation(connection, op).await {
                println!("Error: {}", e);
            }
        }
    }
    
    Ok(())
}

async fn run_single_operation(
    connection: &quiccrunch_core::Connection,
    op_str: &str,
    a: Option<f64>,
    b: Option<f64>,
) -> Result<()> {
    let operation = match op_str {
        "add" => {
            if let (Some(a), Some(b)) = (a, b) {
                Operation::Add(a, b)
            } else {
                println!("Add operation requires --a and --b arguments");
                return Ok(());
            }
        }
        "sub" => {
            if let (Some(a), Some(b)) = (a, b) {
                Operation::Subtract(a, b)
            } else {
                println!("Subtract operation requires --a and --b arguments");
                return Ok(());
            }
        }
        "mul" => {
            if let (Some(a), Some(b)) = (a, b) {
                Operation::Multiply(a, b)
            } else {
                println!("Multiply operation requires --a and --b arguments");
                return Ok(());
            }
        }
        "div" => {
            if let (Some(a), Some(b)) = (a, b) {
                Operation::Divide(a, b)
            } else {
                println!("Divide operation requires --a and --b arguments");
                return Ok(());
            }
        }
        "pow" => {
            if let (Some(a), Some(b)) = (a, b) {
                Operation::Power(a, b)
            } else {
                println!("Power operation requires --a and --b arguments");
                return Ok(());
            }
        }
        "sqrt" => {
            if let Some(a) = a {
                Operation::Sqrt(a)
            } else {
                println!("Square root operation requires --a argument");
                return Ok(());
            }
        }
        _ => {
            println!("Unknown operation: {}. Use: add, sub, mul, div, pow, sqrt", op_str);
            return Ok(());
        }
    };
    
    send_calculation(connection, operation).await
}

async fn send_calculation(
    connection: &quiccrunch_core::Connection,
    operation: Operation,
) -> Result<()> {
    let request = CalculatorRequest::new(operation);
    let request_bytes = serde_json::to_vec(&request)?;
    
    let response_bytes = connection.request(request_bytes).await?;
    
    match serde_json::from_slice::<CalculatorResponse>(&response_bytes) {
        Ok(response) => {
            match response.result {
                Ok(result) => println!("Result: {}", result),
                Err(error) => println!("Calculation error: {}", error),
            }
        }
        Err(e) => {
            println!("Failed to parse response: {}", e);
            println!("Raw response: {}", String::from_utf8_lossy(&response_bytes));
        }
    }
    
    Ok(())
}