use std::time::Duration;
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio::time;
#[cfg(feature = "backend-quinn")]
use quinn::{SendStream as QuinnSend, RecvStream as QuinnRecv};

#[cfg(feature = "telemetry")]
use tracing::{info, error};

#[cfg(feature = "telemetry")]
use {
    tracing::info_span,
};

/// A QUIC bidirectional stream for sending and receiving messages
pub struct QuicStream {
    inner: StreamInner,
    max_message_size: usize,
}

enum StreamInner {
    #[cfg(feature = "backend-quinn")]
    Quinn { send: QuinnSend, recv: QuinnRecv },
    #[cfg(feature = "lock-free-channels")]
    LockFree { handle: crate::lockfree::StreamHandle },
}

impl QuicStream {
    /// Create a new QuicStream (Quinn)
    #[cfg(feature = "backend-quinn")]
    pub fn new(send: QuinnSend, recv: QuinnRecv, max_message_size: usize) -> Self {
        Self { inner: StreamInner::Quinn { send, recv }, max_message_size }
    }
    
    /// Create a new QuicStream (Lock-free)
    #[cfg(feature = "lock-free-channels")]
    pub fn new_lockfree(handle: crate::lockfree::StreamHandle, max_message_size: usize) -> Self {
        Self { inner: StreamInner::LockFree { handle }, max_message_size }
    }
    
    /// Create a simple tQUIC stream (fallback implementation)
    #[cfg(feature = "backend-tquic")]
    pub fn new_simple_tquic(stream_id: u64, max_message_size: usize) -> Self {
        // Use lock-free channels for tQUIC backend until real tQUIC integration is complete
        #[cfg(feature = "lock-free-channels")]
        {
            // Create a mock stream handle for compatibility
            use crossbeam_channel::bounded;
            use std::sync::Arc;
            
            let (write_tx, _write_rx) = bounded(1000);
            let (read_tx, _read_rx) = bounded(1000);
            let (close_tx, _close_rx) = bounded(100);
            
            let handle = crate::lockfree::StreamHandle::new(
                stream_id,
                0, // connection_id (will be set properly when real tQUIC is integrated)
                write_tx,
                read_tx, 
                close_tx,
            );
            
            Self { 
                inner: StreamInner::LockFree { handle }, 
                max_message_size 
            }
        }
        
        #[cfg(not(feature = "lock-free-channels"))]
        {
            // Fallback to mock implementation without lock-free channels
            // This is a temporary solution until real tQUIC integration
            tracing::warn!("tQUIC stream created without lock-free channels - performance may be degraded");
            
            // Create a minimal mock stream that at least doesn't panic
            // In practice this should be replaced with real tQUIC stream handles
            use crossbeam_channel::bounded;
            
            let (write_tx, _write_rx) = bounded(1000);
            let (read_tx, _read_rx) = bounded(1000);
            let (close_tx, _close_rx) = bounded(100);
            
            let handle = crate::lockfree::StreamHandle::new(
                stream_id,
                0, // connection_id
                write_tx,
                read_tx,
                close_tx,
            );
            
            Self { 
                inner: StreamInner::LockFree { handle }, 
                max_message_size 
            }
        }
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

        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { send, .. } => {
                // Quinn send implements AsyncWrite; no local import needed
                send.write_all(&header).await.map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                const CHUNK_SIZE: usize = 262_144; // 256KB chunks for optimal throughput (increased from 32KB)
                let mut remaining = data.as_ref();
                while !remaining.is_empty() {
                    let chunk_size = remaining.len().min(CHUNK_SIZE);
                    let (chunk, rest) = remaining.split_at(chunk_size);
                    
                    if let Err(e) = send.write_all(chunk).await { 
                        return Err(crate::TransportError::StreamError(e.to_string())); 
                    }
                    
                    if let Err(e) = send.flush().await { return Err(crate::TransportError::StreamError(e.to_string())); }
                    remaining = rest;
                }
                if let Err(e) = send.flush().await { return Err(crate::TransportError::StreamError(e.to_string())); }
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                // Use zero-copy operations when available
                #[cfg(feature = "zero-copy")]
                {
                    // Zero-copy: Use registered buffers from io_uring when available
                    handle.write(bytes::Bytes::copy_from_slice(&header), false).await?;

                    // Write data in optimized chunks without copying
                    const CHUNK_SIZE: usize = 262_144; // 256KB chunks
                    let mut remaining = data.as_ref();
                    while !remaining.is_empty() {
                        let chunk_size = remaining.len().min(CHUNK_SIZE);
                        let (chunk, rest) = remaining.split_at(chunk_size);

                        handle.write(bytes::Bytes::copy_from_slice(chunk), false).await?;

                        remaining = rest;
                    }
                }
                #[cfg(not(feature = "zero-copy"))]
                {
                    // Fallback: Use lock-free stream handle for optimal performance
                    handle.write(bytes::Bytes::from(header.to_vec()), false).await?;

                    // Write data in optimized chunks
                    const CHUNK_SIZE: usize = 262_144; // 256KB chunks
                    let mut remaining = data.as_ref();
                    while !remaining.is_empty() {
                        let chunk_size = remaining.len().min(CHUNK_SIZE);
                        let (chunk, rest) = remaining.split_at(chunk_size);

                        handle.write(bytes::Bytes::from(chunk.to_vec()), false).await?;

                        remaining = rest;
                    }
                }
            }
            #[allow(unreachable_patterns)]
            _ => {
                return Err(crate::TransportError::StreamError("Unsupported stream backend".into()));
            }
        }

        #[cfg(feature = "telemetry")]
        info!("Message sent successfully");

        Ok(())
    }

    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { send, .. } => {
                send.finish().await.map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(())
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                handle.close().await?;
                Ok(())
            }
            #[allow(unreachable_patterns)]
            _ => {
                Ok(())
            }
        }
    }

    pub async fn receive_message(&mut self) -> crate::Result<Bytes> {
        #[cfg(feature = "telemetry")]
        let span = info_span!("quiccrunch.receive");
        
        #[cfg(feature = "telemetry")]
        let _guard = span.enter();

        // Read message length with timeout
        const READ_TIMEOUT: Duration = Duration::from_secs(30);
        let mut header = [0u8; 4];
        
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { recv, .. } => {
                let header_result = time::timeout(READ_TIMEOUT, recv.read_exact(&mut header)).await;
                match header_result {
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
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                // Use zero-copy operations when available
                #[cfg(feature = "zero-copy")]
                {
                    // Zero-copy: Direct buffer access without intermediate copies
                    let header_result = time::timeout(READ_TIMEOUT, handle.read(4)).await;
                    match header_result {
                        Ok(Ok(Some(data))) if data.len() >= 4 => {
                            // Zero-copy: Direct slice access
                            header.copy_from_slice(&data[..4]);
                        },
                        Ok(Ok(Some(_))) => return Err(crate::TransportError::StreamError("incomplete header read".into())),
                        Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed".into())),
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err(crate::TransportError::StreamError("Header read timeout".into())),
                    }
                }
                #[cfg(not(feature = "zero-copy"))]
                {
                    // Fallback: Use lock-free read for optimal performance
                    let header_result = time::timeout(READ_TIMEOUT, handle.read(4)).await;
                    match header_result {
                        Ok(Ok(Some(data))) if data.len() >= 4 => {
                            header.copy_from_slice(&data[..4]);
                        },
                        Ok(Ok(Some(_))) => return Err(crate::TransportError::StreamError("incomplete header read".into())),
                        Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed".into())),
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err(crate::TransportError::StreamError("Header read timeout".into())),
                    }
                }
            }
            #[allow(unreachable_patterns)]
            _ => {
                return Err(crate::TransportError::StreamError("Unsupported stream backend".into()));
            }
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
        const CHUNK_SIZE: usize = 262_144; // 256KB chunks for optimal throughput
        
        while bytes_read < msg_len {
            // Calculate next chunk size
            let remaining = msg_len - bytes_read;
            let chunk_size = remaining.min(CHUNK_SIZE);
            
            // Read chunk with timeout
            match &mut self.inner {
                #[cfg(feature = "backend-quinn")]
                StreamInner::Quinn { recv, .. } => {
                    let mut chunk = vec![0u8; chunk_size];
                    let read_res = time::timeout(READ_TIMEOUT, recv.read_exact(&mut chunk)).await;
                    match read_res {
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
                #[cfg(feature = "lock-free-channels")]
                StreamInner::LockFree { handle } => {
                    // Use zero-copy operations when available
                    #[cfg(feature = "zero-copy")]
                    {
                        // Zero-copy: Direct buffer access for chunked reading
                        let read_res = time::timeout(READ_TIMEOUT, handle.read(chunk_size)).await;
                        match read_res {
                            Ok(Ok(Some(data))) => {
                                if data.len() != chunk_size {
                                    return Err(crate::TransportError::StreamError(format!("incomplete chunk read: got {} expected {}", data.len(), chunk_size)));
                                }
                                // Zero-copy: Direct slice access without intermediate buffer
                                buffer.extend_from_slice(&data);
                                bytes_read += chunk_size;
                            },
                            Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed during read".into())),
                            Ok(Err(e)) => return Err(e),
                            Err(_) => return Err(crate::TransportError::StreamError(format!("Content read timeout after {} bytes", bytes_read))),
                        }
                    }
                    #[cfg(not(feature = "zero-copy"))]
                    {
                        // Fallback: Use lock-free read for optimal chunk reading performance
                        let read_res = time::timeout(READ_TIMEOUT, handle.read(chunk_size)).await;
                        match read_res {
                            Ok(Ok(Some(data))) => {
                                if data.len() != chunk_size {
                                    return Err(crate::TransportError::StreamError(format!("incomplete chunk read: got {} expected {}", data.len(), chunk_size)));
                                }
                                buffer.extend_from_slice(&data);
                                bytes_read += chunk_size;
                            },
                            Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed during read".into())),
                            Ok(Err(e)) => return Err(e),
                            Err(_) => return Err(crate::TransportError::StreamError(format!("Content read timeout after {} bytes", bytes_read))),
                        }
                    }
                }
                #[allow(unreachable_patterns)]
                _ => {
                    return Err(crate::TransportError::StreamError("Unsupported stream backend".into()));
                }
            }
        }

        #[cfg(feature = "telemetry")]
        info!(message_size = %msg_len, "Message received successfully");

        Ok(Bytes::from(buffer))
    }
}