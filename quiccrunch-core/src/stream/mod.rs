// This file implements QUIC bidirectional stream logic for quiccrunch-core.
// It provides abstractions for lock-free and tQUIC backends.


/// A QUIC bidirectional stream for sending and receiving messages
#[derive(Debug, Clone)]
pub struct QuicStream {
    inner: StreamInner,
    max_message_size: usize,
}

#[derive(Debug, Clone)]
enum StreamInner {
    #[cfg(feature = "lock-free-channels")]
    LockFree {
        handle: crate::lockfree::StreamHandle,
    },
    // tQUIC implementation - uses worker pool for multi-threaded backend
    TQuic {
        worker_pool: std::sync::Arc<crate::transport::worker_pool::TquicWorkerPool>,
        stream_id: u64,
    },
}

impl QuicStream {
    /// Create a new QuicStream with connection and stream IDs (generic constructor)
    pub fn new(connection_id: u64, stream_id: u64, max_message_size: usize) -> Self {
        // Use the tQUIC backend as default when no lock-free channels
        #[cfg(all(feature = "backend-tquic", not(feature = "lock-free-channels")))]
        {
            let base_config = crate::transport::messages::WorkerConfig::default();
            let worker_pool = std::sync::Arc::new(
                crate::transport::worker_pool::TquicWorkerPool::new(1, base_config)
                    .expect("Failed to create default worker pool")
            );
            Self::new_simple_tquic(connection_id, stream_id, max_message_size, worker_pool)
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

    /// Create a simple tQUIC stream with worker pool
    pub fn new_simple_tquic(_connection_id: u64, stream_id: u64, max_message_size: usize, worker_pool: std::sync::Arc<crate::transport::worker_pool::TquicWorkerPool>) -> Self {
        Self {
            inner: StreamInner::TQuic {
                worker_pool,
                stream_id,
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
            StreamInner::TQuic { worker_pool, .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(
                    0, // connection_id placeholder
                    new_stream_id,
                    self.max_message_size,
                    worker_pool.clone(),
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
            StreamInner::TQuic { worker_pool, .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(
                    0, // connection_id placeholder
                    new_stream_id,
                    self.max_message_size,
                    worker_pool.clone(),
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
                    .map_err(|_| crate::TransportError::ConnectionFailed("Write channel closed".into()))?;
                Ok(())
            }
            StreamInner::TQuic { worker_pool, stream_id } => {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                let cmd = crate::transport::messages::TquicCommand::Send {
                    connection_id: 0, // TODO: pass connection_id
                    stream_id: *stream_id,
                    data: data.to_vec(),
                    fin: false,
                    response: response_tx,
                };
                worker_pool.submit_command(cmd).await
                    .map_err(|_| crate::TransportError::ConnectionFailed("Worker pool command failed".into()))?;
                response_rx.await
                    .map_err(|_| crate::TransportError::ConnectionFailed("Response channel closed".into()))?
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
                    .ok_or_else(|| crate::TransportError::ConnectionFailed("Read channel closed".into()))?;
                Ok(bytes.to_vec())
            }
            StreamInner::TQuic { worker_pool, stream_id } => {
                let (response_tx, response_rx) = tokio::sync::oneshot::channel();
                let cmd = crate::transport::messages::TquicCommand::Receive {
                    connection_id: 0, // TODO: pass connection_id
                    stream_id: *stream_id,
                    max_length: self.max_message_size,
                    response: response_tx,
                };
                worker_pool.submit_command(cmd).await
                    .map_err(|_| crate::TransportError::ConnectionFailed("Worker pool command failed".into()))?;
                response_rx.await
                    .map_err(|_| crate::TransportError::ConnectionFailed("Response channel closed".into()))?
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
                    .map_err(|_| crate::TransportError::ConnectionFailed("Close channel failed".into()))?;
                Ok(())
            }
            StreamInner::TQuic { worker_pool, stream_id } => {
                let cmd = crate::transport::messages::TquicCommand::CloseStream {
                    connection_id: 0, // TODO: pass connection_id
                    stream_id: *stream_id,
                    error_code: 0,
                };
                worker_pool.submit_command(cmd).await
                    .map_err(|_| crate::TransportError::ConnectionFailed("Worker pool command failed".into()))?;
                Ok(())
            }
        }
    }

    /// Send a message (Cap'n Proto compatible)
    pub async fn send_message(&mut self, data: bytes::Bytes) -> crate::Result<()> {
        self.send(&data).await
    }

    /// Receive a message (Cap'n Proto compatible)
    pub async fn receive_message(&mut self) -> crate::Result<bytes::Bytes> {
        let data = self.recv().await?;
        Ok(bytes::Bytes::from(data))
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
        let (worker_pool, stream_id) = match &self.inner {
            StreamInner::TQuic { worker_pool, stream_id } => (worker_pool.clone(), *stream_id),
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { .. } => (std::sync::Arc::new(crate::transport::worker_pool::TquicWorkerPool::new(1, crate::transport::messages::WorkerConfig::default()).unwrap()), 0),
        };
        let reader = QuicStreamReader {
            _inner: self.inner,
        };
        let writer = QuicStreamWriter {
            _inner: StreamInner::TQuic {
                worker_pool,
                stream_id,
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