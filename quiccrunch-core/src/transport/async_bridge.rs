use std::io;
use tokio::sync::{mpsc, oneshot};
use tokio::task;
use mio::net::UdpSocket;
use mio::{Poll, Token, Interest, Events};
use tquic::{Config, Endpoint, ConnectionId, TransportHandler, PacketSendHandler, TlsConfig, Connection};
use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;
use crate::QuicStream;

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

use std::rc::Rc;

/// tQUIC transport handler for mio event loop
use dashmap::DashMap;
use std::task::Waker;

#[derive(Clone)]
struct MyHandler {
    socket: Arc<UdpSocket>,
    accepted_tx: mpsc::Sender<ConnectionId>,
    waker_registry: Arc<DashMap<u64, Waker>>, // stream_id -> Waker
}

impl TransportHandler for MyHandler {
    fn on_conn_created(&mut self, _conn: &mut Connection) {
        println!("Connection created");
    }

    fn on_conn_established(&mut self, conn: &mut Connection) {
        println!("Connection established");
        // Note: connection_id() method doesn't exist in tquic 1.6, we'll need an alternative
        let _ = self.accepted_tx.try_send(conn.trace_id().clone());
    }

    fn on_conn_closed(&mut self, _conn: &mut Connection) {
        println!("Connection closed");
    }

    fn on_stream_created(&mut self, _conn: &mut Connection, stream_id: u64) {
        println!("Stream created: {:?}", stream_id);
    }

    fn on_stream_readable(&mut self, _conn: &mut Connection, stream_id: u64) {
        println!("Stream readable: {:?}", stream_id);
    }

    fn on_stream_writable(&mut self, _conn: &mut Connection, stream_id: u64) {
        println!("Stream writable: {:?}", stream_id);
    }

    fn on_stream_closed(&mut self, _conn: &mut Connection, stream_id: u64) {
        println!("Stream closed: {:?}", stream_id);
    }

    fn on_new_token(&mut self, _conn: &mut Connection, _token: Vec<u8>) {
        // Handle new token if needed
    }
}

impl PacketSendHandler for MyHandler {
    fn on_packets_send(&self, pkts: &[(Vec<u8>, tquic::PacketInfo)]) -> tquic::Result<usize> {
        let mut sent = 0;
        for (buf, packet) in pkts {
            match self.socket.send_to(buf, packet.dst) {
                Ok(_) => sent += 1,
                Err(e) => {
                    eprintln!("Send error: {}", e);
                    return Err(tquic::Error::Io(e));
                }
            }
        }
        Ok(sent)
    }
}

/// Async bridge for tQUIC operations
#[derive(Debug)]
pub struct TquicAsyncBridge {
    command_tx: mpsc::Sender<TquicCommand>,
    handle: task::JoinHandle<()>,
}

impl TquicAsyncBridge {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (command_tx, command_rx) = mpsc::channel(32);

        let handle = task::spawn(async move {
            Self::run_event_loop(command_rx).await;
        });

        Ok(TquicAsyncBridge { command_tx, handle })
    }

    async fn run_event_loop(mut command_rx: mpsc::Receiver<TquicCommand>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Create mio poll and registry
        let poll = mio::Poll::new()?;
        let mut events = Events::with_capacity(1024);
        
        // Setup UDP socket for tQUIC  
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let socket = UdpSocket::from_std(socket);

        // Create connection acceptance channel
        let (accepted_tx, _accepted_rx) = mpsc::channel::<ConnectionId>(32);

        // Setup tQUIC endpoint - using simplified config for now
        let config = Config::new();
        let tls_config = TlsConfig::default();
        
        let handler = MyHandler {
            socket: Arc::new(socket),
            accepted_tx: accepted_tx.clone(),
            waker_registry: Arc::new(DashMap::new()),
        };

        // Simplified endpoint creation - need to fix API compatibility
        let endpoint = std::sync::Arc::new(std::sync::Mutex::new(
            Endpoint::with_configs(config, Some(tls_config), handler, None, None, None)?
        ));

        // Register socket with poll
        poll.registry().register(&mut endpoint.handler().socket, Token(0), Interest::READABLE).unwrap();

        // State management - all tQUIC types stay in this thread
        let mut connections: HashMap<u64, Box<Connection>> = HashMap::new();
        let mut listeners: HashMap<u64, String> = HashMap::new();
        let mut next_conn_id = 1u64;
        let mut next_listener_id = 1u64;

        loop {
            // Handle commands
            while let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    TquicCommand::Connect { addr, response } => {
                        let addr: std::net::SocketAddr = addr.parse().unwrap();
                        let conn_id = next_conn_id;
                        next_conn_id += 1;

                        match endpoint.connect(addr, &[]) {
                            Ok(conn) => {
                                connections.insert(conn_id, conn);
                                let _ = response.send(Ok(conn_id));
                            }
                            Err(e) => {
                                let _ = response.send(Err(Box::new(e)));
                            }
                        }
                    }
                    TquicCommand::Listen { addr, response } => {
                        let listener_id = next_listener_id;
                        next_listener_id += 1;
                        listeners.insert(listener_id, addr);
                        let _ = response.send(Ok(listener_id));
                    }
                    TquicCommand::Accept { listener_id, response } => {
                        if listeners.contains_key(&listener_id) {
                            let conn_id = next_conn_id;
                            next_conn_id += 1;
                            // For demo, create a mock connection
                            let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
                            if let Ok(conn) = endpoint.connect(addr, &[]) {
                                connections.insert(conn_id, conn);
                                let _ = response.send(Ok(conn_id));
                            } else {
                                let _ = response.send(Err(Box::new(io::Error::new(io::ErrorKind::Other, "failed to create connection"))));
                            }
                        } else {
                            let _ = response.send(Err(Box::new(io::Error::new(io::ErrorKind::NotFound, "listener not found"))));
                        }
                    }
                    TquicCommand::Read { stream_id, len, response } => {
                        let mut found = false;
                        for conn in connections.values_mut() {
                            if let Ok(mut stream) = conn.stream_readable(stream_id) {
                                let mut buf = vec![0u8; len];
                                match stream.read(&mut buf) {
                                    Ok(n) => {
                                        buf.truncate(n);
                                        let _ = response.send(Ok(buf));
                                        found = true;
                                        break;
                                    }
                                    Err(e) => {
                                        let _ = response.send(Err(e));
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !found {
                            let _ = response.send(Err(io::Error::new(io::ErrorKind::NotFound, "stream not found")));
                        }
                    }
                    TquicCommand::Write { stream_id, data, response } => {
                        let mut found = false;
                        for conn in connections.values_mut() {
                            if let Ok(mut stream) = conn.stream_writable(stream_id) {
                                match stream.write(&data, false) {
                                    Ok(_) => {
                                        let _ = response.send(Ok(()));
                                        found = true;
                                        break;
                                    }
                                    Err(e) => {
                                        let _ = response.send(Err(e));
                                        found = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !found {
                            let _ = response.send(Err(io::Error::new(io::ErrorKind::NotFound, "stream not found")));
                        }
                    }
                    TquicCommand::Close { stream_id } => {
                        for conn in connections.values_mut() {
                            let _ = conn.stream_shutdown(stream_id, 0, &[]);
                        }
                    }
                    TquicCommand::Shutdown => return,
                }
            }

            // Poll for events
            if poll.poll(&mut events, Some(Duration::from_millis(10))).is_ok() {
                for event in events.iter() {
                    if event.token() == Token(0) && event.is_readable() {
                        let mut buf = [0u8; 65536];
                        if let Ok((len, addr)) = endpoint.handler().socket.recv_from(&mut buf) {
                            let pkt_info = tquic::PacketInfo {
                                src: addr,
                                dst: endpoint.handler().socket.local_addr().unwrap(),
                                time: std::time::Instant::now(),
                            };
                            let _ = endpoint.recv(&buf[..len], &pkt_info);
                        }
                    }
                }
            }

            // Process connections
            endpoint.process_connections();

            // Send packets
            endpoint.send_packets();

            // Small delay to prevent busy loop
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let _ = self.command_tx.send(TquicCommand::Shutdown).await;
        self.handle.await?;
        Ok(())
    }
}

/// Handle for async communication with tQUIC listener
#[derive(Debug)]
pub struct TquicListenerHandle {
    listener_id: u64,
    command_tx: mpsc::Sender<TquicCommand>,
}

impl TquicListenerHandle {
    pub async fn accept(&mut self) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync>> {
        let (response_tx, response_rx) = oneshot::channel();

        self.command_tx.send(TquicCommand::Accept {
            listener_id: self.listener_id,
            response: response_tx,
        }).await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "bridge closed")))?;

        let stream_id = response_rx.await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "response channel closed")))?;

        Ok(QuicStream::new_simple_tquic(self.command_tx.clone(), stream_id))
    }
}

/// Async tQUIC client
#[derive(Debug, Clone)]
pub struct AsyncTquicClient {
    bridge: Arc<TquicAsyncBridge>,
}

impl AsyncTquicClient {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let bridge = Arc::new(TquicAsyncBridge::new().await?);
        Ok(AsyncTquicClient { bridge })
    }

    pub async fn connect(&self, addr: &str) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (response_tx, response_rx) = oneshot::channel();

        self.bridge.command_tx.send(TquicCommand::Connect {
            addr: addr.to_string(),
            response: response_tx,
        }).await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "bridge closed")))?;

        let stream_id = response_rx.await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "response channel closed")))?;

        Ok(QuicStream::new_simple_tquic(self.bridge.command_tx.clone(), stream_id))
    }
}

/// Async tQUIC server
#[derive(Debug)]
pub struct AsyncTquicServer {
    bridge: Arc<TquicAsyncBridge>,
}

impl AsyncTquicServer {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let bridge = Arc::new(TquicAsyncBridge::new().await?);
        Ok(AsyncTquicServer { bridge })
    }

    pub async fn listen(&self, addr: &str) -> Result<AsyncTquicListener, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let (response_tx, response_rx) = oneshot::channel();

        self.bridge.command_tx.send(TquicCommand::Listen {
            addr: addr.to_string(),
            response: response_tx,
        }).await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "bridge closed")))?;

        let listener_id = response_rx.await.map_err(|_| Box::new(io::Error::new(io::ErrorKind::Other, "response channel closed")))?;

        Ok(AsyncTquicListener {
            handle: TquicListenerHandle {
                listener_id,
                command_tx: self.bridge.command_tx.clone(),
            }
        })
    }
}

/// Async tQUIC listener
#[derive(Debug)]
pub struct AsyncTquicListener {
    handle: TquicListenerHandle,
}

impl AsyncTquicListener {
    pub async fn accept(&mut self) -> Result<QuicStream, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.handle.accept().await
    }
}

#[async_trait::async_trait]
impl crate::transport::Listener for AsyncTquicListener {
    type Connection = QuicStream;
    
    async fn accept(&mut self) -> Result<Self::Connection, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.handle.accept().await
    }
}