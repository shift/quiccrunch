use crate::QuicStream;
use std::io;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::oneshot;

use crate::transport::worker_pool::TquicWorkerPool;
use crate::transport::messages::{TquicCommand as WorkerCommand, WorkerConfig};

/// Commands sent to the tQUIC bridge thread  
#[derive(Debug)]
pub enum TquicCommand {
    Connect {
        addr: String,
        response: oneshot::Sender<Result<u64, Box<dyn std::error::Error + Send + Sync>>>,
    },
    Listen {
        addr: String,
        response: oneshot::Sender<Result<u64, Box<dyn std::error::Error + Send + Sync>>>,
    },
    Accept {
        listener_id: u64,
        response: oneshot::Sender<Result<u64, Box<dyn std::error::Error + Send + Sync>>>,
    },
    Read {
        stream_id: u64,
        len: usize,
        response: oneshot::Sender<Result<Vec<u8>, io::Error>>,
    },
    Write {
        stream_id: u64,
        data: Vec<u8>,
        response: oneshot::Sender<Result<(), io::Error>>,
    },
    Close {
        stream_id: u64,
    },
    Shutdown,
}

/// Async bridge for tQUIC operations - delegates to worker pool
#[derive(Debug)]
pub struct TquicAsyncBridge {
    worker_pool: Arc<TquicWorkerPool>,
}

impl TquicAsyncBridge {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Create a default worker configuration
        let base_config = WorkerConfig::default();
        
        // Create worker pool with 4 workers (can be configurable)
        let worker_count = num_cpus::get().min(8).max(2); // Between 2-8 workers
        let worker_pool = TquicWorkerPool::new(worker_count, base_config)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(TquicAsyncBridge {
            worker_pool: Arc::new(worker_pool),
        })
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Shutdown worker pool
        self.worker_pool.shutdown(std::time::Duration::from_secs(5)).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
}

/// Handle for async communication with tQUIC listener
#[derive(Debug)]
pub struct TquicListenerHandle {
    listener_id: u64,
    worker_pool: Arc<TquicWorkerPool>,
}

impl TquicListenerHandle {
    pub async fn accept(&mut self) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync>> {
        // Use worker pool to accept connection
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = WorkerCommand::Accept { response: response_tx };
        self.worker_pool.submit_command(cmd).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let (connection_id, _remote_addr) = response_rx.await
            .map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "response channel closed")))?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Create a stream for the connection
        Ok(QuicStream::new_simple_tquic(
            connection_id, 
            0, // stream_id placeholder
            1024 * 1024, // 1MB max message size
            self.worker_pool.clone(),
        ))
    }
}

/// Async tQUIC client
#[derive(Debug, Clone)]
pub struct AsyncTquicClient {
    worker_pool: Arc<TquicWorkerPool>,
}

impl AsyncTquicClient {
    pub async fn new(
        ca_cert: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Create a default worker configuration with optional CA cert
        let mut base_config = WorkerConfig::default();
        base_config.ca_cert_file = ca_cert;
        base_config.verify_cert = true;
        
        // Create worker pool with optimal worker count
        let worker_count = num_cpus::get().min(8).max(2);
        let worker_pool = TquicWorkerPool::new(worker_count, base_config)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(AsyncTquicClient {
            worker_pool: Arc::new(worker_pool),
        })
    }

    pub async fn connect(
        &self,
        addr: &str,
    ) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let remote_addr: SocketAddr = addr.parse()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Use worker pool to connect
        let (response_tx, response_rx) = oneshot::channel();
        
        let cmd = WorkerCommand::Connect {
            connection_id: 0, // Will be assigned by worker pool
            remote_addr,
            response: response_tx,
        };
        
        self.worker_pool.submit_command(cmd).await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let connection_id = response_rx.await
            .map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "response channel closed")))?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Create a stream for the connection
        Ok(QuicStream::new_simple_tquic(
            connection_id,
            0, // stream_id placeholder 
            1024 * 1024, // 1MB max message size
            self.worker_pool.clone(),
        ))
    }
}

/// Async tQUIC server
#[derive(Debug)]
pub struct AsyncTquicServer {
    worker_pool: Arc<TquicWorkerPool>,
    bind_addr: SocketAddr,
    cert_file: String,
    key_file: String,
}

impl AsyncTquicServer {
    pub async fn new(
        bind_addr: SocketAddr,
        cert_file: String,
        key_file: String,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Create server configuration
        let mut base_config = WorkerConfig::default();
        base_config.is_server = true;
        base_config.bind_addr = bind_addr;
        base_config.cert_file = Some(cert_file.clone());
        base_config.key_file = Some(key_file.clone());
        
        // Create worker pool for server
        let worker_count = num_cpus::get().min(8).max(2);
        let worker_pool = TquicWorkerPool::new(worker_count, base_config)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(AsyncTquicServer {
            worker_pool: Arc::new(worker_pool),
            bind_addr,
            cert_file,
            key_file,
        })
    }

    pub async fn listen(
        &self,
    ) -> Result<AsyncTquicListener, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Create a listener handle
        let listener_id = 1; // Placeholder - would be assigned by worker pool

        Ok(AsyncTquicListener {
            handle: TquicListenerHandle { 
                listener_id,
                worker_pool: self.worker_pool.clone(),
            },
        })
    }
}

/// Async tQUIC listener
#[derive(Debug)]
pub struct AsyncTquicListener {
    handle: TquicListenerHandle,
}

impl AsyncTquicListener {
    pub async fn accept(
        &mut self,
    ) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.handle.accept().await
    }
}

#[async_trait::async_trait]
impl crate::transport::Listener for AsyncTquicListener {
    type Connection = QuicStream;

    async fn accept(
        &mut self,
    ) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.handle.accept().await
    }
}