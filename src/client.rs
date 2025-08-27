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

//! High-level QUIC client API for easy integration

use crate::transport::{QuicTransport, Transport, TlsConfig, Connection};
use crate::{Result, QuicStream};
use bytes::Bytes;

/// Easy-to-use QUIC client
pub struct QuicClient {
    transport: QuicTransport,
}

impl QuicClient {
    /// Create a new client with self-signed certificates (for testing)
    pub fn new() -> Result<Self> {
        let tls_config = crate::test_utils::certs::generate_self_signed_cert("quic-client")?;
        let transport = QuicTransport::new(tls_config)?;
        Ok(Self { transport })
    }

    /// Create a new client with custom TLS configuration
    pub fn with_tls_config(tls_config: TlsConfig) -> Result<Self> {
        let transport = QuicTransport::new(tls_config)?;
        Ok(Self { transport })
    }

    /// Connect to a server
    pub async fn connect(&self, addr: &str) -> Result<QuicConnection> {
        let connection = self.transport.connect(addr).await?;
        Ok(QuicConnection { inner: connection })
    }
}

/// A QUIC connection that provides easy-to-use stream operations
#[derive(Clone)]
pub struct QuicConnection {
    inner: Connection,
}

impl QuicConnection {
    /// Open a new bidirectional stream
    pub async fn open_stream(&self) -> Result<QuicStream> {
        self.inner.open_bi().await
    }

    /// Accept an incoming bidirectional stream
    pub async fn accept_stream(&self) -> Result<QuicStream> {
        self.inner.accept_bi().await
    }

    /// Send a message and receive a response (convenience method)
    pub async fn request(&self, message: impl Into<Bytes>) -> Result<Bytes> {
        let mut stream = self.open_stream().await?;
        stream.send_message(message.into()).await?;
        stream.receive_message().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::QuicServer;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_client_server_basic() -> Result<()> {
        // Start server
        let server = QuicServer::new()?;
        let addr = "127.0.0.1:0";
        let mut incoming = server.listen(addr).await?;
        let bound_addr = incoming.local_addr()?;

        // Accept connections in background
        tokio::spawn(async move {
            while let Ok(conn) = incoming.accept().await {
                tokio::spawn(async move {
                    while let Ok(mut stream) = conn.accept_stream().await {
                        tokio::spawn(async move {
                            while let Ok(msg) = stream.receive_message().await {
                                let response = format!("Echo: {}", 
                                    String::from_utf8_lossy(&msg));
                                let _ = stream.send_message(response.into()).await;
                            }
                        });
                    }
                });
            }
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client
        let client = QuicClient::new()?;
        let connection = client.connect(&bound_addr).await?;

        // Send request
        let response = timeout(
            Duration::from_secs(5),
            connection.request("Hello, World!")
        ).await??;

        assert_eq!(response.as_ref(), b"Echo: Hello, World!");
        Ok(())
    }
}