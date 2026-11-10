//! One client, two transports. All timestamps and phase logic live in
//! `comparison::bench`; only the backend differs.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use comparison::bench::{run, Backend, BenchConfig};
use comparison::grpc::GrpcBackend;
use comparison::quic::{QuicBackend, QuicProfile};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "grpc")]
    transport: String,
    #[arg(long, default_value = "127.0.0.1:50051")]
    addr: String,
    #[arg(long, default_value_t = 5.0)]
    duration: f64,
    #[arg(long, default_value_t = 3.0)]
    warmup: f64,
    #[arg(long, default_value_t = 16)]
    connections: usize,
    #[arg(long, default_value_t = 4)]
    streams: usize,
    #[arg(long, default_value_t = 64)]
    depth: usize,
    #[arg(long, default_value_t = 1024)]
    size: usize,
    #[arg(long, default_value_t = 64)]
    batch: usize,
    /// Server worker count, for the report only.
    #[arg(long, default_value_t = 4)]
    workers: usize,
    #[arg(long, default_value_t = 0.0)]
    delay_ms: f64,
    #[arg(long, default_value = "")]
    label: String,
    /// QuicCrunch client runtime threads (one per TquicClient).
    #[arg(long, default_value_t = 8)]
    client_runtimes: usize,
    /// Tokio worker threads for the task runtime. Total client threads is
    /// tokio_threads + client_runtimes for QUIC, and tokio_threads for gRPC.
    #[arg(long, default_value_t = 2)]
    tokio_threads: usize,
    #[arg(long, default_value = "bench")]
    quic_profile: String,
    #[arg(long, default_value = comparison::DEFAULT_CA)]
    ca: String,
    #[arg(long, default_value = comparison::DEFAULT_DOMAIN)]
    server_name: String,
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_millis()
        .init();

    let args = Args::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.tokio_threads.max(1))
        .enable_all()
        .build()?;
    rt.block_on(async_main(args))
}

async fn async_main(args: Args) -> Result<()> {
    let cfg = BenchConfig {
        addr: args.addr.clone(),
        duration: Duration::from_secs_f64(args.duration.max(0.5)),
        warmup: Duration::from_secs_f64(args.warmup.max(0.0)),
        connections: args.connections.max(1),
        streams_per_conn: args.streams.max(1),
        pipeline_depth: args.depth.max(1),
        msg_size: args.size,
        batch: args.batch.max(1),
        workers: args.workers,
        delay_ms: args.delay_ms,
        label: args.label.clone(),
    };

    let ca_pem =
        std::fs::read(&args.ca).map_err(|e| anyhow::anyhow!("read CA {}: {}", args.ca, e))?;

    let backend: Arc<dyn Backend> = match args.transport.as_str() {
        "grpc" | "tonic" | "tonic-h2" => {
            Arc::new(GrpcBackend::new(ca_pem, args.server_name.clone()))
        }
        "quic" | "quiccrunch" => {
            let profile = match args.quic_profile.as_str() {
                "default" | "production" => QuicProfile::Default,
                _ => QuicProfile::Bench,
            };
            Arc::new(QuicBackend::new(args.client_runtimes, profile, Some(args.ca.clone())).await?)
        }
        other => return Err(anyhow::anyhow!("unknown transport: {other}")),
    };

    println!(
        "=== comparison client ===\ntransport: {}  notes: {}\nconfig: server_workers={} conns={} streams/conn={} depth={} size={}B batch={} delay={}ms\nphases: warmup={:.1}s measurement={:.1}s log_level={}",
        backend.name(),
        backend.notes(),
        cfg.workers,
        cfg.connections,
        cfg.streams_per_conn,
        cfg.pipeline_depth,
        cfg.msg_size,
        cfg.batch,
        cfg.delay_ms,
        cfg.warmup.as_secs_f64(),
        cfg.duration.as_secs_f64(),
        log::max_level()
    );

    let report = run(backend, cfg).await?;

    println!("samples:           {}", report.counters.completed);
    println!("throughput:        {:.0} msg/s", report.throughput_msg_s);
    println!(
        "RTT p50/p95/p99/p99.9: {:.1} / {:.1} / {:.1} / {:.1} us",
        report.rtt_p50_us, report.rtt_p95_us, report.rtt_p99_us, report.rtt_p999_us
    );
    println!(
        "end-to-end p50/p95/p99: {:.1} / {:.1} / {:.1} us",
        report.e2e_p50_us, report.e2e_p95_us, report.e2e_p99_us
    );
    println!(
        "client queue p50/p99: {:.1} / {:.1} us",
        report.queue_p50_us, report.queue_p99_us
    );
    println!(
        "errors: {}  timeouts: {}  cutoff: {}  log level: {}",
        report.counters.errors() + report.missing_streams,
        report.counters.timeouts + report.missing_streams,
        report.counters.cutoff,
        report.log_level
    );
    if args.json {
        println!("{}", report.to_json());
    }
    Ok(())
}
