use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::oneshot;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::error::TransportError;
use crate::transport::messages::{TquicCommand, WorkerConfig, ConnectionId};
use crate::transport::worker_pool::TquicWorkerPool;

/// Async adapter that provides tokio-compatible APIs on top of the tQUIC worker pool
pub struct TquicAdapter {
    worker_pool: Arc<TquicWorkerPool>,
}

impl TquicAdapter {
    pub fn new(worker_count: usize, base_config: WorkerConfig) -> Result<Self, TransportError> {
        let worker_pool = Arc::new(TquicWorkerPool::new(worker_count, base_config)?);
        Ok(TquicAdapter { worker_pool })
    }

    pub async fn connect(&self, remote_addr: SocketAddr) -> Result<ConnectionId, TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = TquicCommand::Connect {
            remote_addr,
            response: response_tx,
        };

        self.worker_pool.submit_command(cmd).await?;
        
        response_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))?
    }

    pub async fn send(
        &self,
        connection_id: ConnectionId,
        stream_id: u64,
        data: Vec<u8>,
        fin: bool,
    ) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = TquicCommand::Send {
            connection_id,
            stream_id,
            data,
            fin,
            response: response_tx,
        };

        self.worker_pool.submit_command(cmd).await?;
        
        response_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))?
    }

    pub async fn receive(
        &self,
        connection_id: ConnectionId,
        stream_id: u64,
        max_length: usize,
    ) -> Result<Vec<u8>, TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = TquicCommand::Receive {
            connection_id,
            stream_id,
            max_length,
            response: response_tx,
        };

        self.worker_pool.submit_command(cmd).await?;
        
        response_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))?
    }

    pub async fn accept(&self) -> Result<(ConnectionId, SocketAddr), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = TquicCommand::Accept {
            response: response_tx,
        };

        self.worker_pool.submit_command(cmd).await?;
        
        response_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))?
    }

    pub async fn open_stream(&self, connection_id: ConnectionId) -> Result<u64, TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = TquicCommand::OpenStream {
            connection_id,
            response: response_tx,
        };

        self.worker_pool.submit_command(cmd).await?;
        
        response_rx.await
            .map_err(|_| TransportError::ConnectionFailed("Response channel closed".into()))?
    }

    pub async fn close_stream(
        &self,
        connection_id: ConnectionId,
        stream_id: u64,
        error_code: u64,
    ) -> Result<(), TransportError> {
        let cmd = TquicCommand::CloseStream {
            connection_id,
            stream_id,
            error_code,
        };

        self.worker_pool.submit_command(cmd).await?;
        Ok(())
    }

    pub async fn close_connection(
        &self,
        connection_id: ConnectionId,
        error_code: u64,
        reason: Vec<u8>,
    ) -> Result<(), TransportError> {
        let cmd = TquicCommand::CloseConnection {
            connection_id,
            error_code,
            reason,
        };

        self.worker_pool.submit_command(cmd).await?;
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), TransportError> {
        self.worker_pool.shutdown(std::time::Duration::from_secs(30)).await
    }

    pub fn worker_status(&self) -> Vec<crate::transport::messages::WorkerStatus> {
        self.worker_pool.get_worker_status()
    }
}

// Implementations for high-level client and server abstractions
#[derive(Clone, Debug)]
pub struct TquicClient {
    adapter: Arc<TquicAdapter>,
}

impl TquicClient {
    pub async fn new(worker_count: Option<usize>) -> Result<Self, TransportError> {
        let worker_count = worker_count.unwrap_or_else(|| num_cpus::get());
        let config = WorkerConfig::default();
        let adapter = Arc::new(TquicAdapter::new(worker_count, config)?);
        Ok(TquicClient { adapter })
    }

    pub async fn connect(&self, addr: &str) -> Result<TquicConnection, TransportError> {
        let remote_addr: SocketAddr = addr.parse()
            .map_err(|e| TransportError::ConfigError(format!("Invalid address: {}", e)))?;
        
        let connection_id = self.adapter.connect(remote_addr).await?;
        Ok(TquicConnection {
            adapter: self.adapter.clone(),
            connection_id,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TquicServer {
    adapter: Arc<TquicAdapter>,
}

impl TquicServer {
    pub async fn new(worker_count: Option<usize>) -> Result<Self, TransportError> {
        let worker_count = worker_count.unwrap_or_else(|| num_cpus::get());
        let mut config = WorkerConfig::default();
        config.is_server = true;
        
        let adapter = Arc::new(TquicAdapter::new(worker_count, config)?);
        Ok(TquicServer { adapter })
    }

    pub async fn listen(&self, addr: &str) -> Result<TquicListener, TransportError> {
        let bind_addr: SocketAddr = addr.parse()
            .map_err(|e| TransportError::ConfigError(format!("Invalid address: {}", e)))?;
        
        // For now, we just create a listener that will accept on any worker
        Ok(TquicListener {
            adapter: self.adapter.clone(),
            bind_addr,
        })
    }
}

pub struct TquicListener {
    adapter: Arc<TquicAdapter>,
    bind_addr: SocketAddr,
}

impl TquicListener {
    pub async fn accept(&mut self) -> Result<TquicConnection, TransportError> {
        let (connection_id, _remote_addr) = self.adapter.accept().await?;
        Ok(TquicConnection {
            adapter: self.adapter.clone(),
            connection_id,
        })
    }
}

// Implement the Listener trait for TquicListener
#[async_trait::async_trait]
impl crate::transport::Listener for TquicListener {
    type Connection = TquicStream;
    
    async fn accept(&mut self) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let connection = self.accept().await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync + 'static>)?;
        let stream = connection.open_stream().await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync + 'static>)?;
        Ok(stream)
    }
}

pub struct TquicConnection {
    adapter: Arc<TquicAdapter>,
    connection_id: ConnectionId,
}

impl TquicConnection {
    pub async fn open_stream(&self) -> Result<TquicStream, TransportError> {
        let stream_id = self.adapter.open_stream(self.connection_id).await?;
        Ok(TquicStream {
            adapter: self.adapter.clone(),
            connection_id: self.connection_id,
            stream_id,
        })
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.adapter.close_connection(self.connection_id, 0, Vec::new()).await
    }
}

pub struct TquicStream {
    adapter: Arc<TquicAdapter>,
    connection_id: ConnectionId,
    stream_id: u64,
}

impl TquicStream {
    pub async fn send(&mut self, data: &[u8], fin: bool) -> Result<(), TransportError> {
        self.adapter.send(self.connection_id, self.stream_id, data.to_vec(), fin).await
    }

    pub async fn receive(&mut self, max_length: usize) -> Result<Vec<u8>, TransportError> {
        self.adapter.receive(self.connection_id, self.stream_id, max_length).await
    }

    pub async fn close(&self) -> Result<(), TransportError> {
        self.adapter.close_stream(self.connection_id, self.stream_id, 0).await
    }
}

// Implement AsyncRead and AsyncWrite for TquicStream
impl AsyncRead for TquicStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // TODO: Implement proper async reading via the adapter
        // For now, return pending to prevent blocking
        Poll::Pending
    }
}

impl AsyncWrite for TquicStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        // TODO: Implement proper async writing via the adapter
        // For now, return ready with 0 bytes written
        Poll::Ready(Ok(0))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // TODO: Implement proper flush via the adapter
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // TODO: Implement proper shutdown via the adapter
        Poll::Ready(Ok(()))
    }
}

impl Unpin for TquicStream {}

// Add Clone and Debug implementations for TquicClient and TquicServer
#[derive(Clone, Debug)]
pub struct TquicClientWrapper {
    inner: TquicClient,
}

#[derive(Clone, Debug)]
pub struct TquicServerWrapper {
    inner: TquicServer,
}