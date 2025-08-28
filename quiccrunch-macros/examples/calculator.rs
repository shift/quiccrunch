use quiccrunch_macros::rpc_service;

// Example trait that demonstrates the macro
#[rpc_service(name = "Calculator")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
}

// Implementation
#[derive(Clone)]
struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }
    
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a - b)
    }
}

fn main() {
    // Demonstrate that the generated code compiles
    let service = CalculatorImpl;
    let _handler = CalculatorHandler::new(service);
    let _client = CalculatorClient::new();
    
    println!("QuicCrunch procedural macros are working!");
    println!("Generated CalculatorHandler and CalculatorClient successfully.");
}