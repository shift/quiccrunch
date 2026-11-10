//! QuicCrunch backend for the comparison harness.
//!
//! The measured path is the production transport adapter
//! (`quiccrunch_core::transport::adapter`), the same code that
//! `quiccrunch-core/src/bin/pipelined_benchmark.rs` exercises.

use anyhow::{Context, Result};
use bytes::Bytes;
use quiccrunch_core::transport::adapter::{
    RuntimeOptions, TquicClient, TquicConnection, TquicStream,
};

use crate::bench::{Backend, ClientConnection, ClientStream, FrameParser, RecvBatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicProfile {
    /// `RuntimeOptions::benchmark*`: pacing off, deep windows. This is the
    /// profile used by the published 36-cell matrix.
    Bench,
    /// `RuntimeOptions::default`: BBR pacing on, production windows.
    Default,
}

pub struct QuicBackend {
    clients: Vec<TquicClient>,
    profile: QuicProfile,
    verify: bool,
}

impl QuicBackend {
    pub async fn new(
        runtimes: usize,
        profile: QuicProfile,
        ca_file: Option<String>,
    ) -> Result<Self> {
        let mut options = match profile {
            QuicProfile::Bench => RuntimeOptions::benchmark_client(),
            QuicProfile::Default => RuntimeOptions::default(),
        };
        if let Some(ca) = ca_file.as_ref() {
            // Verification is on by default; this only supplies the test CA.
            // The harness never disables it, including on loopback.
            options.verify_server_cert = true;
            options.ca_cert_file = Some(ca.clone());
        }
        let verify = options.verify_server_cert;
        let mut clients = Vec::with_capacity(runtimes.max(1));
        for _ in 0..runtimes.max(1) {
            clients.push(
                TquicClient::with_options(options.clone())
                    .await
                    .context("create TquicClient")?,
            );
        }
        Ok(Self {
            clients,
            profile,
            verify,
        })
    }
}

impl Drop for QuicBackend {
    fn drop(&mut self) {
        // Drop the clients before the tokio runtime shuts down; each client
        // owns an OS thread that is joined by its Drop impl.
        self.clients.clear();
    }
}

#[async_trait::async_trait]
impl Backend for QuicBackend {
    fn name(&self) -> &'static str {
        "quic"
    }

    fn notes(&self) -> String {
        format!(
            "tQUIC 1.6 over UDP, TLS1.3, profile={:?}, verify_server_cert={}, client_runtimes={}, conn_window=128MiB stream_window=32MiB",
            self.profile,
            self.verify,
            self.clients.len()
        )
    }

    async fn connect(&self, conn_index: usize, target: &str) -> Result<Box<dyn ClientConnection>> {
        let client = &self.clients[conn_index % self.clients.len()];
        let connection = client
            .connect(target)
            .await
            .context("QUIC connect failed")?;
        Ok(Box::new(QuicConnection { connection }))
    }
}

pub struct QuicConnection {
    connection: TquicConnection,
}

#[async_trait::async_trait]
impl ClientConnection for QuicConnection {
    async fn open_stream(&self) -> Result<Box<dyn ClientStream>> {
        let stream = self
            .connection
            .open_stream()
            .await
            .context("QUIC open_stream failed")?;
        Ok(Box::new(QuicStream {
            stream,
            parser: FrameParser::new(),
        }))
    }
}

pub struct QuicStream {
    stream: TquicStream,
    parser: FrameParser,
}

#[async_trait::async_trait]
impl ClientStream for QuicStream {
    async fn send_batch(&mut self, payload: &Bytes, count: usize) -> Result<()> {
        let mut messages = Vec::with_capacity(count);
        for _ in 0..count {
            messages.push((payload.clone(), false));
        }
        self.stream
            .send_batch(messages)
            .await
            .context("QUIC send_batch failed")
    }

    async fn recv(&mut self) -> Result<RecvBatch> {
        let data = self.stream.recv().await.context("QUIC recv failed")?;
        if data.is_empty() {
            return Ok(RecvBatch::default());
        }
        self.parser.push(&data);
        let mut messages = 0u32;
        while self.parser.next_frame().is_some() {
            messages += 1;
        }
        Ok(RecvBatch {
            messages,
            bytes: data.len() as u64,
        })
    }
}
