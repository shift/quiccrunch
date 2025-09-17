use std::io;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;

use mio::net::UdpSocket;
use mio::Poll;
use mio::Events;
use mio::Registry;

use tquic::Config;
use tquic::Endpoint;
use tquic::Connection;
use tquic::TransportHandler;
use tquic::PacketInfo;
use tquic::PacketSendHandler;
use tquic::Result;

use crate::{QuicStream, config::ServerConfig, config::ClientConfig};

/// A simple UDP socket wrapper that implements PacketSendHandler
pub struct QuicSocket {
    socket: UdpSocket,
}

impl QuicSocket {
    pub fn new(addr: &SocketAddr, registry: &Registry) -> io::Result<Self> {
        let mut socket = UdpSocket::bind(*addr)?;
        registry.register(&mut socket, mio::Token(0), mio::Interest::READABLE)?;
        Ok(QuicSocket { socket })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    pub fn recv_from(&self, buf: &mut [u8], _token: mio::Token) -> io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf)
    }
}

impl PacketSendHandler for QuicSocket {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, PacketInfo)]) -> Result<usize> {
        let mut count = 0;
        for (pkt, info) in pkts {
            match self.socket.send_to(pkt, info.dst) {
                Ok(_) => count += 1,
                Err(_) => break,
            }
        }
        Ok(count)
    }
}

/// TLS configuration for QUIC connections
#[derive(Clone, Debug)]
pub struct TlsConfig {
    pub cert: rustls::Certificate,
    pub key: rustls::PrivateKey,
    pub ca_cert: rustls::Certificate,
    pub cert_pem: Vec<u8>,
    pub key_pem: Vec<u8>,
    pub ca_cert_pem: Vec<u8>,
    pub inner: Option<std::sync::Arc<tquic::TlsConfig>>,
}



struct ClientHandler;

impl TransportHandler for ClientHandler {
    fn on_conn_created(&mut self, _conn: &mut Connection) {}
    fn on_conn_established(&mut self, _conn: &mut Connection) {}
    fn on_conn_closed(&mut self, _conn: &mut Connection) {}
    fn on_stream_created(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_readable(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_writable(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_closed(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {}
}

struct ServerHandler;

impl TransportHandler for ServerHandler {
    fn on_conn_created(&mut self, _conn: &mut Connection) {}
    fn on_conn_established(&mut self, _conn: &mut Connection) {}
    fn on_conn_closed(&mut self, _conn: &mut Connection) {}
    fn on_stream_created(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_readable(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_writable(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_stream_closed(&mut self, _conn: &mut Connection, _stream_id: u64) {}
    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {}
}

/// tQUIC client implementation
#[derive(Clone, Debug)]
pub struct TquicClient {
    config: ClientConfig,
}

impl TquicClient {
    pub async fn new(config: ClientConfig) -> Result<Self, Box<dyn std::error::Error + 'static>> {
        Ok(TquicClient { config })
    }

    pub async fn connect(&self, addr: &str) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync>> {
        let addr: SocketAddr = addr.parse()?;
        tokio::task::spawn_blocking(move || {
            Self::connect_sync(addr)
        }).await?
    }

    fn connect_sync(addr: SocketAddr) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = Config::new()?;
        let tls_config = tquic::TlsConfig::new_client_config(
            vec![b"h3".to_vec(), b"http/0.9".to_vec()],
            false,
        )?;
        config.set_tls_config(tls_config);

        let poll = Poll::new()?;
        let registry = poll.registry();
        let local = "0.0.0.0:0".parse().unwrap();
        let sock = Rc::new(QuicSocket::new(&local, registry)?);
        let endpoint = Endpoint::new(Box::new(config), false, Box::new(ClientHandler), sock.clone())?;

        endpoint.connect(sock.local_addr(), addr, None, None, None, None)?;

        let endpoint = Arc::new(Mutex::new(endpoint));
        let mut events = Events::with_capacity(1024);
        let mut conn_index = None;
        let mut stream_id = None;

        loop {
            {
                let mut endpoint = endpoint.lock().unwrap();
                endpoint.process_connections()?;
                poll.poll(&mut events, endpoint.timeout())?;
                for event in events.iter() {
                    if event.is_readable() {
                        let mut buf = vec![0u8; 65536];
                        match sock.recv_from(&mut buf, event.token()) {
                            Ok((len, local, remote)) => {
                                let pkt_info = PacketInfo {
                                    src: remote,
                                    dst: local,
                                    time: std::time::Instant::now(),
                                };
                                let _ = endpoint.recv(&buf[..len], &pkt_info);
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(e) => return Err(Box::new(e)),
                        }
                    }
                }
                endpoint.on_timeout(std::time::Instant::now());

                for index in endpoint.conn_indices().collect::<Vec<_>>() {
                    let conn = endpoint.conn_get_mut(index)?;
                    if conn.is_established() && conn_index.is_none() {
                        conn_index = Some(index);
                        stream_id = Some(conn.stream_bidi_new()?);
                        break;
                    }
                }
                if conn_index.is_some() {
                    break;
                }
            }
        }

        let conn_index = conn_index.unwrap();
        let stream_id = stream_id.unwrap();
        Ok(QuicStream::new_tquic(endpoint, conn_index, stream_id))
    }
}

/// tQUIC server implementation
#[derive(Clone, Debug)]
pub struct TquicServer {
    config: ServerConfig,
}

impl TquicServer {
    pub async fn new(config: ServerConfig) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(TquicServer { config })
    }

    pub async fn listen(&self, addr: &str) -> Result<TquicListener, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let addr: SocketAddr = addr.parse()?;
        let mut config = Config::new()?;
        let tls_config = tquic::TlsConfig::new_server_config(
            "./cert.crt",
            "./cert.key",
            vec![b"h3".to_vec(), b"http/0.9".to_vec()],
            true,
        )?;
        config.set_tls_config(tls_config);

        let poll = Poll::new()?;
        let registry = poll.registry();
        let sock = Rc::new(QuicSocket::new(&addr, registry)?);
        let endpoint = Endpoint::new(Box::new(config), true, Box::new(ServerHandler), sock.clone())?;
        let endpoint = Arc::new(Mutex::new(endpoint));

        let (tx, rx) = tokio::sync::mpsc::channel(10);
        let endpoint_clone = endpoint.clone();
        let sock_clone = sock.clone();
        let handle = std::thread::spawn(move || {
            let mut events = Events::with_capacity(1024);
            loop {
                {
                    let mut endpoint = endpoint_clone.lock().unwrap();
                    let _ = endpoint.process_connections();
                    let _ = poll.poll(&mut events, endpoint.timeout());
                    for event in events.iter() {
                        if event.is_readable() {
                            let mut buf = vec![0u8; 65536];
                            match sock_clone.recv_from(&mut buf, event.token()) {
                                Ok((len, local, remote)) => {
                                    let pkt_info = PacketInfo {
                                        src: remote,
                                        dst: local,
                                        time: std::time::Instant::now(),
                                    };
                                    let _ = endpoint.recv(&buf[..len], &pkt_info);
                                }
                                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                                Err(_) => {}
                            }
                        }
                    }
                    endpoint.on_timeout(std::time::Instant::now());

                    for index in endpoint.conn_indices().collect::<Vec<_>>() {
                        if let Ok(conn) = endpoint.conn_get_mut(index) {
                            if conn.is_established() {
                                for stream_id in conn.streams().collect::<Vec<_>>() {
                                    if conn.stream_is_readable(stream_id) {
                                        let stream = QuicStream::new_real_tquic(endpoint_clone.clone(), index, stream_id, sock_clone.clone(), 64 * 1024 * 1024);
                                        let _ = tx.try_send(stream);
                                    }
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        Ok(TquicListener {
            accepted_rx: rx,
            _handle: handle,
        })
    }
}

/// tQUIC listener implementation
#[derive(Debug)]
pub struct TquicListener {
    accepted_rx: tokio::sync::mpsc::Receiver<QuicStream>,
    _handle: std::thread::JoinHandle<()>,
}

#[async_trait::async_trait]
impl crate::transport::Listener for TquicListener {
    type Connection = QuicStream;

    async fn accept(&mut self) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.accepted_rx.recv().await.ok_or_else(|| Box::new(std::io::Error::new(std::io::ErrorKind::Other, "listener closed")) as Box<dyn std::error::Error + Send + Sync>)
    }
}

/// tQUIC connection (alias for QuicStream)
pub type TquicConnection = QuicStream;
