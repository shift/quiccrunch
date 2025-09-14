use std::time::Duration;
use bytes::Bytes;

use tokio::time;
#[cfg(feature = "backend-quinn")]
use quinn::{SendStream as QuinnSend, RecvStream as QuinnRecv};

#[cfg(feature = "telemetry")]
use tracing::{info, error};

#[cfg(feature = "telemetry")]
use {
    tracing::info_span,
};

use tokio::io::{AsyncRead, AsyncWrite};

/// A QUIC bidirectional stream for sending and receiving messages
#[derive(Debug, Clone)]
pub struct QuicStream {
    inner: StreamInner,
    max_message_size: usize,
}

#[derive(Debug, Clone)]
enum StreamInner {
    #[cfg(feature = "backend-quinn")]
    Quinn { send: QuinnSend, recv: QuinnRecv },
    #[cfg(feature = "lock-free-channels")]
    LockFree { handle: crate::lockfree::StreamHandle },
    // tQUIC implementation - simplified for now
    TQuic { stream_id: u64, connection_id: u64 },
}

impl QuicStream {
    /// Create a new QuicStream with connection and stream IDs (generic constructor)
    pub fn new(connection_id: u64, stream_id: u64, max_message_size: usize) -> Self {
        // Use the tQUIC backend as default when no lock-free channels
        #[cfg(all(feature = "backend-tquic", not(feature = "lock-free-channels")))]
        {
            Self::new_simple_tquic(connection_id, stream_id, max_message_size)
        }
        #[cfg(feature = "lock-free-channels")]
        {
            // Use lock-free implementation when available
            use crossbeam_channel::bounded;
            
            let (write_tx, write_rx) = bounded(1000);
            let (read_tx, read_rx) = bounded(1000);
            let (close_tx, close_rx) = bounded(100);
            
            let handle = crate::lockfree::StreamHandle::new(
                stream_id,
                connection_id,
                write_tx,
                read_tx,
                close_tx,
            );
            
            Self::new_lockfree(handle, max_message_size)
        }
        #[cfg(all(not(feature = "backend-tquic"), not(feature = "lock-free-channels")))]
        {
            // Fallback to tQUIC implementation
            Self::new_simple_tquic(connection_id, stream_id, max_message_size)
        }
    }

    /// Create a new QuicStream (Lock-free)
    #[cfg(feature = "lock-free-channels")]
    pub fn new_lockfree(handle: crate::lockfree::StreamHandle, max_message_size: usize) -> Self {
        Self { inner: StreamInner::LockFree { handle }, max_message_size }
    }
    
    /// Create a simple tQUIC stream
    pub fn new_simple_tquic(connection_id: u64, stream_id: u64, max_message_size: usize) -> Self {
        Self { 
            inner: StreamInner::TQuic { 
                stream_id, 
                connection_id 
            }, 
            max_message_size 
        }
    }

    /// Add methods for opening and accepting bidirectional streams
    pub async fn open_bi(&mut self) -> crate::Result<crate::QuicStream> {
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { .. } => {
                // For Quinn, we'd need access to the connection to open new streams
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(0, new_stream_id, self.max_message_size))
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                // Open a new stream through the connection handle
                let new_stream_id = rand::random::<u64>();
                let new_handle = crate::lockfree::StreamHandle::new(
                    new_stream_id,
                    handle.connection_id,
                    handle.write_tx.clone(),
                    handle.read_tx.clone(),
                    handle.close_tx.clone(),
                );
                Ok(QuicStream::new_lockfree(new_handle, self.max_message_size))
            }
            StreamInner::TQuic { stream_id: _, connection_id } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*connection_id, new_stream_id, self.max_message_size))
            }
        }
    }

    pub async fn accept_bi(&mut self) -> crate::Result<crate::QuicStream> {
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { .. } => {
                // For Quinn, we'd need access to the connection to accept new streams
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(0, new_stream_id, self.max_message_size))
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                // Accept a new stream through the connection handle  
                let new_stream_id = rand::random::<u64>();
                let new_handle = crate::lockfree::StreamHandle::new(
                    new_stream_id,
                    handle.connection_id,
                    handle.write_tx.clone(),
                    handle.read_tx.clone(),
                    handle.close_tx.clone(),
                );
                Ok(QuicStream::new_lockfree(new_handle, self.max_message_size))
            }
            StreamInner::TQuic { stream_id: _, connection_id } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*connection_id, new_stream_id, self.max_message_size))
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
                const CHUNK_SIZE: usize = 262_144; // 256KB chunks for optimal throughput
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
                // Combine header and data into single write to avoid fragmentation
                let mut combined = Vec::with_capacity(4 + data.len());
                combined.extend_from_slice(&header);
                combined.extend_from_slice(&data);
                
                handle.write(bytes::Bytes::from(combined), false).await?;
            }
            StreamInner::TQuic { stream_id, .. } => {
                // tQUIC implementation: store in global buffer for echo
                use dashmap::DashMap;
                use std::sync::Arc;
                
                lazy_static::lazy_static! {
                    static ref TQUIC_BUFFER: Arc<DashMap<u64, Vec<u8>>> = Arc::new(DashMap::new());
                }
                
                // Combine header and data for storage
                let mut combined = Vec::with_capacity(4 + data.len());
                combined.extend_from_slice(&header);
                combined.extend_from_slice(&data);
                
                TQUIC_BUFFER.insert(*stream_id, combined);
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
            StreamInner::TQuic { .. } => {
                // tQUIC implementation: nothing to close for this simplified version
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
                // Use lock-free read for optimal performance
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
            StreamInner::TQuic { stream_id, .. } => {
                // tQUIC implementation: Read from the global buffer
                use dashmap::DashMap;
                use std::sync::Arc;
                
                lazy_static::lazy_static! {
                    static ref TQUIC_BUFFER: Arc<DashMap<u64, Vec<u8>>> = Arc::new(DashMap::new());
                }
                
                // Try to read header from buffer
                if let Some(mut buffer_entry) = TQUIC_BUFFER.get_mut(stream_id) {
                    if buffer_entry.len() >= 4 {
                        header.copy_from_slice(&buffer_entry[..4]);
                        buffer_entry.drain(0..4); // Remove header from buffer
                    } else {
                        return Err(crate::TransportError::StreamError("No data available in tQUIC stream".into()));
                    }
                } else {
                    return Err(crate::TransportError::StreamError("tQUIC stream not found".into()));
                }
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
                    // Use lock-free read for optimal chunk reading performance
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
                StreamInner::TQuic { stream_id, .. } => {
                    // tQUIC implementation: Read chunk from buffer
                    use dashmap::DashMap;
                    use std::sync::Arc;
                    
                    lazy_static::lazy_static! {
                        static ref TQUIC_BUFFER: Arc<DashMap<u64, Vec<u8>>> = Arc::new(DashMap::new());
                    }
                    
                    // Try to read chunk from buffer
                    if let Some(mut buffer_entry) = TQUIC_BUFFER.get_mut(stream_id) {
                        if buffer_entry.len() >= chunk_size {
                            let chunk: Vec<u8> = buffer_entry.drain(0..chunk_size).collect();
                            buffer.extend_from_slice(&chunk);
                            bytes_read += chunk_size;
                        } else {
                            // Read remaining data if less than chunk_size
                            let remaining_data: Vec<u8> = buffer_entry.drain(..).collect();
                            if remaining_data.is_empty() {
                                return Err(crate::TransportError::StreamError("No more data in tQUIC stream".into()));
                            }
                            buffer.extend_from_slice(&remaining_data);
                            bytes_read += remaining_data.len();
                        }
                    } else {
                        return Err(crate::TransportError::StreamError("tQUIC stream buffer not found".into()));
                    }
                }
            }
        }

        #[cfg(feature = "telemetry")]
        info!(message_size = %msg_len, "Message received successfully");

        Ok(Bytes::from(buffer))
    }
}

impl AsyncRead for QuicStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // This is a placeholder implementation. A real implementation would need to handle the inner stream types.
        std::task::Poll::Pending
    }
}

impl AsyncWrite for QuicStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        _buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        // This is a placeholder implementation.
        std::task::Poll::Pending
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        // This is a placeholder implementation.
        std::task::Poll::Pending
    }

    fn poll_shutdown(self: std::pin::Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), std::io::Error>> {
        // This is a placeholder implementation.
        std::task::Poll::Pending
    }
}

impl Unpin for QuicStream {}

impl crate::transport::Splittable for QuicStream {
    type Reader = crate::stream_splitter::StreamReader;
    type Writer = crate::stream_splitter::StreamWriter;

    fn split(self) -> (Self::Reader, Self::Writer) {
        // This is a placeholder implementation.
        let split_stream = crate::stream_splitter::SplitStream::new(self);
        (crate::stream_splitter::StreamReader::new(split_stream.clone()), crate::stream_splitter::StreamWriter::new(split_stream))
    }
}