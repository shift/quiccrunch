use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::{time};
use bytes::Bytes;
#[cfg(feature = "backend-quinn")]
use quinn::{SendStream as QuinnSend, RecvStream as QuinnRecv};
#[cfg(feature = "backend-tquic")]
#[derive(Clone, Debug)]
pub struct TquicStreamHandle {
    stream_id: u64,
    // Store a clone of the command channel to send stream operations
    cmd_tx: tokio::sync::mpsc::Sender<crate::transport::tquic::StreamCmd>,
}

#[cfg(feature = "backend-tquic")]
impl TquicStreamHandle {
    pub fn new(stream_id: u64, cmd_tx: tokio::sync::mpsc::Sender<crate::transport::tquic::StreamCmd>) -> Self {
        Self { stream_id, cmd_tx }
    }
}

#[cfg(feature = "backend-tquic")]
pub type TquicSend = TquicStreamHandle;
#[cfg(feature = "backend-tquic")]
pub type TquicRecv = TquicStreamHandle;
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
    #[cfg(feature = "backend-tquic")]
    Tquic { send: TquicSend, recv: TquicRecv },
    #[cfg(feature = "lock-free-channels")]
    LockFree { handle: crate::lockfree::StreamHandle },
}

impl QuicStream {
    #[cfg(all(not(feature = "backend-tquic"), feature = "backend-quinn"))]
    pub fn new_tquic<_S, _R>(_send: _S, _recv: _R, _max_message_size: usize) -> Self {
        // Fallback to placeholder; should not be called when tquic disabled
        unimplemented!("tquic backend not enabled")
    }
    /// Create a new QuicStream (Quinn)
    #[cfg(feature = "backend-quinn")]
    pub fn new(send: QuinnSend, recv: QuinnRecv, max_message_size: usize) -> Self {
        Self { inner: StreamInner::Quinn { send, recv }, max_message_size }
    }

    /// Create a new QuicStream (tQUIC)
    #[cfg(feature = "backend-tquic")]
    pub fn new_tquic(send: TquicSend, recv: TquicRecv, max_message_size: usize) -> Self {
        Self { inner: StreamInner::Tquic { send, recv }, max_message_size }
    }
    
    /// Create a new QuicStream (Lock-free)
    #[cfg(feature = "lock-free-channels")]
    pub fn new_lockfree(handle: crate::lockfree::StreamHandle, max_message_size: usize) -> Self {
        Self { inner: StreamInner::LockFree { handle }, max_message_size }
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

        // helper removed; wrote directly to streams to avoid unused warnings

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
                    
                    // Use SIMD-optimized sending if enabled
                    #[cfg(feature = "simd-optimizations")]
                    {
                        if chunk_size >= 32 {
                            // For large chunks, use memory pool for temporary buffer
                            #[cfg(feature = "memory-pools")]
                            {
                                let mut pooled_chunk = crate::memory::PooledBuffer::get(chunk_size);
                                pooled_chunk.resize(chunk_size, 0);
                                let features = crate::simd::CpuFeatures::detect();
                                if features.avx2 {
                                    crate::simd::fast_copy(chunk, pooled_chunk.as_mut_slice());
                                    if let Err(e) = send.write_all(pooled_chunk.as_slice()).await { 
                                        return Err(crate::TransportError::StreamError(e.to_string())); 
                                    }
                                } else {
                                    if let Err(e) = send.write_all(chunk).await { 
                                        return Err(crate::TransportError::StreamError(e.to_string())); 
                                    }
                                }
                            }
                            #[cfg(not(feature = "memory-pools"))]
                            {
                                if let Err(e) = send.write_all(chunk).await { 
                                    return Err(crate::TransportError::StreamError(e.to_string())); 
                                }
                            }
                        } else {
                            if let Err(e) = send.write_all(chunk).await { 
                                return Err(crate::TransportError::StreamError(e.to_string())); 
                            }
                        }
                    }
                    #[cfg(not(feature = "simd-optimizations"))]
                    {
                        if let Err(e) = send.write_all(chunk).await { 
                            return Err(crate::TransportError::StreamError(e.to_string())); 
                        }
                    }
                    
                    if let Err(e) = send.flush().await { return Err(crate::TransportError::StreamError(e.to_string())); }
                    remaining = rest;
                }
                if let Err(e) = send.flush().await { return Err(crate::TransportError::StreamError(e.to_string())); }
            }
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { send, .. } => {
                // For tquic, send write command through the stream handle
                let (tx, rx) = tokio::sync::oneshot::channel();
                if let Err(_) = send.cmd_tx.send(crate::transport::tquic::StreamCmd::Write { 
                    stream_id: send.stream_id, 
                    data: bytes::Bytes::from(header.to_vec()),
                    fin: false,
                    rsp: tx 
                }).await {
                    return Err(crate::TransportError::StreamError("tquic write channel closed".into()));
                }
                let _ = rx.await.map_err(|_| crate::TransportError::StreamError("tquic write canceled".into()))??;

                // Write data in chunks
                const CHUNK_SIZE: usize = 262_144; // 256KB chunks for optimal throughput (increased from 32KB)
                let mut remaining = data.as_ref();
                while !remaining.is_empty() {
                    let chunk_size = remaining.len().min(CHUNK_SIZE);
                    let (chunk, rest) = remaining.split_at(chunk_size);
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if let Err(_) = send.cmd_tx.send(crate::transport::tquic::StreamCmd::Write { 
                        stream_id: send.stream_id, 
                        data: bytes::Bytes::from(chunk.to_vec()),
                        fin: false,
                        rsp: tx 
                    }).await {
                        return Err(crate::TransportError::StreamError("tquic write channel closed".into()));
                    }
                    let _ = rx.await.map_err(|_| crate::TransportError::StreamError("tquic write canceled".into()))??;
                    remaining = rest;
                }
            }
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                // Use lock-free stream handle for optimal performance
                handle.write(bytes::Bytes::from(header.to_vec()), false).await?;
                
                // Write data in optimized chunks using lock-free channels
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

        #[cfg(feature = "telemetry")]
        info!("Message sent successfully");

        Ok(())
    }

    /// Receive a message from the stream with flow control
    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "backend-quinn")]
            StreamInner::Quinn { send, .. } => {
                send.finish().await.map_err(|e| crate::TransportError::StreamError(e.to_string()))?;
                Ok(())
            }
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { send, .. } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if let Err(_) = send.cmd_tx.send(crate::transport::tquic::StreamCmd::Shutdown { 
                    stream_id: send.stream_id, 
                    rsp: tx 
                }).await {
                    return Err(crate::TransportError::StreamError("tquic stream shutdown channel closed".into()));
                }
                rx.await.map_err(|_| crate::TransportError::StreamError("tquic shutdown canceled".into()))??;
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
            #[cfg(feature = "backend-tquic")]
            StreamInner::Tquic { recv, .. } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if let Err(_) = recv.cmd_tx.send(crate::transport::tquic::StreamCmd::Read { 
                    stream_id: recv.stream_id, 
                    len: 4,
                    rsp: tx 
                }).await {
                    return Err(crate::TransportError::StreamError("tquic read channel closed".into()));
                }
                let header_result = time::timeout(READ_TIMEOUT, rx).await;
                match header_result {
                    Ok(Ok(Ok(Some(mut data)))) if data.len() >= 4 => {
                        header.copy_from_slice(&data[..4]);
                    },
                    Ok(Ok(Ok(None))) => return Err(crate::TransportError::StreamError("tquic stream closed".into())),
                    Ok(Ok(Err(e))) => return Err(e),
                    Ok(Err(_)) => return Err(crate::TransportError::StreamError("tquic read canceled".into())),
                    Err(_) => return Err(crate::TransportError::StreamError("Header read timeout".into())),
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
                    Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed".into())),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => return Err(crate::TransportError::StreamError("Header read timeout".into())),
                }
            }
                let header_result = time::timeout(READ_TIMEOUT, rx).await;
                match header_result {
                    Ok(Ok(Ok(Some(data)))) => {
                        if data.len() != 4 {
                            return Err(crate::TransportError::StreamError("incomplete header read".into()));
                        }
                        header.copy_from_slice(&data[..4]);
                    }
                    Ok(Ok(Ok(None))) => {
                        return Err(crate::TransportError::StreamError("stream closed during header read".into()));
                    }
                    Ok(Ok(Err(e))) => return Err(e),
                    Ok(Err(_)) => return Err(crate::TransportError::StreamError("tquic read canceled".into())),
                    Err(_) => return Err(crate::TransportError::StreamError("Header read timeout".into())),
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
        // Use memory pool for zero-allocation buffer management
        #[cfg(feature = "memory-pools")]
        let mut pooled_buffer = crate::memory::PooledBuffer::get(msg_len);
        #[cfg(not(feature = "memory-pools"))]
        let mut buffer = Vec::with_capacity(msg_len);
        let mut bytes_read = 0;

        // Read message content in chunks with flow control
        const CHUNK_SIZE: usize = 262_144; // 256KB chunks for optimal throughput (increased from 32KB)
        
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
                            #[cfg(feature = "memory-pools")]
                            {
                                // Use SIMD-optimized copy if available
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && chunk.len() >= 32 {
                                        // Resize buffer to accommodate new data
                                        let current_len = pooled_buffer.len();
                                        pooled_buffer.resize(current_len + chunk_size, 0);
                                        crate::simd::fast_copy(&chunk, &mut pooled_buffer.as_mut_slice()[current_len..current_len + chunk_size]);
                                    } else {
                                        pooled_buffer.extend_from_slice(&chunk);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                pooled_buffer.extend_from_slice(&chunk);
                            }
                            #[cfg(not(feature = "memory-pools"))]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && chunk.len() >= 32 {
                                        // For Vec, we still benefit from SIMD copies
                                        buffer.resize(bytes_read + chunk_size, 0);
                                        crate::simd::fast_copy(&chunk, &mut buffer[bytes_read..bytes_read + chunk_size]);
                                    } else {
                                        buffer.extend_from_slice(&chunk);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                buffer.extend_from_slice(&chunk);
                            }
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
                #[cfg(feature = "backend-tquic")]
                StreamInner::Tquic { recv, .. } => {
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    if let Err(_) = recv.cmd_tx.send(crate::transport::tquic::StreamCmd::Read { 
                        stream_id: recv.stream_id, 
                        len: chunk_size,
                        rsp: tx 
                    }).await {
                        return Err(crate::TransportError::StreamError("tquic read channel closed".into()));
                    }
                    let read_res = time::timeout(READ_TIMEOUT, rx).await;
                    match read_res {
                        Ok(Ok(Ok(Some(data)))) => {
                            if data.len() != chunk_size {
                                return Err(crate::TransportError::StreamError(format!("incomplete chunk read: got {} expected {}", data.len(), chunk_size)));
                            }
                            #[cfg(feature = "memory-pools")]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        let current_len = pooled_buffer.len();
                                        pooled_buffer.resize(current_len + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut pooled_buffer.as_mut_slice()[current_len..current_len + chunk_size]);
                                    } else {
                                        pooled_buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                pooled_buffer.extend_from_slice(&data);
                            }
                            #[cfg(not(feature = "memory-pools"))]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        buffer.resize(bytes_read + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut buffer[bytes_read..bytes_read + chunk_size]);
                                    } else {
                                        buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                buffer.extend_from_slice(&data);
                            }
                            bytes_read += chunk_size;
                        },
                        Ok(Ok(Ok(None))) => return Err(crate::TransportError::StreamError("tquic stream closed during read".into())),
                        Ok(Ok(Err(e))) => return Err(e),
                        Ok(Err(_)) => return Err(crate::TransportError::StreamError("tquic read canceled".into())),
                        Err(_) => return Err(crate::TransportError::StreamError(format!("Content read timeout after {} bytes", bytes_read))),
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
                            #[cfg(feature = "memory-pools")]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        let current_len = pooled_buffer.len();
                                        pooled_buffer.resize(current_len + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut pooled_buffer.as_mut_slice()[current_len..current_len + chunk_size]);
                                    } else {
                                        pooled_buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                pooled_buffer.extend_from_slice(&data);
                            }
                            #[cfg(not(feature = "memory-pools"))]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        buffer.resize(bytes_read + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut buffer[bytes_read..bytes_read + chunk_size]);
                                    } else {
                                        buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                buffer.extend_from_slice(&data);
                            }
                            bytes_read += chunk_size;
                        },
                        Ok(Ok(None)) => return Err(crate::TransportError::StreamError("lock-free stream closed during read".into())),
                        Ok(Err(e)) => return Err(e),
                        Err(_) => return Err(crate::TransportError::StreamError(format!("Content read timeout after {} bytes", bytes_read))),
                    }
                }
                    let read_res = time::timeout(READ_TIMEOUT, rx).await;
                    match read_res {
                        Ok(Ok(Ok(Some(data)))) => {
                            if data.len() != chunk_size {
                                return Err(crate::TransportError::StreamError(format!("incomplete chunk read: got {} expected {}", data.len(), chunk_size)));
                            }
                            #[cfg(feature = "memory-pools")]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        let current_len = pooled_buffer.len();
                                        pooled_buffer.resize(current_len + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut pooled_buffer.as_mut_slice()[current_len..current_len + chunk_size]);
                                    } else {
                                        pooled_buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                pooled_buffer.extend_from_slice(&data);
                            }
                            #[cfg(not(feature = "memory-pools"))]
                            {
                                #[cfg(feature = "simd-optimizations")]
                                {
                                    let features = crate::simd::CpuFeatures::detect();
                                    if features.avx2 && data.len() >= 32 {
                                        buffer.resize(bytes_read + chunk_size, 0);
                                        crate::simd::fast_copy(&data, &mut buffer[bytes_read..bytes_read + chunk_size]);
                                    } else {
                                        buffer.extend_from_slice(&data);
                                    }
                                }
                                #[cfg(not(feature = "simd-optimizations"))]
                                buffer.extend_from_slice(&data);
                            }
                            bytes_read += chunk_size;
                        },
                        Ok(Ok(Ok(None))) => {
                            return Err(crate::TransportError::StreamError("stream closed during read".into()));
                        },
                        Ok(Ok(Err(e))) => return Err(e),
                        Ok(Err(_)) => return Err(crate::TransportError::StreamError("tquic read canceled".into())),
                        Err(_) => return Err(crate::TransportError::StreamError(format!("Content read timeout after {} bytes", bytes_read))),
                    }
                }
            }
        }

        #[cfg(feature = "telemetry")]
        info!(message_size = %msg_len, "Message received successfully");

        {
            #[cfg(feature = "memory-pools")]
            {
                Ok(Bytes::from(pooled_buffer.into_vec()))
            }
            #[cfg(not(feature = "memory-pools"))]
            {
                Ok(Bytes::from(buffer))
            }
        }
    }
}