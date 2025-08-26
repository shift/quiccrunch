use std::time::Duration;
use tokio::{time, io::AsyncWriteExt};
use quinn::{SendStream, RecvStream};
use bytes::Bytes;
#[cfg(feature = "telemetry")]
use tracing::{info, error};

#[cfg(feature = "telemetry")]
use {
    opentelemetry::{
        trace::{FutureExt, Span, Tracer, TracerProvider},
        Context,
    },
    tracing::info_span,
};

/// A QUIC bidirectional stream for sending and receiving messages
pub struct QuicStream {
    send: SendStream,
    recv: RecvStream,
    max_message_size: usize,
}

impl QuicStream {
    /// Create a new QuicStream
    pub fn new(send: SendStream, recv: RecvStream, max_message_size: usize) -> Self {
        Self { send, recv, max_message_size }
    }

    /// Send a message over the stream with flow control
    pub async fn send_message(&mut self, data: Bytes) -> crate::Result<()> {
        // Enforce application-level max message size (aligned with stream_receive_window)
        if data.len() > self.max_message_size {
            return Err(crate::TransportError::StreamError(format!(
                "Message size {} exceeds limit {}", data.len(), self.max_message_size
            )));
        }
        #[cfg(feature = "telemetry")]
        let span = info_span!("quiccrunch.send", message_size = %data.len());
        
        #[cfg(feature = "telemetry")]
        let _guard = span.enter();

        // Write message length as header
        let len = data.len() as u32;
        let header = len.to_le_bytes();
        self.send.write_all(&header).await
            .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;

        // Initialize chunked transfer
        const CHUNK_SIZE: usize = 32_768; // 32KB chunks for better throughput
        let mut remaining = data.as_ref();

        // Transfer data in chunks with flow control
        while !remaining.is_empty() {
            // Calculate next chunk size
            let chunk_size = remaining.len().min(CHUNK_SIZE);
            let (chunk, rest) = remaining.split_at(chunk_size);
            
            // Write chunk and handle backpressure
            match self.send.write_all(chunk).await {
                Ok(_) => {
                    remaining = rest;
                    // Flush after each chunk to maintain flow control
                    self.send.flush().await
                        .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                },
                Err(e) => {
                    // Handle write error
                    #[cfg(feature = "telemetry")]
                    error!(?e, "Write error during chunked transfer");
                    return Err(crate::TransportError::StreamError(e.to_string()));
                }
            }
        }

        // Ensure all data is flushed (do not finish the stream to allow multiple messages)
        self.send.flush().await
            .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;

        #[cfg(feature = "telemetry")]
        info!("Message sent successfully");

        Ok(())
    }

    /// Receive a message from the stream with flow control
    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        self.send.finish().await
            .map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
        Ok(())
    }

    pub async fn receive_message(&mut self) -> crate::Result<Bytes> {
        #[cfg(feature = "telemetry")]
        let span = info_span!("quiccrunch.receive");
        
        #[cfg(feature = "telemetry")]
        let _guard = span.enter();

        // Read message length with timeout
        const READ_TIMEOUT: Duration = Duration::from_secs(30);
        let mut header = [0u8; 4];
        
        match time::timeout(READ_TIMEOUT, self.recv.read_exact(&mut header)).await {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => {
                #[cfg(feature = "telemetry")]
                error!(?e, "Failed to read message header");
                return Err(crate::TransportError::StreamError(format!("Header read error: {}", e)));
            },
            Err(_) => {
                #[cfg(feature = "telemetry")]
                error!("Header read timeout after {} seconds", READ_TIMEOUT.as_secs());
                return Err(crate::TransportError::StreamError("Header read timeout".into()));
            },
        }

        // Parse message length and prepare buffer
        let msg_len = u32::from_le_bytes(header) as usize;
        if msg_len > self.max_message_size {
            return Err(crate::TransportError::StreamError(format!(
                "Announced message size {} exceeds limit {}", msg_len, self.max_message_size
            )));
        }
        let mut buffer = Vec::with_capacity(msg_len);
        let mut bytes_read = 0;

        // Read message content in chunks with flow control
        const CHUNK_SIZE: usize = 32_768; // 32KB chunks for better throughput
        
        while bytes_read < msg_len {
            // Calculate next chunk size
            let remaining = msg_len - bytes_read;
            let chunk_size = remaining.min(CHUNK_SIZE);
            let mut chunk = vec![0u8; chunk_size];

            // Read chunk with timeout
            match time::timeout(READ_TIMEOUT, self.recv.read_exact(&mut chunk)).await {
                Ok(Ok(_)) => {
                    buffer.extend_from_slice(&chunk);
                    bytes_read += chunk_size;
                    
                    #[cfg(feature = "telemetry")]
                    info!(
                        progress = %(bytes_read as f32 / msg_len as f32 * 100.0),
                        "Read progress"
                    );
                },
                Ok(Err(e)) => {
                    #[cfg(feature = "telemetry")]
                    error!(?e, bytes_read, "Content read error");
                    return Err(crate::TransportError::StreamError(format!(
                        "Content read error at {} bytes: {}", bytes_read, e
                    )));
                },
                Err(_) => {
                    #[cfg(feature = "telemetry")]
                    error!(bytes_read, "Content read timeout");
                    return Err(crate::TransportError::StreamError(format!(
                        "Content read timeout after {} bytes", bytes_read
                    )));
                },
            }
        }

        #[cfg(feature = "telemetry")]
        info!(message_size = %msg_len, "Message received successfully");

        Ok(Bytes::from(buffer))
    }
}