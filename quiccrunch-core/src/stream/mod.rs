// This file implements QUIC bidirectional stream logic for quiccrunch-core.
// It provides abstractions for lock-free and tQUIC backends.


/// A QUIC bidirectional stream for sending and receiving messages
#[derive(Debug)]
pub struct QuicStream {
    inner: StreamInner,
    max_message_size: usize,
}

#[derive(Debug)]
enum StreamInner {
    #[cfg(feature = "lock-free-channels")]
    LockFree {
        handle: crate::lockfree::StreamHandle,
    },
    // tQUIC implementation - channel-based for async bridge
    TQuic {
        command_tx: tokio::sync::mpsc::Sender<crate::transport::async_bridge::TquicCommand>,
        stream_id: u64,
        pending_read: Option<tokio::sync::oneshot::Receiver<std::io::Result<Vec<u8>>>>,
        pending_write: Option<tokio::sync::oneshot::Receiver<std::io::Result<()>>>,
    },
}

impl QuicStream {
    /// Create a new QuicStream with connection and stream IDs (generic constructor)
    pub fn new(connection_id: u64, stream_id: u64, max_message_size: usize) -> Self {
        // Use the tQUIC backend as default when no lock-free channels
        #[cfg(all(feature = "backend-tquic", not(feature = "lock-free-channels")))]
        {
            Self::new_simple_tquic(connection_id, stream_id, max_message_size)
        }

        // Use lock-free channels if available
        #[cfg(feature = "lock-free-channels")]
        {
            // Create a dummy handle for now - should be replaced with actual connection logic
            let handle = crate::lockfree::StreamHandle::new(
                stream_id,
                connection_id,
                tokio::sync::mpsc::channel(1024).0, // write_tx
                tokio::sync::mpsc::channel(1024).0, // read_tx
                tokio::sync::mpsc::channel(1).0,    // close_tx
            );
            Self::new_lockfree(handle, max_message_size)
        }

        // Fallback to simple tQUIC
        #[cfg(not(any(feature = "lock-free-channels", feature = "backend-tquic")))]
        {
            Self::new_simple_tquic(connection_id, stream_id, max_message_size)
        }
    }

    #[cfg(feature = "lock-free-channels")]
    pub fn new_lockfree(
        handle: crate::lockfree::StreamHandle,
        max_message_size: usize,
    ) -> Self {
        Self {
            inner: StreamInner::LockFree { handle },
            max_message_size,
        }
    }

    /// Create a simple tQUIC stream
    pub fn new_simple_tquic(_connection_id: u64, stream_id: u64, max_message_size: usize) -> Self {
        // Create a dummy command channel for now
        let (command_tx, _command_rx) = tokio::sync::mpsc::channel(32);
        
        Self {
            inner: StreamInner::TQuic {
                command_tx,
                stream_id,
                pending_read: None,
                pending_write: None,
            },
            max_message_size,
        }
    }

    /// Add methods for opening and accepting bidirectional streams
    pub async fn open_bi(&mut self) -> crate::Result<crate::QuicStream> {
        match &mut self.inner {
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
            StreamInner::TQuic { .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(
                    0, // connection_id placeholder
                    new_stream_id,
                    self.max_message_size,
                ))
            }
        }
    }

    pub async fn accept_bi(&mut self) -> crate::Result<crate::QuicStream> {
        match &mut self.inner {
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
            StreamInner::TQuic { .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(
                    0, // connection_id placeholder
                    new_stream_id,
                    self.max_message_size,
                ))
            }
        }
    }

    /// Send data over the stream
    pub async fn send(&mut self, data: &[u8]) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                let bytes = Bytes::copy_from_slice(data);
                handle
                    .write_tx
                    .send(bytes)
                    .await
                    .map_err(|_| crate::Error::ConnectionFailed("Write channel closed".into()))?;
                Ok(())
            }
            StreamInner::TQuic { .. } => {
                // TODO: Implement actual tQUIC sending
                // For now, just simulate success
                Ok(())
            }
        }
    }

    /// Receive data from the stream
    pub async fn recv(&mut self) -> crate::Result<Vec<u8>> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                let bytes = handle
                    .read_tx
                    .recv()
                    .await
                    .ok_or_else(|| crate::Error::ConnectionFailed("Read channel closed".into()))?;
                Ok(bytes.to_vec())
            }
            StreamInner::TQuic { .. } => {
                // TODO: Implement actual tQUIC receiving
                // For now, return empty data
                Ok(Vec::new())
            }
        }
    }

    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                handle
                    .close_tx
                    .send(())
                    .await
                    .map_err(|_| crate::Error::ConnectionFailed("Close channel failed".into()))?;
                Ok(())
            }
            StreamInner::TQuic { .. } => {
                // TODO: Implement actual tQUIC close
                Ok(())
            }
        }
    }
}

// Reader half of a split QuicStream
pub struct QuicStreamReader {
    _inner: StreamInner,
}

// Writer half of a split QuicStream  
pub struct QuicStreamWriter {
    _inner: StreamInner,
}

impl crate::transport::Splittable for QuicStream {
    type Reader = QuicStreamReader;
    type Writer = QuicStreamWriter;
    
    fn split(self) -> (Self::Reader, Self::Writer) {
        // For now, create simple wrapper structs
        // In a full implementation, these would properly split the stream
        let reader = QuicStreamReader {
            _inner: self.inner,
        };
        let writer = QuicStreamWriter {
            _inner: StreamInner::TQuic {
                command_tx: tokio::sync::mpsc::channel(32).0,
                stream_id: 0,
                pending_read: None,
                pending_write: None,
            },
        };
        (reader, writer)
    }
}

impl tokio::io::AsyncRead for QuicStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { .. } => {
                // TODO: Implement proper async read
                std::task::Poll::Ready(Ok(()))
            }
            StreamInner::TQuic { .. } => {
                // TODO: Implement proper async read for tQUIC
                std::task::Poll::Ready(Ok(()))
            }
        }
    }
}

impl tokio::io::AsyncWrite for QuicStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { .. } => {
                // TODO: Implement proper async write
                std::task::Poll::Ready(Ok(buf.len()))
            }
            StreamInner::TQuic { .. } => {
                // TODO: Implement proper async write for tQUIC
                std::task::Poll::Ready(Ok(buf.len()))
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        std::task::Poll::Ready(Ok(()))
    }
}