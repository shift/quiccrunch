# QuicCrunch Streaming RPC Tutorial

This comprehensive tutorial will guide you through all of QuicCrunch's streaming RPC patterns, from simple unary calls to complex bidirectional streaming. By the end, you'll understand how to leverage QUIC's native multiplexing for high-performance streaming applications.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Unary RPCs](#unary-rpcs)
3. [Server Streaming](#server-streaming)
4. [Client Streaming](#client-streaming)
5. [Bidirectional Streaming](#bidirectional-streaming)
6. [Advanced Patterns](#advanced-patterns)
7. [Performance Optimization](#performance-optimization)
8. [Error Handling](#error-handling)
9. [Best Practices](#best-practices)

## Quick Start

Let's start with a simple example that demonstrates all four RPC patterns in one service:

```rust
use quiccrunch_macros::rpc_service;
use quiccrunch_core::rpc::{RpcContext, RpcResult, RpcStreamSink, RpcStreamSource};
use bytes::Bytes;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Message {
    content: String,
    timestamp: u64,
}

#[rpc_service(name = "MessageService")]
trait MessageService {
    // Unary: Single request → Single response
    async fn echo(&self, message: String) -> RpcResult<String>;
    
    // Server streaming: Single request → Stream of responses
    async fn subscribe_messages(&self, channel: String) -> RpcResult<RpcStreamSource<Message>>;
    
    // Client streaming: Stream of requests → Single response
    async fn batch_send(&self, messages: RpcStreamSource<Message>) -> RpcResult<u32>;
    
    // Bidirectional: Stream of requests → Stream of responses
    async fn live_chat(&self, input: RpcStreamSource<Message>) -> RpcResult<RpcStreamSource<Message>>;
}
```

## Unary RPCs

Unary RPCs are the simplest pattern - one request, one response. They're perfect for operations like user lookup, configuration retrieval, or simple calculations.

### Implementation

```rust
struct MessageServiceImpl;

impl MessageService for MessageServiceImpl {
    async fn echo(&self, message: String) -> RpcResult<String> {
        // Simple echo with added timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Ok(format!("[{}] Echo: {}", timestamp, message))
    }
}
```

### Client Usage

```rust
// Generated client code (via #[rpc_service] macro)
let client = MessageServiceClient::new(connection);
let response = client.echo("Hello QuicCrunch!".to_string()).await?;
println!("Server response: {}", response);
```

### When to Use Unary RPCs

- **Simple queries**: User lookup, configuration retrieval
- **Atomic operations**: Create, update, delete operations
- **Calculations**: Mathematical operations, data transformations
- **Health checks**: Service status, ping operations

## Server Streaming

Server streaming sends multiple responses for a single request. This pattern is ideal for real-time data feeds, large dataset transfers, or progress reporting.

### Implementation

```rust
impl MessageService for MessageServiceImpl {
    async fn subscribe_messages(&self, channel: String) -> RpcResult<RpcStreamSource<Message>> {
        let (mut sink, source) = RpcStreamSink::create();
        
        // Spawn background task to send messages
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut counter = 0;
            
            loop {
                interval.tick().await;
                counter += 1;
                
                let message = Message {
                    content: format!("Message {} on channel {}", counter, channel),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };
                
                // Send message to client
                if sink.send(message).await.is_err() {
                    break; // Client disconnected
                }
                
                // Stop after 10 messages
                if counter >= 10 {
                    let _ = sink.finish().await;
                    break;
                }
            }
        });
        
        Ok(source)
    }
}
```

### Client Usage

```rust
let mut stream = client.subscribe_messages("general".to_string()).await?;

while let Some(message) = stream.next().await {
    match message {
        Ok(msg) => {
            println!("Received: {} at {}", msg.content, msg.timestamp);
        }
        Err(e) => {
            eprintln!("Stream error: {}", e);
            break;
        }
    }
}

println!("Stream completed");
```

### Server Streaming Use Cases

- **Live data feeds**: Stock prices, sensor readings, logs
- **Large file downloads**: Chunked file transfer with progress
- **Real-time notifications**: Chat messages, system alerts
- **Database cursors**: Streaming query results

### Advanced Server Streaming

For more complex scenarios, you can implement backpressure and flow control:

```rust
async fn advanced_data_feed(&self, request: DataRequest) -> RpcResult<RpcStreamSource<DataPoint>> {
    let (mut sink, source) = RpcStreamSink::create();
    
    tokio::spawn(async move {
        let mut data_source = DatabaseCursor::new(request.query).await?;
        let mut buffer = Vec::with_capacity(100);
        
        while let Some(batch) = data_source.next_batch(100).await? {
            for data_point in batch {
                buffer.push(data_point);
                
                // Send in batches for efficiency
                if buffer.len() >= 10 {
                    for point in buffer.drain(..) {
                        if sink.send(point).await.is_err() {
                            return; // Client disconnected
                        }
                    }
                }
            }
            
            // Respect client's processing speed
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        // Send remaining items
        for point in buffer {
            let _ = sink.send(point).await;
        }
        
        let _ = sink.finish().await;
    });
    
    Ok(source)
}
```

## Client Streaming

Client streaming sends multiple requests and receives a single response. This pattern is perfect for uploading data, batch operations, or aggregating information.

### Implementation

```rust
impl MessageService for MessageServiceImpl {
    async fn batch_send(&self, mut messages: RpcStreamSource<Message>) -> RpcResult<u32> {
        let mut count = 0;
        let mut total_chars = 0;
        
        // Process incoming messages
        while let Some(message) = messages.next().await {
            match message {
                Ok(msg) => {
                    count += 1;
                    total_chars += msg.content.len();
                    
                    // Process the message (save to database, forward, etc.)
                    self.process_message(msg).await?;
                }
                Err(e) => {
                    return Err(RpcError::InvalidParameters(format!("Message error: {}", e)));
                }
            }
        }
        
        println!("Processed {} messages, {} total characters", count, total_chars);
        Ok(count)
    }
}
```

### Client Usage

```rust
let (sink, response_future) = client.batch_send().await?;

// Send multiple messages
let messages = vec![
    Message { content: "First message".to_string(), timestamp: now() },
    Message { content: "Second message".to_string(), timestamp: now() },
    Message { content: "Third message".to_string(), timestamp: now() },
];

for message in messages {
    sink.send(message).await?;
}

// Important: Signal end of stream
sink.finish().await?;

// Wait for final response
let message_count = response_future.await?;
println!("Server processed {} messages", message_count);
```

### Client Streaming Use Cases

- **File uploads**: Large file transfer in chunks
- **Batch processing**: Multiple database operations
- **Data ingestion**: Streaming sensor data, logs
- **Bulk operations**: Mass user imports, bulk updates

### Upload with Progress Example

```rust
async fn upload_file(&self, mut chunks: RpcStreamSource<FileChunk>) -> RpcResult<UploadResult> {
    let mut file_data = Vec::new();
    let mut total_size = 0;
    let mut filename = String::new();
    
    while let Some(chunk_result) = chunks.next().await {
        let chunk = chunk_result?;
        
        if filename.is_empty() {
            filename = chunk.filename.clone();
        }
        
        file_data.extend_from_slice(&chunk.data);
        total_size += chunk.data.len();
        
        // Optional: Send progress updates via another stream
        // progress_sink.send(ProgressUpdate { bytes_received: total_size }).await?;
    }
    
    // Save file and return result
    let file_path = format!("uploads/{}", filename);
    std::fs::write(&file_path, file_data)?;
    
    Ok(UploadResult {
        filename,
        size: total_size as u64,
        path: file_path,
        checksum: calculate_checksum(&file_data),
    })
}
```

## Bidirectional Streaming

Bidirectional streaming allows both client and server to send multiple messages independently. This enables real-time, interactive applications.

### Implementation

```rust
impl MessageService for MessageServiceImpl {
    async fn live_chat(&self, mut input: RpcStreamSource<Message>) -> RpcResult<RpcStreamSource<Message>> {
        let (mut output_sink, output_source) = RpcStreamSink::create();
        
        // Spawn task to handle incoming messages
        tokio::spawn(async move {
            while let Some(message_result) = input.next().await {
                match message_result {
                    Ok(msg) => {
                        println!("Received chat message: {}", msg.content);
                        
                        // Echo the message back with server prefix
                        let echo_message = Message {
                            content: format!("Server echo: {}", msg.content),
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };
                        
                        if output_sink.send(echo_message).await.is_err() {
                            break; // Client disconnected
                        }
                    }
                    Err(e) => {
                        eprintln!("Error receiving message: {}", e);
                        break;
                    }
                }
            }
            
            // Send goodbye message
            let goodbye = Message {
                content: "Server: Goodbye!".to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            
            let _ = output_sink.send(goodbye).await;
            let _ = output_sink.finish().await;
        });
        
        Ok(output_source)
    }
}
```

### Client Usage

```rust
let (input_sink, output_stream) = client.live_chat().await?;

// Spawn task to receive messages
let receive_handle = tokio::spawn(async move {
    let mut stream = output_stream;
    while let Some(message) = stream.next().await {
        match message {
            Ok(msg) => println!("📨 {}", msg.content),
            Err(e) => {
                eprintln!("❌ Receive error: {}", e);
                break;
            }
        }
    }
});

// Send messages from main thread
let messages = vec![
    "Hello server!",
    "How are you?",
    "This is bidirectional streaming!",
];

for msg_text in messages {
    let message = Message {
        content: msg_text.to_string(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    
    input_sink.send(message).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// Signal end of input
input_sink.finish().await?;

// Wait for all responses
receive_handle.await?;
```

### Bidirectional Streaming Use Cases

- **Real-time chat**: Instant messaging, collaborative editing
- **Live collaboration**: Document editing, whiteboarding
- **Gaming**: Real-time multiplayer game state
- **Trading platforms**: Live order updates and market data
- **Monitoring**: Real-time metrics with control commands

## Advanced Patterns

### Stream Multiplexing

QuicCrunch leverages QUIC's native multiplexing to handle thousands of concurrent streams efficiently:

```rust
async fn handle_multi_stream_client(&self) -> RpcResult<()> {
    let mut handles = Vec::new();
    
    // Start multiple concurrent streams
    for i in 0..100 {
        let client = self.client.clone();
        let handle = tokio::spawn(async move {
            let stream = client.subscribe_messages(format!("channel_{}", i)).await?;
            // Process stream...
            Ok::<_, RpcError>(())
        });
        handles.push(handle);
    }
    
    // Wait for all streams to complete
    for handle in handles {
        handle.await??;
    }
    
    Ok(())
}
```

### Stream Composition

Combine multiple streams for complex data processing:

```rust
async fn aggregated_data_stream(&self, sources: Vec<String>) -> RpcResult<RpcStreamSource<AggregatedData>> {
    let (mut output_sink, output_source) = RpcStreamSink::create();
    
    // Start multiple input streams
    let mut streams = Vec::new();
    for source in sources {
        let stream = self.subscribe_raw_data(source).await?;
        streams.push(stream);
    }
    
    tokio::spawn(async move {
        let mut aggregator = DataAggregator::new();
        
        // Use select! to handle multiple streams concurrently
        loop {
            tokio::select! {
                Some(data) = streams[0].next() => {
                    aggregator.add_data(0, data?);
                }
                Some(data) = streams[1].next() => {
                    aggregator.add_data(1, data?);
                }
                // ... handle other streams
                else => break,
            }
            
            // Send aggregated result if ready
            if let Some(result) = aggregator.get_result() {
                if output_sink.send(result).await.is_err() {
                    break;
                }
            }
        }
        
        let _ = output_sink.finish().await;
    });
    
    Ok(output_source)
}
```

### Flow Control and Backpressure

Handle slow consumers gracefully:

```rust
async fn rate_controlled_stream(&self, rate_limit: Duration) -> RpcResult<RpcStreamSource<Data>> {
    let (mut sink, source) = RpcStreamSink::create();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(rate_limit);
        let mut data_generator = DataGenerator::new();
        
        loop {
            interval.tick().await;
            
            let data = data_generator.next();
            match sink.send(data).await {
                Ok(_) => continue,
                Err(e) if e.is_backpressure() => {
                    // Handle backpressure - maybe buffer or drop data
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(_) => break, // Client disconnected
            }
        }
    });
    
    Ok(source)
}
```

## Performance Optimization

### Batching for Efficiency

Group small messages together to reduce protocol overhead:

```rust
async fn batched_stream(&self, batch_size: usize) -> RpcResult<RpcStreamSource<Vec<Item>>> {
    let (mut sink, source) = RpcStreamSink::create();
    
    tokio::spawn(async move {
        let mut buffer = Vec::with_capacity(batch_size);
        let mut data_source = DataSource::new();
        
        while let Some(item) = data_source.next().await {
            buffer.push(item);
            
            if buffer.len() >= batch_size {
                if sink.send(buffer.clone()).await.is_err() {
                    break;
                }
                buffer.clear();
            }
        }
        
        // Send remaining items
        if !buffer.is_empty() {
            let _ = sink.send(buffer).await;
        }
        
        let _ = sink.finish().await;
    });
    
    Ok(source)
}
```

### Memory Management

Avoid unbounded buffering in streaming scenarios:

```rust
async fn memory_efficient_processing(&self, mut input: RpcStreamSource<LargeData>) -> RpcResult<ProcessResult> {
    let mut processor = StreamProcessor::new();
    let mut processed_count = 0;
    
    // Process items one by one without buffering
    while let Some(data_result) = input.next().await {
        let data = data_result?;
        
        // Process immediately and release memory
        processor.process_item(data).await?;
        processed_count += 1;
        
        // Periodic cleanup
        if processed_count % 1000 == 0 {
            processor.cleanup_memory().await;
        }
    }
    
    Ok(ProcessResult {
        processed_items: processed_count,
        final_state: processor.finalize().await?,
    })
}
```

### Connection Reuse

Use connection pooling for high-throughput scenarios:

```rust
use quiccrunch_core::rpc::RpcConnectionPool;

let pool = RpcConnectionPool::new(vec![
    transport.connect("server1:50051").await?,
    transport.connect("server2:50051").await?,
    transport.connect("server3:50051").await?,
]);

// Clients automatically use pooled connections
let client = MessageServiceClient::new(pool.get_client()?);
```

## Error Handling

### Stream Error Recovery

Handle errors gracefully without terminating the entire stream:

```rust
async fn resilient_stream(&self) -> RpcResult<RpcStreamSource<Result<Data, ProcessingError>>> {
    let (mut sink, source) = RpcStreamSink::create();
    
    tokio::spawn(async move {
        let mut data_source = UnreliableDataSource::new();
        
        while let Some(item_result) = data_source.next().await {
            let result = match item_result {
                Ok(item) => {
                    match process_item(item).await {
                        Ok(processed) => Ok(processed),
                        Err(e) => Err(ProcessingError::ProcessingFailed(e)),
                    }
                }
                Err(e) => Err(ProcessingError::SourceError(e)),
            };
            
            // Send result (success or error) to client
            if sink.send(result).await.is_err() {
                break;
            }
        }
        
        let _ = sink.finish().await;
    });
    
    Ok(source)
}
```

### Timeout Handling

Implement timeouts for streaming operations:

```rust
async fn timed_stream_processing(&self, mut input: RpcStreamSource<Data>) -> RpcResult<ProcessResult> {
    let timeout = Duration::from_secs(30);
    
    let result = tokio::time::timeout(timeout, async move {
        let mut processor = DataProcessor::new();
        
        while let Some(data) = input.next().await {
            processor.add(data?);
        }
        
        processor.finalize()
    }).await;
    
    match result {
        Ok(processor_result) => processor_result,
        Err(_) => Err(RpcError::Timeout),
    }
}
```

### Circuit Breaker Pattern

Protect against cascading failures:

```rust
struct StreamCircuitBreaker {
    failure_count: AtomicU32,
    last_failure: Mutex<Option<Instant>>,
    threshold: u32,
    timeout: Duration,
}

impl StreamCircuitBreaker {
    async fn call_with_circuit_breaker<T>(&self, operation: impl Future<Output = RpcResult<T>>) -> RpcResult<T> {
        // Check if circuit is open
        if self.is_open() {
            return Err(RpcError::ServiceUnavailable("Circuit breaker open".to_string()));
        }
        
        match operation.await {
            Ok(result) => {
                self.reset();
                Ok(result)
            }
            Err(e) => {
                self.record_failure();
                Err(e)
            }
        }
    }
    
    fn is_open(&self) -> bool {
        let failure_count = self.failure_count.load(Ordering::Relaxed);
        if failure_count >= self.threshold {
            if let Some(last_failure) = *self.last_failure.lock().unwrap() {
                return last_failure.elapsed() < self.timeout;
            }
        }
        false
    }
}
```

## Best Practices

### 1. Choose the Right Pattern

- **Unary**: Simple request/response, low latency required
- **Server streaming**: Real-time feeds, large data downloads
- **Client streaming**: Batch uploads, data ingestion
- **Bidirectional**: Interactive applications, real-time collaboration

### 2. Resource Management

```rust
// Always call finish() to properly close streams
async fn proper_cleanup(sink: RpcStreamSink<Data>) -> RpcResult<()> {
    // Send data...
    
    // Always finish the stream
    sink.finish().await?;
    Ok(())
}

// Use RAII for automatic cleanup
struct StreamManager {
    sink: Option<RpcStreamSink<Data>>,
}

impl Drop for StreamManager {
    fn drop(&mut self) {
        if let Some(sink) = self.sink.take() {
            // Spawn cleanup task since Drop can't be async
            tokio::spawn(async move {
                let _ = sink.finish().await;
            });
        }
    }
}
```

### 3. Performance Guidelines

- **Batch small messages** to reduce protocol overhead
- **Use appropriate buffer sizes** - too small = overhead, too large = memory usage
- **Implement backpressure** for slow consumers
- **Monitor stream metrics** - throughput, latency, error rates

### 4. Error Handling Strategy

```rust
// Handle errors per-message, not per-stream
while let Some(item_result) = stream.next().await {
    match item_result {
        Ok(item) => {
            // Process successfully received item
            process_item(item).await?;
        }
        Err(e) => {
            // Log error but continue processing
            tracing::warn!("Stream item error: {}", e);
            error_count += 1;
            
            // Bail out if too many errors
            if error_count > MAX_ERRORS {
                return Err(RpcError::ServiceError("Too many stream errors".to_string()));
            }
        }
    }
}
```

### 5. Security Considerations

```rust
// Implement rate limiting
struct RateLimiter {
    permits: Arc<Semaphore>,
    window: Duration,
}

impl RateLimiter {
    async fn acquire(&self) -> RpcResult<()> {
        match tokio::time::timeout(self.window, self.permits.acquire()).await {
            Ok(Ok(_permit)) => Ok(()),
            Ok(Err(_)) => Err(RpcError::ServiceUnavailable("Rate limit exceeded".to_string())),
            Err(_) => Err(RpcError::Timeout),
        }
    }
}

// Validate input size limits
async fn validate_stream_input(mut input: RpcStreamSource<Data>) -> RpcResult<Vec<Data>> {
    let mut items = Vec::new();
    let mut total_size = 0;
    const MAX_ITEMS: usize = 10000;
    const MAX_SIZE: usize = 100 * 1024 * 1024; // 100MB
    
    while let Some(data) = input.next().await {
        let data = data?;
        total_size += data.estimated_size();
        
        if items.len() >= MAX_ITEMS {
            return Err(RpcError::InvalidParameters("Too many items".to_string()));
        }
        
        if total_size >= MAX_SIZE {
            return Err(RpcError::InvalidParameters("Stream too large".to_string()));
        }
        
        items.push(data);
    }
    
    Ok(items)
}
```

## Next Steps

Now that you understand QuicCrunch's streaming patterns, explore these advanced topics:

- [Performance Optimization Guide](../optimization-guide.md)
- [Production Deployment](../deployment.md)
- [Monitoring and Observability](../monitoring.md)
- [Migration from gRPC](../migration/grpc.md)

For more examples, check out the [examples directory](../../quiccrunch-core/examples/) in the repository.