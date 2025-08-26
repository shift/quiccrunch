# Stream API Reference

The stream module provides QUIC stream handling capabilities for bidirectional communication.

## QuicStream

Represents a bidirectional QUIC stream for sending and receiving messages.

### Methods

#### `async fn send_message(&mut self, message: bytes::Bytes) -> Result<()>`

Sends a message over the stream.

Parameters:
- `message`: Message data as bytes

Returns:
- `Result<()>`: Success or error

Example:
```rust
stream.send_message(bytes::Bytes::from("Hello")).await?;
```

#### `async fn receive_message(&mut self) -> Result<bytes::Bytes>`

Receives a message from the stream.

Returns:
- `Result<bytes::Bytes>`: Received message or error

Example:
```rust
let message = stream.receive_message().await?;
println!("Received: {:?}", message);
```

### Stream Management

Streams are automatically managed by the QUIC connection:
- Streams are multiplexed over a single connection
- Flow control is handled automatically
- Streams are closed when dropped

## Best Practices

1. **Message Handling**
   - Use appropriate message sizes
   - Handle partial messages correctly
   - Implement message framing if needed
   - Use zero-copy when possible

2. **Error Handling**
   - Handle stream errors independently
   - Implement retry logic when appropriate
   - Clean up resources on errors

3. **Performance**
   - Reuse streams when possible
   - Batch messages when appropriate
   - Monitor stream performance
   - Use multiple streams for concurrent operations

4. **Resource Management**
   - Close streams when done
   - Don't keep unused streams open
   - Monitor stream usage

## Common Patterns

### Basic Send/Receive
```rust
let mut stream = connection.open_bi().await?;

// Send
stream.send_message(request_data).await?;

// Receive
let response = stream.receive_message().await?;
```

### Streaming Protocol
```rust
let mut stream = connection.open_bi().await?;

// Send multiple messages
for chunk in chunks {
    stream.send_message(chunk).await?;
}

// Receive multiple messages
while let Ok(message) = stream.receive_message().await {
    process_message(message);
}
```

### Error Handling
```rust
let mut stream = connection.open_bi().await?;

match stream.send_message(data).await {
    Ok(_) => println!("Message sent"),
    Err(e) => {
        match e {
            TransportError::StreamError(_) => {
                // Handle stream error
            }
            TransportError::ConnectionFailed(_) => {
                // Handle connection error
            }
            _ => {
                // Handle other errors
            }
        }
    }
}
```

### Concurrent Streams
```rust
let mut handles = vec![];

for _ in 0..10 {
    let stream = connection.open_bi().await?;
    handles.push(tokio::spawn(async move {
        // Use stream
    }));
}

futures::future::join_all(handles).await;
```