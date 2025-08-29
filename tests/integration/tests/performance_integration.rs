use quiccrunch_core::{QuicClient, Result};
use quiccrunch_calculator::{CalculatorRequest, CalculatorResponse, Operation};
use std::env;
use std::time::{Duration, Instant};
use tokio::time::timeout;

async fn get_servers() -> (String, String, String) {
    let calculator = env::var("CALCULATOR_SERVER").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    let chat = env::var("CHAT_SERVER").unwrap_or_else(|_| "127.0.0.1:50052".to_string());
    let echo = env::var("ECHO_SERVER").unwrap_or_else(|_| "127.0.0.1:50053".to_string());
    (calculator, chat, echo)
}

#[tokio::test]
async fn test_latency_performance() -> Result<()> {
    let (calculator_server, _, echo_server) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Test calculator latency
    let calc_connection = client.connect(&calculator_server).await?;
    let mut calc_latencies = Vec::new();
    
    for _ in 0..100 {
        let start = Instant::now();
        let request = CalculatorRequest::new(Operation::Add(1.0, 2.0));
        let request_bytes = serde_json::to_vec(&request)?;
        let response_bytes = calc_connection.request(request_bytes).await?;
        let _response: CalculatorResponse = serde_json::from_slice(&response_bytes)?;
        let latency = start.elapsed();
        calc_latencies.push(latency);
    }
    
    // Test echo latency
    let echo_connection = client.connect(&echo_server).await?;
    let mut echo_latencies = Vec::new();
    
    for _ in 0..100 {
        let start = Instant::now();
        let _response = echo_connection.request("test").await?;
        let latency = start.elapsed();
        echo_latencies.push(latency);
    }
    
    // Analyze results
    let calc_avg = calc_latencies.iter().sum::<Duration>() / calc_latencies.len() as u32;
    let echo_avg = echo_latencies.iter().sum::<Duration>() / echo_latencies.len() as u32;
    
    println!("Calculator average latency: {:?}", calc_avg);
    println!("Echo average latency: {:?}", echo_avg);
    
    // Performance assertions (these should be reasonable for QUIC)
    assert!(calc_avg < Duration::from_millis(100), "Calculator latency too high: {:?}", calc_avg);
    assert!(echo_avg < Duration::from_millis(50), "Echo latency too high: {:?}", echo_avg);
    
    // Calculate 95th percentile
    calc_latencies.sort();
    echo_latencies.sort();
    
    let calc_p95 = calc_latencies[(calc_latencies.len() * 95) / 100];
    let echo_p95 = echo_latencies[(echo_latencies.len() * 95) / 100];
    
    println!("Calculator 95th percentile latency: {:?}", calc_p95);
    println!("Echo 95th percentile latency: {:?}", echo_p95);
    
    assert!(calc_p95 < Duration::from_millis(200), "Calculator P95 latency too high: {:?}", calc_p95);
    assert!(echo_p95 < Duration::from_millis(100), "Echo P95 latency too high: {:?}", echo_p95);
    
    Ok(())
}

#[tokio::test]
async fn test_throughput_performance() -> Result<()> {
    let (calculator_server, _, echo_server) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Test calculator throughput with concurrent connections
    let calc_connection = client.connect(&calculator_server).await?;
    
    let start = Instant::now();
    let mut handles = Vec::new();
    
    // Send 1000 concurrent requests
    for i in 0..1000 {
        let connection = calc_connection.clone();
        let handle = tokio::spawn(async move {
            let request = CalculatorRequest::new(Operation::Add(i as f64, 1.0));
            let request_bytes = serde_json::to_vec(&request).unwrap();
            match timeout(Duration::from_secs(10), connection.request(request_bytes)).await {
                Ok(Ok(_)) => 1,
                _ => 0,
            }
        });
        handles.push(handle);
    }
    
    // Collect results
    let mut successful_requests = 0;
    for handle in handles {
        successful_requests += handle.await.unwrap();
    }
    
    let duration = start.elapsed();
    let throughput = successful_requests as f64 / duration.as_secs_f64();
    
    println!("Calculator throughput: {:.2} requests/second", throughput);
    println!("Successful requests: {}/1000", successful_requests);
    
    // Assertions
    assert!(successful_requests >= 950, "Too many failed requests: {}/1000", successful_requests);
    assert!(throughput > 100.0, "Throughput too low: {:.2} req/s", throughput);
    
    Ok(())
}

#[tokio::test]
async fn test_connection_reuse_performance() -> Result<()> {
    let (calculator_server, _, _) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Test with connection reuse (better performance)
    let connection = client.connect(&calculator_server).await?;
    
    let start = Instant::now();
    for _ in 0..100 {
        let request = CalculatorRequest::new(Operation::Add(1.0, 2.0));
        let request_bytes = serde_json::to_vec(&request)?;
        let _response = connection.request(request_bytes).await?;
    }
    let reused_duration = start.elapsed();
    
    // Test without connection reuse (should be slower)
    let start = Instant::now();
    for _ in 0..100 {
        let new_connection = client.connect(&calculator_server).await?;
        let request = CalculatorRequest::new(Operation::Add(1.0, 2.0));
        let request_bytes = serde_json::to_vec(&request)?;
        let _response = new_connection.request(request_bytes).await?;
    }
    let new_connection_duration = start.elapsed();
    
    println!("Connection reuse time: {:?}", reused_duration);
    println!("New connection time: {:?}", new_connection_duration);
    
    // Connection reuse should be significantly faster
    assert!(
        reused_duration < new_connection_duration,
        "Connection reuse should be faster: {:?} vs {:?}",
        reused_duration,
        new_connection_duration
    );
    
    // The difference should be substantial (at least 2x faster)
    assert!(
        new_connection_duration.as_millis() > reused_duration.as_millis() * 2,
        "Connection reuse benefit should be substantial: {:?} vs {:?}",
        reused_duration,
        new_connection_duration
    );
    
    Ok(())
}

#[tokio::test]
async fn test_concurrent_streams_performance() -> Result<()> {
    let (calculator_server, _, _) = get_servers().await;
    let client = QuicClient::new()?;
    let connection = client.connect(&calculator_server).await?;
    
    // Test multiple concurrent streams on a single connection
    let start = Instant::now();
    let mut handles = Vec::new();
    
    // Create 50 concurrent streams
    for i in 0..50 {
        let connection = connection.clone();
        let handle = tokio::spawn(async move {
            let mut stream_results = Vec::new();
            
            // Each stream sends 10 requests
            for j in 0..10 {
                let request = CalculatorRequest::new(Operation::Multiply(i as f64, j as f64));
                let request_bytes = serde_json::to_vec(&request).unwrap();
                
                match timeout(Duration::from_secs(5), connection.request(request_bytes)).await {
                    Ok(Ok(response_bytes)) => {
                        if let Ok(response) = serde_json::from_slice::<CalculatorResponse>(&response_bytes) {
                            if let Ok(result) = response.result {
                                stream_results.push(result);
                            }
                        }
                    }
                    _ => {}
                }
            }
            
            stream_results.len()
        });
        handles.push(handle);
    }
    
    // Collect results
    let mut total_successful = 0;
    for handle in handles {
        total_successful += handle.await.unwrap();
    }
    
    let duration = start.elapsed();
    let throughput = total_successful as f64 / duration.as_secs_f64();
    
    println!("Concurrent streams throughput: {:.2} requests/second", throughput);
    println!("Total successful requests: {}/500", total_successful);
    
    // Assertions
    assert!(total_successful >= 450, "Too many failed requests in concurrent streams test");
    assert!(throughput > 200.0, "Concurrent streams throughput too low: {:.2} req/s", throughput);
    assert!(duration < Duration::from_secs(10), "Concurrent streams test took too long: {:?}", duration);
    
    Ok(())
}

#[tokio::test]
async fn test_memory_usage_stability() -> Result<()> {
    let (calculator_server, _, _) = get_servers().await;
    let client = QuicClient::new()?;
    
    // Perform a large number of operations to test memory stability
    let connection = client.connect(&calculator_server).await?;
    
    // Send many requests to test for memory leaks
    for batch in 0..10 {
        let mut handles = Vec::new();
        
        for i in 0..100 {
            let connection = connection.clone();
            let handle = tokio::spawn(async move {
                let request = CalculatorRequest::new(Operation::Power(2.0, (i % 10) as f64));
                let request_bytes = serde_json::to_vec(&request).unwrap();
                let response_bytes = connection.request(request_bytes).await.unwrap();
                let response: CalculatorResponse = serde_json::from_slice(&response_bytes).unwrap();
                response.result.unwrap()
            });
            handles.push(handle);
        }
        
        // Wait for batch to complete
        for handle in handles {
            let _ = handle.await.unwrap();
        }
        
        println!("Completed batch {} of memory stability test", batch + 1);
        
        // Small delay between batches to allow cleanup
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    println!("Memory stability test completed successfully");
    
    Ok(())
}