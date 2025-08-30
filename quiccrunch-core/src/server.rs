// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! High-level QUIC server API for easy integration

use crate::transport::{QuicTransport, Transport, TlsConfig, Incoming, Connection};
use crate::{Result, QuicStream};
use bytes::Bytes;

/// Easy-to-use QUIC server
pub struct QuicServer {
    transport: QuicTransport,
}

impl QuicServer {
    /// Create a new server with self-signed certificates (for testing)
    pub fn new() -> Result<Self> {
        let tls_config = crate::test_utils::certs::generate_self_signed_cert("quic-server")?;
        let transport = QuicTransport::new(tls_config)?;
        Ok(Self { transport })
    }

    /// Create a new server with custom TLS configuration
    pub fn with_tls_config(tls_config: TlsConfig) -> Result<Self> {
        let transport = QuicTransport::new(tls_config)?;
        Ok(Self { transport })
    }

    /// Start listening on the given address
    pub async fn listen(&self, addr: &str) -> Result<QuicListener> {
        let incoming = self.transport.listen(addr).await?;
        Ok(QuicListener { inner: incoming })
    }
}

/// A QUIC listener that accepts incoming connections
pub struct QuicListener {
    inner: Incoming,
}

impl QuicListener {
    /// Accept an incoming connection
    pub async fn accept(&mut self) -> Result<QuicConnection> {
        let connection = self.inner.accept().await?;
        Ok(QuicConnection { inner: connection })
    }

    /// Get the local address the listener is bound to
    pub fn local_addr(&self) -> Result<String> {
        self.inner.local_addr()
    }
}

/// A QUIC connection from the server side
#[derive(Clone)]
pub struct QuicConnection {
    inner: Connection,
}

impl QuicConnection {
    /// Accept an incoming bidirectional stream
    pub async fn accept_stream(&self) -> Result<QuicStream> {
        self.inner.accept_bi().await
    }

    /// Open a new bidirectional stream (server can also initiate streams)
    pub async fn open_stream(&self) -> Result<QuicStream> {
        self.inner.open_bi().await
    }
}

/// Handler trait for processing messages
pub trait MessageHandler: Send + Sync + 'static {
    type Response: Into<Bytes> + Send;
    
    fn handle(&self, message: Bytes) -> impl std::future::Future<Output = Self::Response> + Send;
}

/// Echo handler that returns the same message
#[derive(Clone)]
pub struct EchoHandler;

impl MessageHandler for EchoHandler {
    type Response = Bytes;
    
    async fn handle(&self, message: Bytes) -> Bytes {
        message
    }
}

/// Helper function to run a simple echo server
pub async fn run_echo_server(addr: &str) -> Result<()> {
    let server = QuicServer::new()?;
    let mut listener = server.listen(addr).await?;
    
    println!("QUIC echo server listening on {}", listener.local_addr()?);
    
    while let Ok(connection) = listener.accept().await {
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        if let Err(e) = stream.send_message(message).await {
                            eprintln!("Failed to echo message: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}

/// Helper function to run a server with a custom message handler
pub async fn run_server_with_handler<H>(addr: &str, handler: H) -> Result<()>
where
    H: MessageHandler + Clone,
{
    let server = QuicServer::new()?;
    let mut listener = server.listen(addr).await?;
    
    println!("QUIC server listening on {}", listener.local_addr()?);
    
    while let Ok(connection) = listener.accept().await {
        let handler = handler.clone();
        tokio::spawn(async move {
            while let Ok(mut stream) = connection.accept_stream().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    while let Ok(message) = stream.receive_message().await {
                        let response = handler.handle(message).await;
                        if let Err(e) = stream.send_message(response.into()).await {
                            eprintln!("Failed to send response: {}", e);
                            break;
                        }
                    }
                });
            }
        });
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::QuicClient;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_echo_server() -> Result<()> {
        // Start echo server
        let server = QuicServer::new()?;
        let mut listener = server.listen("127.0.0.1:0").await?;
        let server_addr = listener.local_addr()?;

        // Run echo server in background
        tokio::spawn(async move {
            while let Ok(connection) = listener.accept().await {
                tokio::spawn(async move {
                    while let Ok(mut stream) = connection.accept_stream().await {
                        tokio::spawn(async move {
                            while let Ok(message) = stream.receive_message().await {
                                let _ = stream.send_message(message).await;
                            }
                        });
                    }
                });
            }
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client and test
        let client = QuicClient::new()?;
        let connection = client.connect(&server_addr).await?;
        
        let response = timeout(
            Duration::from_secs(5),
            connection.request("Hello, Server!")
        ).await??;

        assert_eq!(response.as_ref(), b"Hello, Server!");
        Ok(())
    }
}