use quiccrunch_core::{QuicClient, Result};
use quiccrunch_calculator::{CalculatorRequest, CalculatorResponse, Operation};
use quiccrunch_chat_service::{ChatRequest, ChatResponse};
use std::env;
use tokio::time::{timeout, Duration};

async fn get_servers() -> (String, String, String) {
    let calculator = env::var("CALCULATOR_SERVER").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    let chat = env::var("CHAT_SERVER").unwrap_or_else(|_| "127.0.0.1:50052".to_string());
    let echo = env::var("ECHO_SERVER").unwrap_or_else(|_| "127.0.0.1:50053".to_string());
    (calculator, chat, echo)
}

#[tokio::test]
async fn test_all_services_running() -> Result<()> {
    let (calculator_server, chat_server, echo_server) = get_servers().await;
    
    // Test that all services are reachable
    let client = QuicClient::new()?;
    
    // Test calculator service
    let calc_connection = timeout(
        Duration::from_secs(10),
        client.connect(&calculator_server)
    ).await.expect("Calculator service should be reachable")?;
    
    // Test chat service
    let chat_connection = timeout(
        Duration::from_secs(10),
        client.connect(&chat_server)
    ).await.expect("Chat service should be reachable")?;
    
    // Test echo service
    let echo_connection = timeout(
        Duration::from_secs(10),
        client.connect(&echo_server)
    ).await.expect("Echo service should be reachable")?;
    
    // Verify they respond correctly
    
    // Calculator test
    let calc_request = CalculatorRequest::new(Operation::Add(2.0, 3.0));
    let calc_request_bytes = serde_json::to_vec(&calc_request)?;
    let calc_response_bytes = calc_connection.request(calc_request_bytes).await?;
    let calc_response: CalculatorResponse = serde_json::from_slice(&calc_response_bytes)?;
    assert_eq!(calc_response.result.unwrap(), 5.0);
    
    // Chat test
    let chat_request = ChatRequest::ListRooms;
    let chat_request_bytes = serde_json::to_vec(&chat_request)?;
    let chat_response_bytes = chat_connection.request(chat_request_bytes).await?;
    let _chat_response: ChatResponse = serde_json::from_slice(&chat_response_bytes)?;
    
    // Echo test
    let echo_response = echo_connection.request("test message").await?;
    assert!(String::from_utf8_lossy(&echo_response).contains("test message"));
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_multi_service_usage() -> Result<()> {
    let (calculator_server, chat_server, echo_server) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Create connections to all services
    let calc_connection = client.connect(&calculator_server).await?;
    let chat_connection = client.connect(&chat_server).await?;
    let echo_connection = client.connect(&echo_server).await?;
    
    let mut handles = Vec::new();
    
    // Start concurrent operations on all services
    for i in 0..10 {
        // Calculator operations
        let calc_conn = calc_connection.clone();
        let calc_handle = tokio::spawn(async move {
            let request = CalculatorRequest::new(Operation::Multiply(i as f64, 2.0));
            let request_bytes = serde_json::to_vec(&request).unwrap();
            let response_bytes = calc_conn.request(request_bytes).await.unwrap();
            let response: CalculatorResponse = serde_json::from_slice(&response_bytes).unwrap();
            ("calc", i, response.result.unwrap())
        });
        handles.push(calc_handle);
        
        // Chat operations
        let chat_conn = chat_connection.clone();
        let chat_handle = tokio::spawn(async move {
            // Join room
            let request = ChatRequest::Join {
                user: format!("user_{}", i),
                room: "integration_test".to_string(),
            };
            let request_bytes = serde_json::to_vec(&request).unwrap();
            let _ = chat_conn.request(request_bytes).await.unwrap();
            
            // Send message
            let request = ChatRequest::SendMessage {
                user: format!("user_{}", i),
                room: "integration_test".to_string(),
                content: format!("Message from user {}", i),
            };
            let request_bytes = serde_json::to_vec(&request).unwrap();
            let response_bytes = chat_conn.request(request_bytes).await.unwrap();
            let response: ChatResponse = serde_json::from_slice(&response_bytes).unwrap();
            
            match response {
                ChatResponse::MessageReceived { message } => ("chat", i, message.content.len() as f64),
                _ => ("chat", i, 0.0),
            }
        });
        handles.push(chat_handle);
        
        // Echo operations
        let echo_conn = echo_connection.clone();
        let echo_handle = tokio::spawn(async move {
            let message = format!("Echo test message {}", i);
            let response = echo_conn.request(&message).await.unwrap();
            let response_str = String::from_utf8_lossy(&response);
            ("echo", i, if response_str.contains(&i.to_string()) { 1.0 } else { 0.0 })
        });
        handles.push(echo_handle);
    }
    
    // Collect all results
    let mut calc_results = Vec::new();
    let mut chat_results = Vec::new();
    let mut echo_results = Vec::new();
    
    for handle in handles {
        let (service, i, result) = handle.await.unwrap();
        match service {
            "calc" => calc_results.push((i, result)),
            "chat" => chat_results.push((i, result)),
            "echo" => echo_results.push((i, result)),
            _ => unreachable!(),
        }
    }
    
    // Verify results
    calc_results.sort_by_key(|(i, _)| *i);
    for (i, result) in calc_results {
        assert_eq!(result, (i as f64) * 2.0);
    }
    
    chat_results.sort_by_key(|(i, _)| *i);
    for (_, result) in chat_results {
        assert!(result > 0.0); // Message should have content
    }
    
    echo_results.sort_by_key(|(i, _)| *i);
    for (_, result) in echo_results {
        assert_eq!(result, 1.0); // Echo should contain the index
    }
    
    Ok(())
}

#[tokio::test]
async fn test_service_isolation() -> Result<()> {
    let (calculator_server, chat_server, _) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Connect to both services
    let calc_connection = client.connect(&calculator_server).await?;
    let chat_connection = client.connect(&chat_server).await?;
    
    // Create a scenario where one service might be under load
    let mut calc_handles = Vec::new();
    for i in 0..50 {
        let calc_conn = calc_connection.clone();
        let handle = tokio::spawn(async move {
            // Simulate some computation load
            let request = CalculatorRequest::new(Operation::Power(i as f64, 3.0));
            let request_bytes = serde_json::to_vec(&request).unwrap();
            let response_bytes = calc_conn.request(request_bytes).await.unwrap();
            let response: CalculatorResponse = serde_json::from_slice(&response_bytes).unwrap();
            response.result.unwrap()
        });
        calc_handles.push(handle);
    }
    
    // While calculator is under load, test that chat service is still responsive
    let chat_start = std::time::Instant::now();
    let chat_request = ChatRequest::Join {
        user: "isolation_test_user".to_string(),
        room: "isolation_test_room".to_string(),
    };
    let chat_request_bytes = serde_json::to_vec(&chat_request)?;
    let chat_response_bytes = chat_connection.request(chat_request_bytes).await?;
    let _chat_response: ChatResponse = serde_json::from_slice(&chat_response_bytes)?;
    let chat_duration = chat_start.elapsed();
    
    // Chat should be responsive (< 1 second) even while calculator is busy
    assert!(chat_duration < Duration::from_secs(1));
    
    // Wait for calculator operations to complete
    for handle in calc_handles {
        let _ = handle.await.unwrap();
    }
    
    Ok(())
}

#[tokio::test]
async fn test_cross_service_workflow() -> Result<()> {
    let (calculator_server, chat_server, _) = get_servers().await;
    let client = QuicClient::new()?;
    
    let calc_connection = client.connect(&calculator_server).await?;
    let chat_connection = client.connect(&chat_server).await?;
    
    // Simulate a workflow that uses multiple services:
    // 1. Calculate something
    // 2. Share the result in chat
    
    // Step 1: Perform calculation
    let calc_request = CalculatorRequest::new(Operation::Add(123.0, 456.0));
    let calc_request_bytes = serde_json::to_vec(&calc_request)?;
    let calc_response_bytes = calc_connection.request(calc_request_bytes).await?;
    let calc_response: CalculatorResponse = serde_json::from_slice(&calc_response_bytes)?;
    let result = calc_response.result.unwrap();
    
    // Step 2: Join a chat room
    let chat_request = ChatRequest::Join {
        user: "workflow_user".to_string(),
        room: "math_results".to_string(),
    };
    let chat_request_bytes = serde_json::to_vec(&chat_request)?;
    let _ = chat_connection.request(chat_request_bytes).await?;
    
    // Step 3: Share the calculation result
    let chat_request = ChatRequest::SendMessage {
        user: "workflow_user".to_string(),
        room: "math_results".to_string(),
        content: format!("Calculation result: 123 + 456 = {}", result),
    };
    let chat_request_bytes = serde_json::to_vec(&chat_request)?;
    let chat_response_bytes = chat_connection.request(chat_request_bytes).await?;
    let chat_response: ChatResponse = serde_json::from_slice(&chat_response_bytes)?;
    
    // Verify the workflow completed successfully
    match chat_response {
        ChatResponse::MessageReceived { message } => {
            assert!(message.content.contains("579")); // 123 + 456 = 579
            assert_eq!(message.user, "workflow_user");
            assert_eq!(message.room, "math_results");
        },
        _ => panic!("Expected message received response"),
    }
    
    Ok(())
}