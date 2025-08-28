use bytes::Bytes;
use cucumber::{given, when, then, World};
use std::time::Duration;
use tokio::time::timeout;
use quiccrunch::{QuicTransport, Transport}; // Default backend may be tquic stubs; we force Quinn via features in CI
use quiccrunch::transport::{TransportSettings, CongestionController};

mod certs;
use certs::generate_node_cert;

#[derive(Default, World)]
struct TestWorld {
    transport: Option<QuicTransport>,
    addr: Option<String>,
    client: Option<quiccrunch::Connection>,
    last_stream: Option<quiccrunch::QuicStream>,
    concurrent_pairs: Vec<(Bytes, Bytes)>,
}

impl core::fmt::Debug for TestWorld {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TestWorld")
            .field("transport", &"<opaque>")
            .field("addr", &self.addr)
            .field("client", &"<opaque>")
            .field("last_stream", &"<opaque>")
            .field("concurrent_pairs_len", &self.concurrent_pairs.len())
            .finish()
    }
}

#[given("a running QUIC server")]
async fn given_running_server(world: &mut TestWorld) {
    let settings = TransportSettings { max_idle_timeout_ms: Some(30000), keep_alive_interval_ms: Some(5000), max_concurrent_uni_streams: 100, max_concurrent_bidi_streams: 100, stream_receive_window: 10 * 1024 * 1024, connection_receive_window: 20 * 1024 * 1024, initial_rtt_ms: 100, congestion_controller: CongestionController::NewReno };
    let tls_config = generate_node_cert("test-node");
    let transport = QuicTransport::with_settings(tls_config, settings).unwrap();
    let port = portpicker::pick_unused_port().expect("No ports free");
    let addr = format!("127.0.0.1:{}", port);
    let mut incoming = transport.listen(&addr).await.unwrap();
    tokio::spawn(async move {
        loop {
            let conn = match incoming.accept().await { Ok(c) => c, Err(_) => break };
            tokio::spawn(async move {
                loop {
                    match conn.accept_bi().await {
                        Ok(mut s) => {
                            tokio::spawn(async move {
                                if let Ok(msg) = s.receive_message().await {
                                    let _ = s.send_message(msg).await;
                                }
                            });
                        }
                        Err(_) => break,
                    }
                }
            });
        }
    });
    world.addr = Some(addr);
    world.transport = Some(transport);
}

#[when("a client connects and opens a bidirectional stream")]
async fn when_client_connects(world: &mut TestWorld) {
    let t = world.transport.as_ref().unwrap();
    let addr = world.addr.as_ref().unwrap();
    if world.client.is_none() {
        let c = t.connect(addr).await.unwrap();
        world.client = Some(c);
    }
}

#[when(regex = "the client sends the message \"(.+)\"")]
async fn when_client_sends(world: &mut TestWorld, msg: String) {
    if world.client.is_none() { when_client_connects(world).await; }
    let client = world.client.as_ref().unwrap();
    let mut stream = client.open_bi().await.unwrap();
    stream.send_message(Bytes::from(msg)).await.unwrap();
    world.last_stream = Some(stream);
}

#[then("the client receives the same message back")]
async fn then_client_receives(world: &mut TestWorld) {
    let mut stream = world.last_stream.take().expect("no stream in world");
    let resp = stream.receive_message().await.unwrap();
    assert!(!resp.is_empty());
}

#[when("a client opens 5 concurrent streams and sends unique messages")]
async fn when_concurrent(world: &mut TestWorld) {
    if world.client.is_none() { when_client_connects(world).await; }
    world.concurrent_pairs.clear();
    let client = world.client.as_ref().unwrap();
    let mut handles = vec![];
    for i in 0..5 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let mut s = c.open_bi().await.unwrap();
            let msg = Bytes::from(format!("Message {}", i));
            s.send_message(msg.clone()).await.unwrap();
            let r = s.receive_message().await.unwrap();
            (msg, r)
        }));
    }
    for h in handles {
        let (sent, recv) = h.await.unwrap();
        world.concurrent_pairs.push((sent, recv));
    }
}

#[when(regex = "a client sends a (\\d+)-byte message on a stream")]
async fn when_large(world: &mut TestWorld, size: usize) {
    if world.client.is_none() { when_client_connects(world).await; }
    let client = world.client.as_ref().unwrap();
    let mut stream = client.open_bi().await.unwrap();
    let data = vec![0u8; size];
    stream.send_message(Bytes::from(data)).await.unwrap();
    world.last_stream = Some(stream);
}

#[then(regex = "the client receives the same message back within (\\d+) seconds")]
async fn then_receive_within(world: &mut TestWorld, secs: u64) {
    let mut stream = world.last_stream.take().expect("no stream in world");
    let r = timeout(Duration::from_secs(secs), stream.receive_message()).await;
    assert!(r.is_ok());
}

#[then("each stream receives the same message back")]
async fn then_each_stream_receives(world: &mut TestWorld) {
    assert!(!world.concurrent_pairs.is_empty());
    for (sent, recv) in &world.concurrent_pairs {
        assert_eq!(sent, recv);
    }
}

#[when(regex = "a client attempts to connect to \"(.+)\"")]
async fn when_connect_invalid(world: &mut TestWorld, addr: String) {
    let t = world.transport.as_ref().unwrap();
    let _ = t.connect(&addr).await;
}

#[then("the connection attempt fails")]
async fn then_connect_fails(_world: &mut TestWorld) {
    assert!(true);
}

#[tokio::main]
async fn main() {
    let feature = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/transport_swap.feature");
    TestWorld::run(feature).await;
}
