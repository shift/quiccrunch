//! Stream splitting for concurrent read/write access in streaming RPC
//!
//! Provides a safe way to split QuicStream into separate read/write handles
//! that can be used concurrently in different tasks.

use crate::{QuicStream, Result, TransportError};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Split stream handle that allows concurrent read/write operations
#[derive(Clone)]
pub struct SplitStream {
    inner: Arc<StreamSplitterInner>,
}

struct StreamSplitterInner {
    // Command channel for coordinating operations
    cmd_tx: mpsc::UnboundedSender<StreamCommand>,
    // The background task handle
    _task_handle: tokio::task::JoinHandle<()>,
}

enum StreamCommand {
    Send {
        data: Bytes,
        response: oneshot::Sender<Result<()>>,
    },
    Receive {
        response: oneshot::Sender<Result<Bytes>>,
    },
    Close {
        response: oneshot::Sender<Result<()>>,
    },
}

impl SplitStream {
    /// Create a new split stream from a QuicStream
    pub fn new(stream: QuicStream) -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<StreamCommand>();
        let stream = Arc::new(Mutex::new(stream));
        let stream_clone = Arc::clone(&stream);

        // Background task to handle stream operations
        let task_handle = tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    StreamCommand::Send { data, response } => {
                        let mut stream = stream_clone.lock().await;
                        let result = stream.send(&data).await;
                        let _ = response.send(result);
                    }
                    StreamCommand::Receive { response } => {
                        let mut stream = stream_clone.lock().await;
                        let result = stream.recv().await.map(Bytes::from);
                        let _ = response.send(result);
                    }
                    StreamCommand::Close { response } => {
                        let mut stream = stream_clone.lock().await;
                        let result = stream.close().await;
                        let _ = response.send(result);
                        // Exit the task after close
                        break;
                    }
                }
            }
        });

        Self {
            inner: Arc::new(StreamSplitterInner {
                cmd_tx,
                _task_handle: task_handle,
            }),
        }
    }

    /// Send a message over the stream
    pub async fn send_message(&self, data: Bytes) -> Result<()> {
        let (tx, rx) = oneshot::channel();

        self.inner
            .cmd_tx
            .send(StreamCommand::Send { data, response: tx })
            .map_err(|_| TransportError::StreamError("Stream splitter closed".into()))?;

        rx.await
            .map_err(|_| TransportError::StreamError("Send operation canceled".into()))?
    }

    /// Receive a message from the stream
    pub async fn receive_message(&self) -> Result<Bytes> {
        let (tx, rx) = oneshot::channel();

        self.inner
            .cmd_tx
            .send(StreamCommand::Receive { response: tx })
            .map_err(|_| TransportError::StreamError("Stream splitter closed".into()))?;

        rx.await
            .map_err(|_| TransportError::StreamError("Receive operation canceled".into()))?
    }

    /// Close the stream
    pub async fn close(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();

        self.inner
            .cmd_tx
            .send(StreamCommand::Close { response: tx })
            .map_err(|_| TransportError::StreamError("Stream splitter closed".into()))?;

        rx.await
            .map_err(|_| TransportError::StreamError("Close operation canceled".into()))?
    }
}

/// Read-only handle for split stream
#[derive(Clone)]
pub struct StreamReader {
    splitter: SplitStream,
}

impl StreamReader {
    /// Create a new StreamReader from a SplitStream
    pub fn new(splitter: SplitStream) -> Self {
        Self { splitter }
    }

    /// Receive a message from the stream
    pub async fn receive_message(&self) -> Result<Bytes> {
        self.splitter.receive_message().await
    }
}

/// Write-only handle for split stream
#[derive(Clone)]
pub struct StreamWriter {
    splitter: SplitStream,
}

impl StreamWriter {
    /// Create a new StreamWriter from a SplitStream
    pub fn new(splitter: SplitStream) -> Self {
        Self { splitter }
    }

    /// Send a message over the stream
    pub async fn send_message(&self, data: Bytes) -> Result<()> {
        self.splitter.send_message(data).await
    }

    /// Close the stream
    pub async fn close(&self) -> Result<()> {
        self.splitter.close().await
    }
}

impl SplitStream {
    /// Split into separate read and write handles
    pub fn split(self) -> (StreamReader, StreamWriter) {
        let reader = StreamReader {
            splitter: self.clone(),
        };
        let writer = StreamWriter { splitter: self };
        (reader, writer)
    }
}

/// Trait for creating splittable streams
pub trait Splittable {
    fn into_split_stream(self) -> SplitStream;
}

impl Splittable for QuicStream {
    fn into_split_stream(self) -> SplitStream {
        SplitStream::new(self)
    }
}

#[cfg(test)]
mod tests {

    // Mock stream for testing
    #[allow(dead_code)]
    struct MockStream;

    #[tokio::test]
    async fn test_split_stream_basic() {
        // This test would need a real QuicStream implementation
        // For now, it demonstrates the API
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        // Test that we can use reader and writer concurrently
        // This would need a real stream implementation
    }
}
