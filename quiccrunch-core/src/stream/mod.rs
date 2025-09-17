use std::time::Duration;
use std::sync::Arc;
use bytes::Bytes;

use tokio::io::AsyncRead;
use dashmap::DashMap;
use mio::Poll;

/// A QUIC bidirectional stream for sending and receiving messages
#[derive(Debug, Clone)]
pub struct QuicStream {
    inner: StreamInner,
    max_message_size: usize,
}

#[derive(Debug)]
enum StreamInner {
    #[cfg(feature = "lock-free-channels")]
    LockFree { handle: crate::lockfree::StreamHandle },
    // tQUIC implementation - simplified for now
    TQuic { stream_id: u64, connection_id: u64 },
    #[cfg(feature = "backend-tquic")]
    RealTQuic {
        endpoint: std::sync::Arc<std::sync::Mutex<tquic::Endpoint>>,
        conn_index: u64,
        stream_id: u64,
        sock: std::rc::Rc<crate::transport::tquic::QuicSocket>,
        read_rx: tokio::sync::mpsc::Receiver<bytes::Bytes>,
        write_tx: tokio::sync::mpsc::Sender<bytes::Bytes>,
        close_tx: tokio::sync::mpsc::Sender<()>,
        _handle: std::thread::JoinHandle<()>
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

    /// Create a real tQUIC stream
    #[cfg(feature = "backend-tquic")]
    pub fn new_real_tquic(endpoint: std::sync::Arc<std::sync::Mutex<tquic::Endpoint>>, conn_index: u64, stream_id: u64, sock: std::rc::Rc<tquic_tools::QuicSocket>, max_message_size: usize) -> Self {
        let (read_tx, read_rx) = tokio::sync::mpsc::channel(100);
        let (write_tx, write_rx) = tokio::sync::mpsc::channel(100);
        let (close_tx, close_rx) = tokio::sync::mpsc::channel(1);

        let endpoint_clone = endpoint.clone();
        let sock_clone = sock.clone();
        let handle = std::thread::spawn(move || {
            let mut poll = mio::Poll::new().unwrap();
            let registry = poll.registry();
            let _ = sock_clone.add("0.0.0.0:0".parse().unwrap(), registry); // register if needed
            let mut events = mio::Events::with_capacity(1024);
            let mut buf = vec![0u8; 65536];

            loop {
                if close_rx.try_recv().is_ok() {
                    break;
                }

                if let Ok(mut ep) = endpoint_clone.lock() {
                    let _ = ep.process_connections();
                }

                if poll.poll(&mut events, Some(std::time::Duration::from_millis(10))).is_ok() {
                    for event in events.iter() {
                        if event.is_readable() {
                            if let Ok((len, local, remote)) = sock_clone.recv_from(&mut buf, event.token()) {
                                let pkt_info = tquic::PacketInfo {
                                    src: remote,
                                    dst: local,
                                    time: std::time::Instant::now(),
                                };
                                if let Ok(mut ep) = endpoint_clone.lock() {
                                    let _ = ep.recv(&buf[..len], &pkt_info);
                                }
                            }
                        }
                    }
                }

                while let Ok(data) = write_rx.try_recv() {
                    if let Ok(mut ep) = endpoint_clone.lock() {
                        if let Some(conn) = ep.conn_get_mut(conn_index) {
                            let _ = conn.stream_write(stream_id, data, false);
                        }
                    }
                }

                if let Ok(mut ep) = endpoint_clone.lock() {
                    if let Some(conn) = ep.conn_get_mut(conn_index) {
                        while let Ok((len, _)) = conn.stream_read(stream_id, &mut buf) {
                            if len > 0 {
                                let _ = read_tx.try_send(bytes::Bytes::copy_from_slice(&buf[..len]));
                            } else {
                                break;
                            }
                        }
                    }
                }

                if let Ok(mut ep) = endpoint_clone.lock() {
                    ep.on_timeout(std::time::Instant::now());
                }

                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        Self { 
            inner: StreamInner::RealTQuic { 
                endpoint, 
                conn_index, 
                stream_id,
                sock,
                read_rx,
                write_tx,
                close_tx,
                _handle: handle
            }, 
            max_message_size 
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
              StreamInner::TQuic { stream_id: _, connection_id } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*connection_id, new_stream_id, self.max_message_size))
            }
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { conn_index, .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*conn_index, new_stream_id, self.max_message_size))
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
            StreamInner::TQuic { stream_id: _, connection_id } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*connection_id, new_stream_id, self.max_message_size))
            }
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { conn_index, .. } => {
                let new_stream_id = rand::random::<u64>();
                Ok(QuicStream::new_simple_tquic(*conn_index, new_stream_id, self.max_message_size))
            }
        }
    }

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
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { write_tx, .. } => {
                let mut combined = Vec::with_capacity(4 + data.len());
                combined.extend_from_slice(&header);
                combined.extend_from_slice(&data);
                write_tx.try_send(bytes::Bytes::from(combined)).map_err(|_| crate::TransportError::StreamError("send failed".into()))?;
                Ok(())
            }
        }

        #[cfg(feature = "telemetry")]
        info!("Message sent successfully");

        Ok(())
    }

    /// Close the stream
    pub async fn close(&mut self) -> crate::Result<()> {
        match &mut self.inner {
            #[cfg(feature = "lock-free-channels")]
            StreamInner::LockFree { handle } => {
                handle.close().await?;
                Ok(())
            }
            StreamInner::TQuic { .. } => {
                // tQUIC implementation: nothing to close for this simplified version
                Ok(())
            }
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { close_tx, .. } => {
                let _ = close_tx.try_send(());
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
        let mut buffer = Vec::new();
        let mut bytes_read = 0;

        lazy_static::lazy_static! {
            static ref TQUIC_BUFFER: Arc<DashMap<u64, Vec<u8>>> = Arc::new(DashMap::new());
        }

        match &mut self.inner {
            StreamInner::TQuic { stream_id, .. } => {
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
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { read_rx, .. } => {
                let data = read_rx.recv().await.ok_or(crate::TransportError::StreamError("tQUIC stream closed".into()))?;
                let data = data.as_ref();
                if data.len() < 4 {
                    return Err(crate::TransportError::StreamError("incomplete header".into()));
                }
                header.copy_from_slice(&data[0..4]);
                let msg_len = u32::from_le_bytes(header) as usize;
                if data.len() < 4 + msg_len {
                    return Err(crate::TransportError::StreamError("incomplete body".into()));
                }
                buffer = data[4..4 + msg_len].to_vec();
                bytes_read = msg_len;
            }
        }

        // Parse message length and prepare buffer
        let msg_len = u32::from_le_bytes(header) as usize;
        if msg_len > self.max_message_size {
            return Err(crate::TransportError::StreamError(format!(
                "Announced message size {} exceeds limit {}", msg_len, self.max_message_size
            )));
        }
        
        if bytes_read == 0 {
            buffer = Vec::with_capacity(msg_len);
        }

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
                #[cfg(feature = "backend-tquic")]
                StreamInner::RealTQuic { .. } => {
                    // Already read full message
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
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.as_mut().inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { close_tx, .. } => {
                let _ = close_tx.try_send(());
                std::task::Poll::Ready(Ok(()))
            }
            _ => std::task::Poll::Ready(Ok(())),
        }
    }
}

impl tokio::io::AsyncWrite for QuicStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut self.as_mut().inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { write_tx, .. } => {
                // Try to send the data
                match write_tx.try_send(bytes::Bytes::copy_from_slice(buf)) {
                    Ok(_) => std::task::Poll::Ready(Ok(buf.len())),
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                        // Channel is full, would block
                        std::task::Poll::Pending
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                        std::task::Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "Stream write channel closed",
                        )))
                    }
                }
            }
            _ => std::task::Poll::Ready(Ok(buf.len())),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.as_mut().inner {
            #[cfg(feature = "backend-tquic")]
            StreamInner::RealTQuic { close_tx, .. } => {
                let _ = close_tx.try_send(());
            }
            _ => {}
        }
        std::task::Poll::Ready(Ok(()))
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