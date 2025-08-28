use std::time::Duration;
use clap::Parser;
use bytes::Bytes;
use hdrhistogram::Histogram; // requires dev-dependency
use quiccrunch_core::test_utils::certs;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    addr: String,
    #[arg(long, default_value_t = 1)]
    streams: usize,
    #[arg(long, default_value_t = 16384)]
    size: usize,
    #[arg(long, default_value_t = 30)]
    duration: u64,
}

#[tokio::main]
async fn main() -> quiccrunch::Result<()> {
    tracing_subscriber::fmt::try_init().ok();
    let args = Args::parse();

    let tls_cfg = crate::certs::generate_node_cert("quic-client");
    let (cert, key, ca) = (tls_cfg.cert, tls_cfg.key, tls_cfg.ca_cert);
    let tls = quiccrunch::transport::TlsConfig { cert, key, ca_cert: ca };
    let adv = quiccrunch::config::QuicTransportSettings { backend: quiccrunch::config::Backend::Quinn, ..Default::default() };
    let transport = quiccrunch::transport::QuicTransport::with_settings_advanced(tls, adv)?;

    use quiccrunch_core::Transport; // bring trait into scope
    let conn = transport.connect(&args.addr).await?;

    let payload = Bytes::from(vec![0u8; args.size]);
    let end_at = tokio::time::Instant::now() + Duration::from_secs(args.duration);

    let mut tasks = Vec::new();
    for _ in 0..args.streams {
        let conn = conn.clone();
        let payload = payload.clone();
        tasks.push(tokio::spawn(async move {
            let mut stream = conn.open_bi().await.expect("open_bi");
            let mut hist = Histogram::<u64>::new(3).unwrap();
            let mut bytes: u64 = 0;
            loop {
                if tokio::time::Instant::now() >= end_at { break; }
                let start = tokio::time::Instant::now();
                stream.send_message(payload.clone()).await.expect("send");
                let _ = stream.receive_message().await.expect("recv");
                let dur = tokio::time::Instant::now() - start;
                let us = dur.as_micros() as u64;
                let _ = hist.record(us);
                bytes += args.size as u64;
            }
            (hist, bytes)
        }));
    }

    let mut agg = Histogram::<u64>::new(3).unwrap();
    let mut total_bytes = 0u64;
    for t in tasks { let (h, b) = t.await.unwrap(); agg.add(h).unwrap(); total_bytes += b; }

    let secs = args.duration as f64;
    let mib = total_bytes as f64 / (1024.0 * 1024.0);
    let thr = mib / secs;

    println!("RESULT {{\"throughput_mib_s\":{:.3},\"p50_ms\":{:.3},\"p95_ms\":{:.3},\"p99_ms\":{:.3}}}",
        thr,
        agg.value_at_quantile(0.50) as f64 / 1000.0,
        agg.value_at_quantile(0.95) as f64 / 1000.0,
        agg.value_at_quantile(0.99) as f64 / 1000.0);

    Ok(())
}
