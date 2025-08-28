//! Test just the simple streaming concepts

use bytes::Bytes;

#[derive(Debug)]
pub struct SimpleStreamingDemo;

impl SimpleStreamingDemo {
    pub fn new() -> Self {
        Self
    }
    
    /// Simulate server streaming by returning multiple responses
    pub async fn server_stream_numbers(&self, count: u32) -> Vec<Bytes> {
        let mut results = Vec::new();
        for i in 0..count {
            let data = format!("number_{}", i);
            results.push(Bytes::from(data));
        }
        results
    }
    
    /// Simulate client streaming by aggregating multiple requests  
    pub async fn client_stream_sum(&self, requests: Vec<Bytes>) -> Bytes {
        let mut sum = 0i32;
        for request in requests {
            let num_str = String::from_utf8_lossy(&request);
            if let Ok(num) = num_str.parse::<i32>() {
                sum += num;
            }
        }
        Bytes::from(sum.to_string())
    }
    
    /// Simulate bidirectional streaming with echo functionality
    pub async fn bidirectional_echo(&self, requests: Vec<Bytes>) -> Vec<Bytes> {
        let mut responses = Vec::new();
        for request in requests {
            let echo = format!("echo_{}", String::from_utf8_lossy(&request));
            responses.push(Bytes::from(echo));
        }
        responses
    }
}

#[tokio::test]
async fn test_server_streaming_concept() {
    let demo = SimpleStreamingDemo::new();
    let results = demo.server_stream_numbers(3).await;
    
    assert_eq!(results.len(), 3);
    assert_eq!(String::from_utf8_lossy(&results[0]), "number_0");
    assert_eq!(String::from_utf8_lossy(&results[1]), "number_1");
    assert_eq!(String::from_utf8_lossy(&results[2]), "number_2");
}

#[tokio::test]
async fn test_client_streaming_concept() {
    let demo = SimpleStreamingDemo::new();
    let requests = vec![
        Bytes::from("1"),
        Bytes::from("2"), 
        Bytes::from("3"),
    ];
    
    let result = demo.client_stream_sum(requests).await;
    assert_eq!(String::from_utf8_lossy(&result), "6");
}

#[tokio::test]
async fn test_bidirectional_streaming_concept() {
    let demo = SimpleStreamingDemo::new();
    let requests = vec![
        Bytes::from("hello"),
        Bytes::from("world"),
    ];
    
    let responses = demo.bidirectional_echo(requests).await;
    assert_eq!(responses.len(), 2);
    assert_eq!(String::from_utf8_lossy(&responses[0]), "echo_hello");
    assert_eq!(String::from_utf8_lossy(&responses[1]), "echo_world");
}

#[tokio::test]
async fn test_empty_requests() {
    let demo = SimpleStreamingDemo::new();
    
    // Test empty server stream
    let results = demo.server_stream_numbers(0).await;
    assert_eq!(results.len(), 0);
    
    // Test empty client stream
    let result = demo.client_stream_sum(vec![]).await;
    assert_eq!(String::from_utf8_lossy(&result), "0");
    
    // Test empty bidirectional stream
    let responses = demo.bidirectional_echo(vec![]).await;
    assert_eq!(responses.len(), 0);
}

#[tokio::test]
async fn test_large_stream() {
    let demo = SimpleStreamingDemo::new();
    
    // Test large server stream
    let results = demo.server_stream_numbers(100).await;
    assert_eq!(results.len(), 100);
    assert_eq!(String::from_utf8_lossy(&results[99]), "number_99");
    
    // Test large client stream
    let requests: Vec<Bytes> = (1..=100).map(|i| Bytes::from(i.to_string())).collect();
    let result = demo.client_stream_sum(requests).await;
    assert_eq!(String::from_utf8_lossy(&result), "5050"); // Sum of 1 to 100
}