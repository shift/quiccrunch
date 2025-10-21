use mio::{Interest, Token};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tquic::{PacketInfo, PacketSendHandler, Result};

/// Async UDP socket wrapper that implements PacketSendHandler for tQUIC
pub struct AsyncQuicSocket {
    socket: Arc<UdpSocket>,
}

impl AsyncQuicSocket {
    pub async fn new(addr: &SocketAddr) -> io::Result<Self> {
        let socket = UdpSocket::bind(*addr).await?;
        Ok(AsyncQuicSocket {
            socket: Arc::new(socket),
        })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf).await
    }
}

impl PacketSendHandler for AsyncQuicSocket {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, PacketInfo)]) -> Result<usize> {
        // Note: This is still blocking for send operations
        // For full async, we'd need to spawn a task or use a channel
        // But tQUIC's PacketSendHandler is sync, so we keep this for compatibility
        let socket = self.socket.clone();
        let pkts = pkts.to_vec();
        let pkt_count = pkts.len(); // Clone the length before moving pkts

        // Spawn a blocking task for sends
        tokio::task::spawn_blocking(move || {
            let runtime = tokio::runtime::Handle::current();
            runtime.block_on(async {
                let mut count = 0;
                for (pkt, info) in pkts {
                    match socket.send_to(&pkt, info.dst).await {
                        Ok(_) => count += 1,
                        Err(_) => break,
                    }
                }
                count
            })
        });

        // Return immediately - actual send happens asynchronously
        Ok(pkt_count)
    }
}

/// Mio Registry wrapper for async operations
pub struct AsyncRegistry {
    // We don't actually need to register with mio since we're using tokio
    // This is just a compatibility layer
}

impl AsyncRegistry {
    pub fn new() -> Self {
        AsyncRegistry {}
    }

    pub fn register(
        &self,
        _socket: &mut AsyncQuicSocket,
        _token: Token,
        _interest: Interest,
    ) -> io::Result<()> {
        // No-op since we're using tokio's reactor
        Ok(())
    }
}

/// Poll wrapper for async operations
pub struct AsyncPoll {
    registry: AsyncRegistry,
}

impl AsyncPoll {
    pub fn new() -> io::Result<Self> {
        Ok(AsyncPoll {
            registry: AsyncRegistry::new(),
        })
    }

    pub fn registry(&self) -> &AsyncRegistry {
        &self.registry
    }

    pub async fn poll(
        &self,
        _events: &mut mio::Events,
        timeout: Option<std::time::Duration>,
    ) -> io::Result<()> {
        // In async context, we don't need to poll manually
        // Events are handled by tokio's reactor
        if let Some(timeout) = timeout {
            tokio::time::sleep(timeout).await;
        }
        Ok(())
    }
}
