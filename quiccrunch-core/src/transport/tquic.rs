use std::net::SocketAddr;
use std::rc::Rc;
use mio::{Interest, Poll, Registry, Token};
use socket2::{Domain, Protocol, Socket, Type};
use tquic::{Endpoint, PacketInfo, PacketSendHandler, Result};

/// Our custom socket wrapper that holds a socket configured by `socket2`.
pub struct MyConfiguredSocket {
    sock: mio::net::UdpSocket,
}

impl MyConfiguredSocket {
    /// Creates a new socket, configures it with `socket2`, and registers with mio.
    pub fn new(listen_addr: SocketAddr, registry: &Registry) -> std::io::Result<Self> {
        let domain = if listen_addr.is_ipv4() { Domain::IPV4 } else { Domain::IPV6 };
        let sock = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;

        // Set desired socket options here
        let twenty_mb = 20 * 1024 * 1024;
        sock.set_recv_buffer_size(twenty_mb)?;
        sock.set_send_buffer_size(twenty_mb)?;
        println!("Custom socket configured with 20MB send/recv buffers.");

        sock.set_reuse_address(true)?;
        sock.bind(&listen_addr.into())?;
        
        let mut sock = mio::net::UdpSocket::from_std(sock.into());
        registry.register(&mut sock, Token(0), Interest::READABLE)?;

        Ok(Self { sock })
    }

    /// Implement recv_from to be used in the event loop.
    pub fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.sock.recv_from(buf)
    }
    
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.sock.local_addr()
    }
}

/// Implement the trait that tquic requires for sending packets.
impl PacketSendHandler for MyConfiguredSocket {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, PacketInfo)]) -> Result<usize> {
        let mut count = 0;
        for (pkt, info) in pkts {
            match self.sock.send_to(pkt, info.dst) {
                Ok(_) => {
                    count += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Socket is busy, stop sending for now.
                    break;
                }
                Err(e) => {
                    // A real error occurred.
                    log::error!("send_to error: {:?}", e);
                    return Err(tquic::Error::Network(e.kind()));
                }
            }
        }
        Ok(count)
    }
}

