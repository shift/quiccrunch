/// Lock-free channel implementation for high-performance QUIC streams
/// 
/// This module provides lock-free, multi-producer, single-consumer channels
/// optimized for the QUIC use case where multiple streams need to communicate
/// with the underlying transport without contention.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use crossbeam_channel::{bounded, Sender, Receiver, TryRecvError};
use std::collections::HashMap;
use tokio::sync::{oneshot, Mutex};
use bytes::Bytes;

/// Stream command types for lock-free communication
#[derive(Debug)]
pub enum StreamCommand {
    Write {
        stream_id: u64,
        data: Bytes,
        fin: bool,
        response: oneshot::Sender<Result<(), crate::TransportError>>,
    },
    Read {
        stream_id: u64,
        buffer_size: usize,
        response: oneshot::Sender<Result<Option<Bytes>, crate::TransportError>>,
    },
    Close {
        stream_id: u64,
        response: oneshot::Sender<Result<(), crate::TransportError>>,
    },
    OpenStream {
        response: oneshot::Sender<u64>,
    },
}

/// Connection command types
#[derive(Debug)]
pub enum ConnectionCommand {
    NewConnection {
        connection_id: u64,
        response: oneshot::Sender<ConnectionHandle>,
    },
    CloseConnection {
        connection_id: u64,
        response: oneshot::Sender<()>,
    },
}

/// High-performance stream handle with dedicated channels
#[derive(Clone, Debug)]
pub struct StreamHandle {
    pub stream_id: u64,
    pub connection_id: u64,
    write_tx: Sender<StreamCommand>,
    read_tx: Sender<StreamCommand>,
    close_tx: Sender<StreamCommand>,
    stats: Arc<StreamStats>,
}

impl StreamHandle {
    pub fn new(
        stream_id: u64, 
        connection_id: u64,
        write_tx: Sender<StreamCommand>,
        read_tx: Sender<StreamCommand>,
        close_tx: Sender<StreamCommand>,
    ) -> Self {
        Self {
            stream_id,
            connection_id,
            write_tx,
            read_tx,
            close_tx,
            stats: Arc::new(StreamStats::new()),
        }
    }

    /// Send data on this stream (non-blocking)
    pub async fn write(&self, data: Bytes, fin: bool) -> Result<(), crate::TransportError> {
        let (tx, rx): (oneshot::Sender<Result<(), crate::TransportError>>, oneshot::Receiver<Result<(), crate::TransportError>>) = oneshot::channel();
        
        let cmd = StreamCommand::Write {
            stream_id: self.stream_id,
            data: data.clone(),
            fin,
            response: tx,
        };

        // Use dedicated write channel to avoid contention
        self.write_tx.send(cmd)
            .map_err(|_| crate::TransportError::StreamError("Write channel closed".into()))?;

        self.stats.writes_sent.fetch_add(1, Ordering::Relaxed);
        self.stats.bytes_written.fetch_add(data.len() as u64, Ordering::Relaxed);

        rx.await
            .map_err(|_| crate::TransportError::StreamError("Write response canceled".into()))?
    }

    /// Read data from this stream (non-blocking)
    pub async fn read(&self, buffer_size: usize) -> Result<Option<Bytes>, crate::TransportError> {
        let (tx, rx): (oneshot::Sender<Result<Option<Bytes>, crate::TransportError>>, oneshot::Receiver<Result<Option<Bytes>, crate::TransportError>>) = oneshot::channel();
        
        let cmd = StreamCommand::Read {
            stream_id: self.stream_id,
            buffer_size,
            response: tx,
        };

        self.read_tx.send(cmd)
            .map_err(|_| crate::TransportError::StreamError("Read channel closed".into()))?;

        self.stats.reads_sent.fetch_add(1, Ordering::Relaxed);

        let result = rx.await
            .map_err(|_| crate::TransportError::StreamError("Read response canceled".into()))?;

        if let Ok(Some(ref data)) = result {
            self.stats.bytes_read.fetch_add(data.len() as u64, Ordering::Relaxed);
        }

        result
    }

    /// Close this stream
    pub async fn close(&self) -> Result<(), crate::TransportError> {
        let (tx, rx): (oneshot::Sender<Result<(), crate::TransportError>>, oneshot::Receiver<Result<(), crate::TransportError>>) = oneshot::channel();
        
        let cmd = StreamCommand::Close {
            stream_id: self.stream_id,
            response: tx,
        };

        self.close_tx.send(cmd)
            .map_err(|_| crate::TransportError::StreamError("Close channel closed".into()))?;

        rx.await
            .map_err(|_| crate::TransportError::StreamError("Close response canceled".into()))?
    }

    /// Get stream statistics
    pub fn stats(&self) -> StreamStatsSnapshot {
        self.stats.snapshot()
    }
}

/// Statistics for individual streams
#[derive(Debug)]
pub struct StreamStats {
    pub writes_sent: AtomicU64,
    pub reads_sent: AtomicU64,
    pub bytes_written: AtomicU64,
    pub bytes_read: AtomicU64,
    pub errors: AtomicU64,
}

impl StreamStats {
    pub fn new() -> Self {
        Self {
            writes_sent: AtomicU64::new(0),
            reads_sent: AtomicU64::new(0),
            bytes_written: AtomicU64::new(0),
            bytes_read: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> StreamStatsSnapshot {
        StreamStatsSnapshot {
            writes_sent: self.writes_sent.load(Ordering::Relaxed),
            reads_sent: self.reads_sent.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
            bytes_read: self.bytes_read.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamStatsSnapshot {
    pub writes_sent: u64,
    pub reads_sent: u64,
    pub bytes_written: u64,
    pub bytes_read: u64,
    pub errors: u64,
}

/// Connection handle managing multiple streams
#[derive(Clone)]
#[derive(Debug)]
pub struct ConnectionHandle {
    pub connection_id: u64,
    stream_counter: Arc<AtomicU64>,
    
    // Separate channels for different operations to minimize contention
    write_tx: Sender<StreamCommand>,
    read_tx: Sender<StreamCommand>,
    close_tx: Sender<StreamCommand>,
    
    // Per-connection stream registry - TODO: Replace Mutex with lock-free alternative
    // PERFORMANCE CRITICAL: This Mutex negates "lock-free" benefits!
    streams: Arc<Mutex<HashMap<u64, StreamHandle>>>,
    stats: Arc<ConnectionStats>,
}

impl ConnectionHandle {
    pub fn new(
        connection_id: u64,
        write_capacity: usize,
        read_capacity: usize,
        close_capacity: usize,
    ) -> (Self, ConnectionChannels) {
        let (write_tx, write_rx) = bounded(write_capacity);
        let (read_tx, read_rx) = bounded(read_capacity);
        let (close_tx, close_rx) = bounded(close_capacity);

        let handle = Self {
            connection_id,
            stream_counter: Arc::new(AtomicU64::new(0)),
            write_tx,
            read_tx,
            close_tx,
            streams: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(ConnectionStats::new()),
        };

        let channels = ConnectionChannels {
            write_rx,
            read_rx,
            close_rx,
        };

        (handle, channels)
    }

    /// Create a new stream on this connection
    pub async fn open_stream(&self) -> Result<StreamHandle, crate::TransportError> {
        let stream_id = self.stream_counter.fetch_add(1, Ordering::Relaxed);
        
        let stream = StreamHandle::new(
            stream_id,
            self.connection_id,
            self.write_tx.clone(),
            self.read_tx.clone(),
            self.close_tx.clone(),
        );

        // Register stream
        {
            let mut streams = self.streams.lock().await;
            streams.insert(stream_id, stream.clone());
        }

        self.stats.streams_opened.fetch_add(1, Ordering::Relaxed);
        
        Ok(stream)
    }

    /// Close a stream and remove it from registry
    pub async fn close_stream(&self, stream_id: u64) -> Result<(), crate::TransportError> {
        let mut streams = self.streams.lock().await;
        if let Some(stream) = streams.remove(&stream_id) {
            drop(streams); // Release lock before async operation
            stream.close().await?;
            self.stats.streams_closed.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Get connection statistics
    pub fn stats(&self) -> ConnectionStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get number of active streams
    pub async fn stream_count(&self) -> usize {
        self.streams.lock().await.len()
    }
}

/// Receiver channels for processing stream commands
pub struct ConnectionChannels {
    pub write_rx: Receiver<StreamCommand>,
    pub read_rx: Receiver<StreamCommand>,
    pub close_rx: Receiver<StreamCommand>,
}

/// Statistics for connections
#[derive(Debug)]
pub struct ConnectionStats {
    pub streams_opened: AtomicU64,
    pub streams_closed: AtomicU64,
    pub total_bytes_written: AtomicU64,
    pub total_bytes_read: AtomicU64,
    pub command_queue_depth: AtomicUsize,
}

impl ConnectionStats {
    pub fn new() -> Self {
        Self {
            streams_opened: AtomicU64::new(0),
            streams_closed: AtomicU64::new(0),
            total_bytes_written: AtomicU64::new(0),
            total_bytes_read: AtomicU64::new(0),
            command_queue_depth: AtomicUsize::new(0),
        }
    }

    pub fn snapshot(&self) -> ConnectionStatsSnapshot {
        ConnectionStatsSnapshot {
            streams_opened: self.streams_opened.load(Ordering::Relaxed),
            streams_closed: self.streams_closed.load(Ordering::Relaxed),
            total_bytes_written: self.total_bytes_written.load(Ordering::Relaxed),
            total_bytes_read: self.total_bytes_read.load(Ordering::Relaxed),
            command_queue_depth: self.command_queue_depth.load(Ordering::Relaxed),
            active_streams: self.streams_opened.load(Ordering::Relaxed) 
                          - self.streams_closed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionStatsSnapshot {
    pub streams_opened: u64,
    pub streams_closed: u64,
    pub active_streams: u64,
    pub total_bytes_written: u64,
    pub total_bytes_read: u64,
    pub command_queue_depth: usize,
}

/// High-performance command processor for stream operations
pub struct StreamProcessor {
    connection_id: u64,
    channels: ConnectionChannels,
    backend: Box<dyn StreamBackend + Send>,
}

impl StreamProcessor {
    pub fn new(
        connection_id: u64,
        channels: ConnectionChannels,
        backend: Box<dyn StreamBackend + Send>,
    ) -> Self {
        Self {
            connection_id,
            channels,
            backend,
        }
    }

    /// Process commands from all channels concurrently
    pub async fn run(mut self) -> Result<(), crate::TransportError> {
        let mut write_batch = Vec::with_capacity(32);
        let mut read_batch = Vec::with_capacity(32);
        let mut close_batch = Vec::with_capacity(8);

        loop {
            // Batch process write commands
            self.process_write_batch(&mut write_batch).await?;
            
            // Batch process read commands
            self.process_read_batch(&mut read_batch).await?;
            
            // Process close commands
            self.process_close_batch(&mut close_batch).await?;

            // Small yield to prevent CPU spinning
            tokio::task::yield_now().await;
        }
    }

    async fn process_write_batch(&mut self, batch: &mut Vec<StreamCommand>) -> Result<(), crate::TransportError> {
        // Collect up to batch size
        while batch.len() < batch.capacity() {
            match self.channels.write_rx.try_recv() {
                Ok(cmd) => batch.push(cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        // Process batch
        for cmd in batch.drain(..) {
            if let StreamCommand::Write { stream_id, data, fin, response } = cmd {
                let result = self.backend.write(stream_id, data, fin).await;
                let _ = response.send(result);
            }
        }

        Ok(())
    }

    async fn process_read_batch(&mut self, batch: &mut Vec<StreamCommand>) -> Result<(), crate::TransportError> {
        // Similar batching for reads
        while batch.len() < batch.capacity() {
            match self.channels.read_rx.try_recv() {
                Ok(cmd) => batch.push(cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        for cmd in batch.drain(..) {
            if let StreamCommand::Read { stream_id, buffer_size, response } = cmd {
                let result = self.backend.read(stream_id, buffer_size).await;
                let _ = response.send(result);
            }
        }

        Ok(())
    }

    async fn process_close_batch(&mut self, batch: &mut Vec<StreamCommand>) -> Result<(), crate::TransportError> {
        while batch.len() < batch.capacity() {
            match self.channels.close_rx.try_recv() {
                Ok(cmd) => batch.push(cmd),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }

        for cmd in batch.drain(..) {
            if let StreamCommand::Close { stream_id, response } = cmd {
                let result = self.backend.close(stream_id).await;
                let _ = response.send(result);
            }
        }

        Ok(())
    }
}

/// Backend trait for actual QUIC implementation
#[async_trait::async_trait]
pub trait StreamBackend {
    async fn write(&mut self, stream_id: u64, data: Bytes, fin: bool) -> Result<(), crate::TransportError>;
    async fn read(&mut self, stream_id: u64, buffer_size: usize) -> Result<Option<Bytes>, crate::TransportError>;
    async fn close(&mut self, stream_id: u64) -> Result<(), crate::TransportError>;
}

/// Factory for creating optimized channel configurations
pub struct ChannelConfig {
    pub write_capacity: usize,
    pub read_capacity: usize,
    pub close_capacity: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            write_capacity: 1024,  // High capacity for writes
            read_capacity: 512,    // Medium capacity for reads
            close_capacity: 64,    // Low capacity for closes
        }
    }
}

impl ChannelConfig {
    /// Configuration optimized for high-throughput scenarios
    pub fn high_throughput() -> Self {
        Self {
            write_capacity: 4096,
            read_capacity: 2048,
            close_capacity: 128,
        }
    }

    /// Configuration optimized for low-latency scenarios
    pub fn low_latency() -> Self {
        Self {
            write_capacity: 256,
            read_capacity: 128,
            close_capacity: 32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_handle_creation() {
        let config = ChannelConfig::default();
        let (conn, _channels) = ConnectionHandle::new(
            1,
            config.write_capacity,
            config.read_capacity,
            config.close_capacity,
        );

        let stream = conn.open_stream().await.unwrap();
        assert_eq!(stream.connection_id, 1);
        assert_eq!(stream.stream_id, 0);
    }

    #[tokio::test]
    async fn test_concurrent_stream_operations() {
        let config = ChannelConfig::default();
        let (conn, _channels) = ConnectionHandle::new(
            1,
            config.write_capacity,
            config.read_capacity,
            config.close_capacity,
        );

        // Create multiple streams concurrently
        let mut handles = Vec::new();
        for _ in 0..10 {
            let conn = conn.clone();
            handles.push(tokio::spawn(async move {
                conn.open_stream().await.unwrap()
            }));
        }

        // Wait for all streams to be created
        let streams = futures::future::join_all(handles).await;
        assert_eq!(streams.len(), 10);

        // Verify unique stream IDs
        let mut stream_ids: Vec<_> = streams.into_iter()
            .map(|s| s.unwrap().stream_id)
            .collect();
        stream_ids.sort();
        stream_ids.dedup();
        assert_eq!(stream_ids.len(), 10);
    }
}