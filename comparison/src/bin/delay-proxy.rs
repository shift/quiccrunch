//! Userspace one-way delay proxy shared by both transports.
//!
//! `tc netem` needs CAP_NET_ADMIN, which this host does not grant, so the
//! comparison injects propagation delay in userspace. The same binary and the
//! same queueing/scheduler code serve TCP (tonic) and UDP (QUIC):
//!
//! * every read chunk / datagram is stamped with `now + delay`;
//! * one FIFO writer per direction drains the queue in order, sleeping until
//!   each deadline, so ordering inside a flow is preserved;
//! * a constant delay means the queue never reorders, it only decouples the
//!   read loop from the delayed send, which is what keeps line rate intact.
//!
//! TLS is end-to-end: the proxy is a byte relay and never terminates TLS.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, ValueEnum};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Tcp,
    Udp,
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, value_enum)]
    mode: Mode,
    #[arg(long)]
    listen: SocketAddr,
    #[arg(long)]
    upstream: SocketAddr,
    #[arg(long, default_value_t = 0.0)]
    delay_ms: f64,
}

type Delayed = mpsc::UnboundedSender<(Instant, Vec<u8>)>;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_millis()
        .init();
    let args = Args::parse();
    let delay = Duration::from_secs_f64((args.delay_ms / 1000.0).max(0.0));
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        log::warn!(
            "delay-proxy: mode={:?} listen={} upstream={} delay={:.3}ms",
            args.mode,
            args.listen,
            args.upstream,
            args.delay_ms
        );
        match args.mode {
            Mode::Tcp => run_tcp(args.listen, args.upstream, delay).await,
            Mode::Udp => run_udp(args.listen, args.upstream, delay).await,
        }
    })
}

/// Spawn a FIFO delayed writer and return the queue that feeds it.
fn delayed_writer<W>(mut writer: W) -> Delayed
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
    tokio::spawn(async move {
        while let Some((due, data)) = rx.recv().await {
            let now = Instant::now();
            if due > now {
                tokio::time::sleep(due - now).await;
            }
            if writer.write_all(&data).await.is_err() {
                break;
            }
            let _ = writer.flush().await;
        }
    });
    tx
}

async fn run_tcp(listen: SocketAddr, upstream: SocketAddr, delay: Duration) -> Result<()> {
    let listener = TcpListener::bind(listen).await?;
    log::warn!("delay-proxy: TCP listening on {}", listen);
    loop {
        let (client, peer) = listener.accept().await?;
        let _ = client.set_nodelay(true);
        tokio::spawn(async move {
            let server = match TcpStream::connect(upstream).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("delay-proxy: upstream connect failed: {}", e);
                    return;
                }
            };
            let _ = server.set_nodelay(true);
            let (mut cr, cw) = client.into_split();
            let (mut sr, sw) = server.into_split();

            let c2s_tx = delayed_writer(sw);
            let s2c_tx = delayed_writer(cw);

            let up = tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match cr.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if c2s_tx
                                .send((Instant::now() + delay, buf[..n].to_vec()))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            let down = tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match sr.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if s2c_tx
                                .send((Instant::now() + delay, buf[..n].to_vec()))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            let _ = tokio::join!(up, down);
            log::debug!("delay-proxy: TCP flow {} closed", peer);
        });
    }
}

async fn run_udp(listen: SocketAddr, upstream: SocketAddr, delay: Duration) -> Result<()> {
    let front = Arc::new(UdpSocket::bind(listen).await?);
    log::warn!("delay-proxy: UDP listening on {}", listen);
    // Client 4-tuple -> queue feeding that flow's upstream socket.
    let mut flows: HashMap<SocketAddr, Delayed> = HashMap::new();
    let mut buf = vec![0u8; 65535];
    loop {
        let (n, client) = front.recv_from(&mut buf).await?;
        let tx = match flows.get(&client) {
            Some(tx) => tx.clone(),
            None => {
                let tx = spawn_udp_flow(front.clone(), client, upstream, delay).await?;
                flows.insert(client, tx.clone());
                tx
            }
        };
        if tx
            .send((Instant::now() + delay, buf[..n].to_vec()))
            .is_err()
        {
            flows.remove(&client);
        }
    }
}

async fn spawn_udp_flow(
    front: Arc<UdpSocket>,
    client: SocketAddr,
    upstream: SocketAddr,
    delay: Duration,
) -> Result<Delayed> {
    let up = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    up.connect(upstream).await?;

    // client -> upstream
    let up_tx = delayed_datagram_writer(up.clone(), None);

    // upstream -> client, also delayed.
    let up2 = up.clone();
    let front2 = front.clone();
    tokio::spawn(async move {
        let back_tx = delayed_datagram_writer(front2, Some(client));
        let mut buf = vec![0u8; 65535];
        while let Ok(n) = up2.recv(&mut buf).await {
            if back_tx
                .send((Instant::now() + delay, buf[..n].to_vec()))
                .is_err()
            {
                break;
            }
        }
    });

    Ok(up_tx)
}

fn delayed_datagram_writer(sock: Arc<UdpSocket>, dest: Option<SocketAddr>) -> Delayed {
    let (tx, mut rx) = mpsc::unbounded_channel::<(Instant, Vec<u8>)>();
    tokio::spawn(async move {
        while let Some((due, data)) = rx.recv().await {
            let now = Instant::now();
            if due > now {
                tokio::time::sleep(due - now).await;
            }
            let sent = match dest {
                Some(addr) => sock.send_to(&data, addr).await,
                None => sock.send(&data).await,
            };
            if sent.is_err() {
                break;
            }
        }
    });
    tx
}
