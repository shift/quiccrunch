use quiccrunch_core::{QuicClient, Result};
use quiccrunch_calculator::{CalculatorRequest, CalculatorResponse, Operation};
use std::env;

async fn get_calculator_server() -> String {
    env::var("CALCULATOR_SERVER").unwrap_or_else(|_| "127.0.0.1:50051".to_string())
}

#[tokio::test]
async fn test_calculator_basic_operations() -> Result<()> {
    let server = get_calculator_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Test addition
    let request = CalculatorRequest::new(Operation::Add(5.0, 3.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_ok());
    assert_eq!(response.result.unwrap(), 8.0);
    
    // Test subtraction
    let request = CalculatorRequest::new(Operation::Subtract(10.0, 4.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_ok());
    assert_eq!(response.result.unwrap(), 6.0);
    
    Ok(())
}

#[tokio::test]
async fn test_calculator_error_handling() -> Result<()> {
    let server = get_calculator_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Test division by zero
    let request = CalculatorRequest::new(Operation::Divide(10.0, 0.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_err());
    assert_eq!(response.result.unwrap_err(), "Division by zero");
    
    // Test square root of negative number
    let request = CalculatorRequest::new(Operation::Sqrt(-4.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_err());
    assert!(response.result.unwrap_err().contains("negative number"));
    
    Ok(())
}

#[tokio::test]
async fn test_calculator_concurrent_requests() -> Result<()> {
    let server = get_calculator_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    let mut handles = Vec::new();
    
    // Send 10 concurrent requests
    for i in 0..10 {
        let connection = connection.clone();
        let handle = tokio::spawn(async move {
            let request = CalculatorRequest::new(Operation::Add(i as f64, 1.0));
            let request_bytes = serde_json::to_vec(&request).unwrap();
            let response_bytes = connection.request(request_bytes).await.unwrap();
            let response: CalculatorResponse = serde_json::from_slice(&response_bytes).unwrap();
            (i, response.result.unwrap())
        });
        handles.push(handle);
    }
    
    // Collect results
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    
    // Verify all requests were processed correctly
    results.sort_by_key(|(i, _)| *i);
    for (i, result) in results {
        assert_eq!(result, (i as f64) + 1.0);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_calculator_advanced_operations() -> Result<()> {
    let server = get_calculator_server().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&server).await?;
    
    // Test power operation
    let request = CalculatorRequest::new(Operation::Power(2.0, 3.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_ok());
    assert_eq!(response.result.unwrap(), 8.0);
    
    // Test square root
    let request = CalculatorRequest::new(Operation::Sqrt(16.0));
    let request_bytes = serde_json::to_vec(&request)?;
    let response_bytes = connection.request(request_bytes).await?;
    let response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
    
    assert!(response.result.is_ok());
    assert_eq!(response.result.unwrap(), 4.0);
    
    Ok(())
}