//! tQUIC transport adapters for the backend messaging contract.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;

use crate::transport::tquic::{TquicClient, TquicConnection, TquicListener, TquicStream};

use super::client::{Connector, FrameIo, MessagingClient, PooledConnection};
use super::retention::{SubscriberBudget, SubscriberId};
use super::server::{Dispatch, MessagingServer};
use super::{MessagingError, PeerIdentity, Result, MAX_ENVELOPE_BYTES};

/// Length-prefixed framing over a single bidirectional tQUIC stream.
pub struct TquicFrameIo {
    stream: TquicStream,
    buffer: Vec<u8>,
}

impl TquicFrameIo {
    pub fn new(stream: TquicStream) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
        }
    }

    pub fn into_stream(self) -> TquicStream {
        self.stream
    }
}

#[async_trait]
impl FrameIo for TquicFrameIo {
    async fn send_frame(&mut self, frame: &[u8]) -> Result<()> {
        if frame.len() > MAX_ENVELOPE_BYTES + 4 {
            return Err(MessagingError::FrameTooLarge {
                size: frame.len(),
                max: MAX_ENVELOPE_BYTES + 4,
            });
        }
        self.stream
            .send_bytes(Bytes::copy_from_slice(frame), false)
            .await
            .map_err(|e| MessagingError::Transport(e.to_string()))
    }

    async fn recv_frame(&mut self) -> Result<Vec<u8>> {
        loop {
            if self.buffer.len() >= 4 {
                let len = u32::from_be_bytes([
                    self.buffer[0],
                    self.buffer[1],
                    self.buffer[2],
                    self.buffer[3],
                ]) as usize;
                if len > MAX_ENVELOPE_BYTES {
                    return Err(MessagingError::FrameTooLarge {
                        size: len,
                        max: MAX_ENVELOPE_BYTES,
                    });
                }
                let total = 4 + len;
                if self.buffer.len() >= total {
                    let frame = self.buffer[..total].to_vec();
                    self.buffer.drain(..total);
                    return Ok(frame);
                }
            }

            let chunk = self
                .stream
                .receive(MAX_ENVELOPE_BYTES)
                .await
                .map_err(|e| MessagingError::Transport(e.to_string()))?;
            if chunk.is_empty() {
                return Err(MessagingError::Transport("stream closed".into()));
            }
            self.buffer.extend_from_slice(&chunk);
            if self.buffer.len() > MAX_ENVELOPE_BYTES + 4 {
                return Err(MessagingError::FrameTooLarge {
                    size: self.buffer.len(),
                    max: MAX_ENVELOPE_BYTES + 4,
                });
            }
        }
    }
}

/// Connector used by [`super::client::MessagingPool`].
pub struct TquicConnector {
    client: TquicClient,
}

impl TquicConnector {
    pub async fn new(client: TquicClient) -> Self {
        Self { client }
    }
}

struct TquicPooledConnection {
    connection: TquicConnection,
}

#[async_trait]
impl PooledConnection for TquicPooledConnection {
    async fn open_stream(&self) -> Result<Box<dyn FrameIo>> {
        let stream = self
            .connection
            .open_stream()
            .await
            .map_err(|e| MessagingError::Transport(e.to_string()))?;
        Ok(Box::new(TquicFrameIo::new(stream)))
    }

    async fn close(&self) {
        let _ = self.connection.close().await;
    }
}

#[async_trait]
impl Connector for TquicConnector {
    async fn connect(
        &self,
        address: &str,
        server_name: Option<&str>,
    ) -> Result<Box<dyn PooledConnection>> {
        let connection = self
            .client
            .connect_with_server_name(address, server_name)
            .await
            .map_err(|e| MessagingError::Transport(e.to_string()))?;
        Ok(Box::new(TquicPooledConnection { connection }))
    }
}

/// Connect a one-shot messaging client through an existing tQUIC client.
pub async fn connect(
    client: &TquicClient,
    address: &str,
    server_name: Option<&str>,
    identity: PeerIdentity,
    metadata: HashMap<String, String>,
) -> Result<MessagingClient> {
    let connection = client
        .connect_with_server_name(address, server_name)
        .await
        .map_err(|e| MessagingError::Transport(e.to_string()))?;
    let stream = connection
        .open_stream()
        .await
        .map_err(|e| MessagingError::Transport(e.to_string()))?;
    let mut messaging = MessagingClient::new(Box::new(TquicFrameIo::new(stream)), identity);
    for (key, value) in metadata {
        messaging = messaging.with_metadata(key, value);
    }
    Ok(messaging)
}

/// Accept connections and streams, dispatching each stream as one RPC.
pub async fn serve_listener(
    server: Arc<MessagingServer>,
    mut listener: TquicListener,
    peer: Option<PeerIdentity>,
) -> Result<()> {
    let next_subscriber = Arc::new(AtomicU64::new(1));
    loop {
        let connection = listener
            .accept()
            .await
            .map_err(|e| MessagingError::Transport(e.to_string()))?;
        let server = server.clone();
        let peer = peer.clone();
        let subscriber = next_subscriber.fetch_add(1, Ordering::Relaxed);
        let budget = server.new_subscriber_budget();
        tokio::spawn(async move {
            if let Err(err) =
                serve_connection_with_budget(server, connection, peer, subscriber, budget).await
            {
                tracing::debug!("messaging connection closed: {err}");
            }
        });
    }
}

/// Serve all streams on one connection until it closes.
pub async fn serve_connection(
    server: Arc<MessagingServer>,
    connection: TquicConnection,
    peer: Option<PeerIdentity>,
) -> Result<()> {
    let subscriber = 1;
    let budget = server.new_subscriber_budget();
    serve_connection_with_budget(server, connection, peer, subscriber, budget).await
}

async fn serve_connection_with_budget(
    server: Arc<MessagingServer>,
    connection: TquicConnection,
    peer: Option<PeerIdentity>,
    subscriber: SubscriberId,
    budget: Arc<SubscriberBudget>,
) -> Result<()> {
    loop {
        let stream = match connection.accept_stream().await {
            Ok(stream) => stream,
            Err(_) => return Ok(()),
        };
        let server = server.clone();
        let peer = peer.clone();
        let budget = budget.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_stream(stream, server, peer, subscriber, budget).await {
                tracing::debug!("messaging stream closed: {err}");
            }
        });
    }
}

async fn handle_stream(
    stream: TquicStream,
    server: Arc<MessagingServer>,
    peer: Option<PeerIdentity>,
    subscriber: SubscriberId,
    budget: Arc<SubscriberBudget>,
) -> Result<()> {
    let mut io = TquicFrameIo::new(stream);
    let frame = io.recv_frame().await?;
    match server
        .dispatch_with_budget(&frame, peer.as_ref(), subscriber, budget)
        .await
    {
        Dispatch::Frames(frames) => {
            for response in frames {
                send_with_deadline(&mut io, &response).await?;
            }
        }
        Dispatch::Watch(mut watch) => {
            while let Some(frame) = watch.next().await {
                // A subscriber that cannot accept a frame within the deadline
                // is treated as gone: drop the watch (unsubscribes) and close.
                send_with_deadline(&mut io, &frame).await?;
            }
        }
    }
    // Half-close the response direction.
    let _ = io.into_stream().send(&[], true).await;
    Ok(())
}

async fn send_with_deadline(io: &mut TquicFrameIo, frame: &[u8]) -> Result<()> {
    match tokio::time::timeout(Duration::from_secs(5), io.send_frame(frame)).await {
        Ok(result) => result,
        Err(_) => Err(MessagingError::SlowConsumer(
            "watch delivery exceeded the send deadline".into(),
        )),
    }
}
