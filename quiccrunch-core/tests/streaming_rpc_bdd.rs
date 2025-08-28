//! BDD tests for streaming RPC functionality
//! 
//! These tests follow Given-When-Then scenarios to verify streaming RPC patterns.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use bytes::Bytes;
use tokio::sync::{mpsc, Mutex};
use tokio_stream::StreamExt;

use quiccrunch_core::rpc::{
    RpcServer, RpcClient, StreamingRpcService, RpcContext, RpcStreamSink, 
    RpcStreamReceiver, StreamingRpcResult, RpcResult
};
use quiccrunch_core::{QuicStream, Connection};

/// Test streaming service that implements all streaming patterns
struct TestStreamingService;

#[async_trait]
impl StreamingRpcService for TestStreamingService {
    /// Server streaming: single request, multiple responses
    async fn server_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        params: Bytes,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()> {
        match method {
            "stream_numbers" => {
                // Parse count from params
                let count: u32 = String::from_utf8_lossy(&params).parse().unwrap_or(5);
                
                // Send count numbers
                for i in 0..count {
                    let data = Bytes::from(format!("number_{}", i));
                    sink.send(data).await?;
                }
                
                // Finish the stream
                sink.finish().await?;
                Ok(())
            }
            _ => Err(quiccrunch::rpc::RpcError::MethodNotFound(method.to_string())),
        }
    }
    
    /// Client streaming: multiple requests, single response
    async fn client_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut stream: RpcStreamReceiver,
    ) -> StreamingRpcResult<Bytes> {
        match method {
            "sum_numbers" => {
                let mut sum = 0u64;
                
                // Collect all numbers from the stream
                while let Some(result) = stream.next().await {
                    let data = result?;
                    if let Ok(num_str) = String::from_utf8(data.to_vec()) {
                        if let Ok(num) = num_str.parse::<u64>() {
                            sum += num;
                        }
                    }
                }
                
                Ok(Bytes::from(sum.to_string()))
            }
            _ => Err(quiccrunch::rpc::RpcError::MethodNotFound(method.to_string())),
        }
    }
    
    /// Bidirectional streaming: multiple requests, multiple responses
    async fn bidirectional_streaming_call(
        &self,
        _context: &RpcContext,
        method: &str,
        mut stream: RpcStreamReceiver,
        sink: RpcStreamSink,
    ) -> StreamingRpcResult<()> {
        match method {
            "echo_transform" => {
                // Echo each message with transformation
                while let Some(result) = stream.next().await {
                    let data = result?;
                    let transformed = format!("echo: {}", String::from_utf8_lossy(&data));
                    sink.send(Bytes::from(transformed)).await?;
                }
                
                sink.finish().await?;
                Ok(())
            }
            _ => Err(quiccrunch::rpc::RpcError::MethodNotFound(method.to_string())),
        }
    }
}

/// Mock QuicStream for testing
struct MockQuicStream {
    sender: Option<mpsc::UnboundedSender<Bytes>>,
    receiver: Option<mpsc::UnboundedReceiver<Bytes>>,
}

impl MockQuicStream {
    fn new() -> (Self, mpsc::UnboundedSender<Bytes>, mpsc::UnboundedReceiver<Bytes>) {
        let (tx1, rx1) = mpsc::unbounded_channel();
        let (tx2, rx2) = mpsc::unbounded_channel();
        
        let stream = Self {
            sender: Some(tx1),
            receiver: Some(rx2),
        };
        
        (stream, tx2, rx1)
    }
}

#[async_trait::async_trait]
impl QuicStream for MockQuicStream {
    async fn send_message(&mut self, data: Bytes) -> quiccrunch::Result<()> {
        if let Some(sender) = &self.sender {
            sender.send(data).map_err(|_| quiccrunch::TransportError::Connection("Send failed".to_string()))?;
        }
        Ok(())
    }
    
    async fn receive_message(&mut self) -> quiccrunch::Result<Bytes> {
        if let Some(receiver) = &mut self.receiver {
            receiver.recv().await.ok_or_else(|| quiccrunch::TransportError::Connection("Receive failed".to_string()))
        } else {
            Err(quiccrunch::TransportError::Connection("No receiver".to_string()))
        }
    }
}

impl Clone for MockQuicStream {
    fn clone(&self) -> Self {
        // For testing, create a new disconnected stream
        Self {
            sender: None,
            receiver: None,
        }
    }
}

/// Helper to create a mock sink for testing
fn create_mock_sink(tx: mpsc::UnboundedSender<Bytes>) -> RpcStreamSink {
    let (mock_stream, _stream_tx, mut stream_rx) = MockQuicStream::new();
    let stream_id = 1;
    
    // Spawn task to forward messages to the test channel
    tokio::spawn(async move {
        while let Some(data) = stream_rx.recv().await {
            if tx.send(data).is_err() {
                break;
            }
        }
    });
    
    RpcStreamSink::new(Box::new(mock_stream), stream_id)
}

#[tokio::test]
async fn test_server_streaming_scenario() {
    // Given: A streaming RPC server with a number streaming service
    let service = TestStreamingService;
    let context = RpcContext::new("TestStreamingService".to_string(), "stream_numbers".to_string());
    let params = Bytes::from("3");
    
    // When: Client requests server streaming with count parameter
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mock_sink = create_mock_sink(tx);
    
    // Then: Server should send the specified number of messages
    let result = service.server_streaming_call(&context, "stream_numbers", params, mock_sink).await;
    assert!(result.is_ok(), "Server streaming call should succeed");
    
    // Verify we received the expected number of messages
    let mut received_count = 0;
    while let Some(data) = rx.recv().await {
        received_count += 1;
        assert!(data.starts_with(b"number_"), "Message should start with 'number_'");
        if received_count >= 3 {
            break;
        }
    }
    assert_eq!(received_count, 3, "Should receive exactly 3 messages");
}

#[tokio::test]
async fn test_client_streaming_scenario() {
    // Given: A streaming RPC server with a sum aggregation service
    let service = TestStreamingService;
    let context = RpcContext::new("TestStreamingService".to_string(), "sum_numbers".to_string());
    
    // When: Client sends multiple numbers via streaming
    let (tx, rx) = mpsc::unbounded_channel();
    let stream_receiver = RpcStreamReceiver::new(rx);
    
    // Send test numbers
    tx.send(Ok(Bytes::from("10"))).unwrap();
    tx.send(Ok(Bytes::from("20"))).unwrap();
    tx.send(Ok(Bytes::from("30"))).unwrap();
    drop(tx); // Close the stream
    
    // Then: Server should return the sum
    let result = service.client_streaming_call(&context, "sum_numbers", stream_receiver).await;
    assert!(result.is_ok(), "Client streaming call should succeed");
    
    let sum = String::from_utf8_lossy(&result.unwrap());
    assert_eq!(sum, "60", "Sum should be 60");
}

#[tokio::test]
async fn test_bidirectional_streaming_scenario() {
    // Given: A streaming RPC server with an echo transformation service
    let service = TestStreamingService;
    let context = RpcContext::new("TestStreamingService".to_string(), "echo_transform".to_string());
    
    // When: Client sends messages and receives transformed responses
    let (input_tx, input_rx) = mpsc::unbounded_channel();
    let stream_receiver = RpcStreamReceiver::new(input_rx);
    
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let mock_sink = create_mock_sink(output_tx);
    
    // Send test messages
    input_tx.send(Ok(Bytes::from("hello"))).unwrap();
    input_tx.send(Ok(Bytes::from("world"))).unwrap();
    drop(input_tx); // Close input stream
    
    // Then: Server should echo with transformation
    let result = service.bidirectional_streaming_call(&context, "echo_transform", stream_receiver, mock_sink).await;
    assert!(result.is_ok(), "Bidirectional streaming call should succeed");
    
    // Verify transformed messages
    if let Some(msg1) = output_rx.recv().await {
        assert_eq!(String::from_utf8_lossy(&msg1), "echo: hello");
    }
    if let Some(msg2) = output_rx.recv().await {
        assert_eq!(String::from_utf8_lossy(&msg2), "echo: world");
    }
}

#[tokio::test]
async fn test_streaming_error_handling() {
    // Given: A streaming service
    let service = TestStreamingService;
    let context = RpcContext::new("TestStreamingService".to_string(), "unknown_method".to_string());
    
    // When: Calling a non-existent streaming method
    let (tx, rx) = mpsc::unbounded_channel();
    let stream_receiver = RpcStreamReceiver::new(rx);
    let mock_sink = create_mock_sink(mpsc::unbounded_channel().0);
    
    // Then: Should return method not found error
    let result = service.server_streaming_call(&context, "unknown_method", Bytes::new(), mock_sink).await;
    assert!(result.is_err(), "Should return error for unknown method");
    
    if let Err(err) = result {
        assert!(matches!(err, quiccrunch::rpc::RpcError::MethodNotFound(_)));
    }
}