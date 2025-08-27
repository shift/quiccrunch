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
                const CHUNK_SIZE: usize = 32_768;
                let mut remaining = data.as_ref();
                while !remaining.is_empty() {
                    let chunk_size = remaining.len().min(CHUNK_SIZE);
                    let (chunk, rest) = remaining.split_at(chunk_size);
                    if let Err(e) = send.write_all(chunk).await { return Err(crate::TransportError::StreamError(e.to_string())); }
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
                const CHUNK_SIZE: usize = 32_768;
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
        let mut buffer = Vec::with_capacity(msg_len);
        let mut bytes_read = 0;

        // Read message content in chunks with flow control
        const CHUNK_SIZE: usize = 32_768; // 32KB chunks for better throughput
        
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
                            buffer.extend_from_slice(&data);
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

        Ok(Bytes::from(buffer))
    }
}