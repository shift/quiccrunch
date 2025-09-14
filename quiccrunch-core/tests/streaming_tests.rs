//! Comprehensive streaming RPC tests

use quiccrunch_core::rpc::*;
use quiccrunch_core::{QuicTransport, TlsConfig};
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use async_trait::async_trait;

/// Test streaming service
pub struct TestStreamingService;

#[async_trait]
impl StreamingRpcService for TestStreamingService {
    fn name(&self) -> &str {
        "TestStreamingService"
    }
    
    async fn server_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        params: Bytes,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            "stream_numbers" => {
                let count: u32 = String::from_utf8_lossy(&params).parse().unwrap_or(5);
                
                for i in 0..count {
                    let data = format!("number_{}", i);
                    sink.send(Bytes::from(data)).await?;
                    sleep(Duration::from_millis(10)).await;
                }
                
                sink.finish(None).await?;
            }
            "stream_error" => {
                sink.error(RpcError::ServiceError("Test error".to_string())).await?;
            }
            _ => {
                sink.error(RpcError::MethodNotFound(format!("Unknown method: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    async fn client_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
    ) -> RpcResult<Bytes> {
        match method {
            "collect_numbers" => {
                let mut sum = 0i32;
                let mut count = 0;
                
                while let Some(chunk_result) = receiver.recv().await {
                    match chunk_result {
                        Ok(chunk_data) => {
                            let num_str = String::from_utf8_lossy(&chunk_data);
                            if let Ok(num) = num_str.parse::<i32>() {
                                sum += num;
                                count += 1;
                            }
                        }
                        Err(_) => break,
                    }
                }
                
                let result = format!("sum:{},count:{}", sum, count);
                Ok(Bytes::from(result))
            }
            _ => Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
    
    async fn bidirectional_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> RpcResult<()> {
        match method {
            "echo_stream" => {
                while let Some(chunk_result) = receiver.recv().await {
                    match chunk_result {
                        Ok(chunk_data) => {
                            let echo_data = format!("echo_{}", String::from_utf8_lossy(&chunk_data));
                            sink.send(Bytes::from(echo_data)).await?;
                        }
                        Err(_) => break,
                    }
                }
                
                sink.finish(None).await?;
            }
            _ => {
                sink.error(RpcError::MethodNotFound(format!("Unknown method: {}", method))).await?;
            }
        }
        
        Ok(())
    }
    
    fn streaming_methods(&self) -> Vec<(String, StreamingType)> {
        vec![
            ("stream_numbers".to_string(), StreamingType::ServerStreaming),
            ("stream_error".to_string(), StreamingType::ServerStreaming),
            ("collect_numbers".to_string(), StreamingType::ClientStreaming),
            ("echo_stream".to_string(), StreamingType::BidirectionalStreaming),
        ]
    }
}

async fn setup_server_and_client() -> (RpcClient, tokio::task::JoinHandle<()>) {
    let server = RpcServer::new();
    let streaming_service = Arc::new(TestStreamingService);
    server.register_streaming_service(streaming_service).await;
    
    let transport = QuicTransport::new(TlsConfig::default()).unwrap();
    let listener = transport.bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    
    let server_handle = tokio::spawn(async move {
        while let Ok(connection) = listener.accept().await {
            let _ = server.handle_connection(connection).await;
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let client_transport = QuicTransport::new(TlsConfig::default()).unwrap();
    let connection = client_transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RpcClient::new(connection);
    
    (client, server_handle)
}

#[tokio::test]
async fn test_server_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let mut receiver = client.server_streaming_call(
        "TestStreamingService",
        "stream_numbers",
        Bytes::from("3"),
    ).await.unwrap();
    
    let mut received = Vec::new();
    while let Some(chunk_result) = receiver.recv().await {
        match chunk_result {
            Ok(chunk_data) => {
                received.push(String::from_utf8_lossy(&chunk_data).to_string());
            }
            Err(_) => break,
        }
    }
    
    assert_eq!(received, vec!["number_0", "number_1", "number_2"]);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_server_streaming_error() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let mut receiver = client.server_streaming_call(
        "TestStreamingService",
        "stream_error",
        Bytes::new(),
    ).await.unwrap();
    
    let mut error_received = false;
    while let Some(chunk_result) = receiver.recv().await {
        match chunk_result {
            Ok(_) => {},
            Err(_) => {
                error_received = true;
                break;
            }
        }
    }
    
    assert!(error_received, "Expected to receive an error");
    
    server_handle.abort();
}

#[tokio::test]
async fn test_client_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send numbers
    for i in 1..=5 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish(None).await.unwrap();
    
    // Receive result
    if let Some(result) = receiver.recv().await {
        let result_data = result.unwrap();
        let result_str = String::from_utf8_lossy(&result_data);
        assert_eq!(result_str, "sum:15,count:5");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_bidirectional_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.bidirectional_streaming_call(
        "TestStreamingService",
        "echo_stream"
    ).await.unwrap();
    
    // Spawn task to receive echoes
    let receive_handle = tokio::spawn(async move {
        let mut received = Vec::new();
        while let Some(chunk_result) = receiver.recv().await {
            match chunk_result {
                Ok(chunk_data) => {
                    received.push(String::from_utf8_lossy(&chunk_data).to_string());
                }
                Err(_) => break,
            }
        }
        received
    });
    
    // Send messages
    let messages = vec!["hello", "world", "test"];
    for msg in &messages {
        sink.send(Bytes::from(*msg)).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish(None).await.unwrap();
    
    // Check echoes
    let received = receive_handle.await.unwrap();
    let expected: Vec<String> = messages.iter()
        .map(|msg| format!("echo_{}", msg))
        .collect();
    
    assert_eq!(received, expected);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_streaming_with_large_data() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send large amount of data
    for i in 1..=100 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish(None).await.unwrap();
    
    // Receive result
    if let Some(result) = receiver.recv().await {
        let result_data = result.unwrap();
        let result_str = String::from_utf8_lossy(&result_data);
        // Sum of 1 to 100 is 5050
        assert_eq!(result_str, "sum:5050,count:100");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_streams() {
    let (client, server_handle) = setup_server_and_client().await;
    
    // Start multiple concurrent streams
    let mut handles = Vec::new();
    
    for i in 0..5 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let mut receiver = client_clone.server_streaming_call(
                "TestStreamingService",
                "stream_numbers",
                Bytes::from("2"),
            ).await.unwrap();
            
            let mut count = 0;
            while let Some(chunk_result) = receiver.recv().await {
                if chunk_result.is_ok() {
                    count += 1;
                }
            }
            (i, count)
        });
        handles.push(handle);
    }
    
    // Wait for all streams to complete
    let mut results = Vec::new();
    for handle in handles {
        let (stream_id, count) = handle.await.unwrap();
        results.push((stream_id, count));
    }
    
    // All streams should have received 2 messages
    assert_eq!(results.len(), 5);
    for (_, count) in results {
        assert_eq!(count, 2);
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_stream_backpressure() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send data rapidly without waiting
    for i in 1..=10 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish(None).await.unwrap();
    
    // Should still receive correct result despite rapid sending
    if let Some(result) = receiver.recv().await {
        let result_data = result.unwrap();
        let result_str = String::from_utf8_lossy(&result_data);
        assert_eq!(result_str, "sum:55,count:10");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}
                    sleep(Duration::from_millis(10)).await;
                }
                
                let _ = sink.finish().await;
            }
            "stream_error" => {
                return Err(RpcError::ServiceError("Test error".to_string()));
            }
            _ => {
                return Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)));
            }
        }
        
        Ok(())
    }
    
    async fn client_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
    ) -> StreamingRpcResult<Bytes> {
        match method {
            "collect_numbers" => {
                let mut sum = 0i32;
                let mut count = 0;
                
                while let Ok(Some(chunk_data)) = receiver.next().await {
                    let num_str = String::from_utf8_lossy(&chunk_data);
                    if let Ok(num) = num_str.parse::<i32>() {
                        sum += num;
                        count += 1;
                    }
                }
                
                let result = format!("sum:{},count:{}", sum, count);
                Ok(Bytes::from(result))
            }
            _ => Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
    
    async fn bidirectional_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()> {
        match method {
            "echo_stream" => {
                while let Ok(Some(chunk_data)) = receiver.next().await {
                    let echo_data = format!("echo_{}", String::from_utf8_lossy(&chunk_data));
                    if sink.send(Bytes::from(echo_data)).await.is_err() {
                        break;
                    }
                }
                
                let _ = sink.finish().await;
            }
            _ => {
                return Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)));
            }
        }
        
        Ok(())
    }
}

async fn setup_server_and_client() -> (RpcClient, tokio::task::JoinHandle<()>) {
    let server = RpcServer::new();
    let streaming_service = Arc::new(TestStreamingService);
    server.register_streaming_service("TestStreamingService".to_string(), streaming_service).await;
    
    let tls_config = TlsConfig::generate_self_signed("localhost").unwrap();
    let transport = QuicTransport::new(tls_config.clone()).unwrap();
    let mut listener = transport.listen("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    
    let server_handle = tokio::spawn(async move {
        while let Ok(connection) = listener.accept().await {
            let server = server.clone();
            tokio::spawn(async move {
                if let Err(e) = server.handle_connection(connection).await {
                    eprintln!("Connection error: {}", e);
                }
            });
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let client_transport = QuicTransport::new(tls_config).unwrap();
    let connection = client_transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RpcClient::new(connection);
    
    (client, server_handle)
}

#[tokio::test]
async fn test_server_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let mut receiver = client.server_streaming_call(
        "TestStreamingService",
        "stream_numbers",
        Bytes::from("3"),
    ).await.unwrap();
    
    let mut received = Vec::new();
    while let Ok(Some(chunk_data)) = receiver.next().await {
        received.push(String::from_utf8_lossy(&chunk_data).to_string());
    }
    
    assert_eq!(received, vec!["number_0", "number_1", "number_2"]);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_server_streaming_error() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let result = client.server_streaming_call(
        "TestStreamingService",
        "stream_error",
        Bytes::new(),
    ).await;
    
    assert!(result.is_err(), "Expected to receive an error from the initial call");
    
    server_handle.abort();
}

#[tokio::test]
async fn test_client_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, result_future) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send numbers
    for i in 1..=5 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish().await.unwrap();
    
    // Receive result
    let result_data = result_future.await.unwrap();
    let result_str = String::from_utf8_lossy(&result_data);
    assert_eq!(result_str, "sum:15,count:5");
    
    server_handle.abort();
}

#[tokio::test]
async fn test_bidirectional_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.bidirectional_streaming_call(
        "TestStreamingService",
        "echo_stream"
    ).await.unwrap();
    
    // Spawn task to receive echoes
    let receive_handle = tokio::spawn(async move {
        let mut received = Vec::new();
        while let Ok(Some(chunk_data)) = receiver.next().await {
            received.push(String::from_utf8_lossy(&chunk_data).to_string());
        }
        received
    });
    
    // Send messages
    let messages = vec!["hello", "world", "test"];
    for msg in &messages {
        sink.send(Bytes::from(*msg)).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish().await.unwrap();
    
    // Check echoes
    let received = receive_handle.await.unwrap();
    let expected: Vec<String> = messages.iter()
        .map(|msg| format!("echo_{}", msg))
        .collect();
    
    assert_eq!(received, expected);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_streaming_with_large_data() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, result_future) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send large amount of data
    for i in 1..=100 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish().await.unwrap();
    
    // Receive result
    let result_data = result_future.await.unwrap();
    let result_str = String::from_utf8_lossy(&result_data);
    // Sum of 1 to 100 is 5050
    assert_eq!(result_str, "sum:5050,count:100");
    
    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_streams() {
    let (client, server_handle) = setup_server_and_client().await;
    
    // Start multiple concurrent streams
    let mut handles = Vec::new();
    
    for i in 0..5 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let mut receiver = client_clone.server_streaming_call(
                "TestStreamingService",
                "stream_numbers",
                Bytes::from("2"),
            ).await.unwrap();
            
            let mut count = 0;
            while let Ok(Some(_)) = receiver.next().await {
                count += 1;
            }
            (i, count)
        });
        handles.push(handle);
    }
    
    // Wait for all streams to complete
    let mut results = Vec::new();
    for handle in handles {
        let (stream_id, count) = handle.await.unwrap();
        results.push((stream_id, count));
    }
    
    // All streams should have received 2 messages
    assert_eq!(results.len(), 5);
    for (_, count) in results {
        assert_eq!(count, 2);
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_stream_backpressure() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, result_future) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send data rapidly without waiting
    for i in 1..=10 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish().await.unwrap();
    
    // Should still receive correct result despite rapid sending
    let result_data = result_future.await.unwrap();
    let result_str = String::from_utf8_lossy(&result_data);
    assert_eq!(result_str, "sum:55,count:10");
    
    server_handle.abort();
}
                
                sink.finish().await?;
            }
            "stream_error" => {
                // The RpcStreamSink does not have an error method.
                // To signal an error, we can just drop the sink or return an error from the function.
                return Err(RpcError::ServiceError("Test error".to_string()));
            }
            _ => {
                return Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)));
            }
        }
        
        Ok(())
    }
    
    async fn client_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
    ) -> StreamingRpcResult<Bytes> {
        match method {
            "collect_numbers" => {
                let mut sum = 0i32;
                let mut count = 0;
                
                while let Ok(Some(chunk_data)) = receiver.next().await {
                    let num_str = String::from_utf8_lossy(&chunk_data);
                    if let Ok(num) = num_str.parse::<i32>() {
                        sum += num;
                        count += 1;
                    }
                }
                
                let result = format!("sum:{},count:{}", sum, count);
                Ok(Bytes::from(result))
            }
            _ => Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)))
        }
    }
    
    async fn bidirectional_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut receiver: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()> {
        match method {
            "echo_stream" => {
                while let Ok(Some(chunk_data)) = receiver.next().await {
                    let echo_data = format!("echo_{}", String::from_utf8_lossy(&chunk_data));
                    sink.send(Bytes::from(echo_data)).await?;
                }
                
                sink.finish().await?;
            }
            _ => {
                return Err(RpcError::MethodNotFound(format!("Unknown method: {}", method)));
            }
        }
        
        Ok(())
    }
}

async fn setup_server_and_client() -> (RpcClient, tokio::task::JoinHandle<()>) {
    let server = RpcServer::new();
    let streaming_service = Arc::new(TestStreamingService);
    server.register_streaming_service("TestStreamingService".to_string(), streaming_service).await;
    
    let transport = QuicTransport::new(TlsConfig::generate_self_signed("localhost").unwrap()).unwrap();
    let listener = transport.listen("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    
    let server_handle = tokio::spawn(async move {
        while let Ok(connection) = listener.accept().await {
            let _ = server.handle_connection(connection).await;
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    
    let client_transport = QuicTransport::new(TlsConfig::generate_self_signed("localhost").unwrap()).unwrap();
    let connection = client_transport.connect(&server_addr.to_string()).await.unwrap();
    let client = RpcClient::new(connection);
    
    (client, server_handle)
}

#[tokio::test]
async fn test_server_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let mut receiver = client.server_streaming_call(
        "TestStreamingService",
        "stream_numbers",
        Bytes::from("3"),
    ).await.unwrap();
    
    let mut received = Vec::new();
    while let Ok(Some(chunk_data)) = receiver.next().await {
        received.push(String::from_utf8_lossy(&chunk_data).to_string());
    }
    
    assert_eq!(received, vec!["number_0", "number_1", "number_2"]);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_server_streaming_error() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let mut receiver = client.server_streaming_call(
        "TestStreamingService",
        "stream_error",
        Bytes::new(),
    ).await.unwrap();
    
    let mut error_received = false;
    while let Ok(Some(_)) = receiver.next().await {
        // Should not receive data
    }

    // After the loop, check if an error was the cause of termination.
    // The `next` method will return `Err` when the stream terminates with an error.
    if let Err(_) = receiver.next().await {
        error_received = true;
    }
    
    assert!(error_received, "Expected to receive an error");
    
    server_handle.abort();
}

#[tokio::test]
async fn test_client_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send numbers
    for i in 1..=5 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish().await.unwrap();
    
    // Receive result
    if let Ok(Some(result_data)) = receiver.next().await {
        let result_str = String::from_utf8_lossy(&result_data);
        assert_eq!(result_str, "sum:15,count:5");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_bidirectional_streaming() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.bidirectional_streaming_call(
        "TestStreamingService",
        "echo_stream"
    ).await.unwrap();
    
    // Spawn task to receive echoes
    let receive_handle = tokio::spawn(async move {
        let mut received = Vec::new();
        while let Ok(Some(chunk_data)) = receiver.next().await {
            received.push(String::from_utf8_lossy(&chunk_data).to_string());
        }
        received
    });
    
    // Send messages
    let messages = vec!["hello", "world", "test"];
    for msg in &messages {
        sink.send(Bytes::from(*msg)).await.unwrap();
        sleep(Duration::from_millis(10)).await;
    }
    
    sink.finish().await.unwrap();
    
    // Check echoes
    let received = receive_handle.await.unwrap();
    let expected: Vec<String> = messages.iter()
        .map(|msg| format!("echo_{}", msg))
        .collect();
    
    assert_eq!(received, expected);
    
    server_handle.abort();
}

#[tokio::test]
async fn test_streaming_with_large_data() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send large amount of data
    for i in 1..=100 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish().await.unwrap();
    
    // Receive result
    if let Ok(Some(result_data)) = receiver.next().await {
        let result_str = String::from_utf8_lossy(&result_data);
        // Sum of 1 to 100 is 5050
        assert_eq!(result_str, "sum:5050,count:100");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_streams() {
    let (client, server_handle) = setup_server_and_client().await;
    
    // Start multiple concurrent streams
    let mut handles = Vec::new();
    
    for i in 0..5 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let mut receiver = client_clone.server_streaming_call(
                "TestStreamingService",
                "stream_numbers",
                Bytes::from("2"),
            ).await.unwrap();
            
            let mut count = 0;
            while let Ok(Some(_)) = receiver.next().await {
                count += 1;
            }
            (i, count)
        });
        handles.push(handle);
    }
    
    // Wait for all streams to complete
    let mut results = Vec::new();
    for handle in handles {
        let (stream_id, count) = handle.await.unwrap();
        results.push((stream_id, count));
    }
    
    // All streams should have received 2 messages
    assert_eq!(results.len(), 5);
    for (_, count) in results {
        assert_eq!(count, 2);
    }
    
    server_handle.abort();
}

#[tokio::test]
async fn test_stream_backpressure() {
    let (client, server_handle) = setup_server_and_client().await;
    
    let (sink, mut receiver) = client.client_streaming_call(
        "TestStreamingService",
        "collect_numbers"
    ).await.unwrap();
    
    // Send data rapidly without waiting
    for i in 1..=10 {
        sink.send(Bytes::from(i.to_string())).await.unwrap();
    }
    
    sink.finish().await.unwrap();
    
    // Should still receive correct result despite rapid sending
    if let Ok(Some(result_data)) = receiver.next().await {
        let result_str = String::from_utf8_lossy(&result_data);
        assert_eq!(result_str, "sum:55,count:10");
    } else {
        panic!("Expected to receive result");
    }
    
    server_handle.abort();
}