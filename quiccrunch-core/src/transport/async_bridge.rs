use crate::QuicStream;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::oneshot;

use crate::transport::messages::{TquicCommand as WorkerCommand, WorkerConfig};
use crate::transport::worker_pool::TquicWorkerPool;

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
        let worker_count = num_cpus::get().clamp(2, 8); // Between 2-8 workers
        let worker_pool = TquicWorkerPool::new(worker_count, base_config)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(TquicAsyncBridge {
            worker_pool: Arc::new(worker_pool),
        })
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Shutdown worker pool
        self.worker_pool
            .shutdown(std::time::Duration::from_secs(5))
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        Ok(())
    }
}

/// Handle for async communication with tQUIC listener
#[derive(Debug)]
pub struct TquicListenerHandle {
    /// Retained for listener diagnostics/identification.
    #[allow(dead_code)]
    listener_id: u64,
    worker_pool: Arc<TquicWorkerPool>,
}

impl TquicListenerHandle {
    pub async fn accept(&mut self) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync>> {
        // Use worker pool to accept connection
        let (response_tx, response_rx) = oneshot::channel();

        let cmd = WorkerCommand::Accept {
            response: response_tx,
        };
        self.worker_pool
            .submit_command(cmd)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let (connection_id, _remote_addr) = response_rx
            .await
            .map_err(|_| Box::new(io::Error::other("response channel closed")))?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Create a stream for the connection
        Ok(QuicStream::new_simple_tquic(
            connection_id,
            0,           // stream_id placeholder
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
        let base_config = WorkerConfig {
            ca_cert_file: ca_cert,
            verify_cert: true,
            ..WorkerConfig::default()
        };

        // Create worker pool with optimal worker count
        let worker_count = num_cpus::get().clamp(2, 8);
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
        let remote_addr: SocketAddr = addr
            .parse()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Use worker pool to connect
        let (response_tx, response_rx) = oneshot::channel();

        let cmd = WorkerCommand::Connect {
            connection_id: 0, // Will be assigned by worker pool
            remote_addr,
            response: response_tx,
        };

        self.worker_pool
            .submit_command(cmd)
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        let connection_id = response_rx
            .await
            .map_err(|_| Box::new(io::Error::other("response channel closed")))?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        // Create a stream for the connection
        Ok(QuicStream::new_simple_tquic(
            connection_id,
            0,           // stream_id placeholder
            1024 * 1024, // 1MB max message size
            self.worker_pool.clone(),
        ))
    }
}

/// Async tQUIC server
#[derive(Debug)]
pub struct AsyncTquicServer {
    worker_pool: Arc<TquicWorkerPool>,
    /// Server bind address, retained for introspection.
    #[allow(dead_code)]
    bind_addr: SocketAddr,
    /// TLS certificate path, retained for introspection.
    #[allow(dead_code)]
    cert_file: String,
    /// TLS key path, retained for introspection.
    #[allow(dead_code)]
    key_file: String,
}

impl AsyncTquicServer {
    pub async fn new(
        bind_addr: SocketAddr,
        cert_file: String,
        key_file: String,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        // Create server configuration
        let base_config = WorkerConfig {
            is_server: true,
            bind_addr,
            cert_file: Some(cert_file.clone()),
            key_file: Some(key_file.clone()),
            ..WorkerConfig::default()
        };

        // Create worker pool for server
        let worker_count = num_cpus::get().clamp(2, 8);
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
