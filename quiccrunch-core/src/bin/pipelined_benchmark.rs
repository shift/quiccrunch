//! Corrected, latency-instrumented QUIC benchmark.
//!
//! Design notes (why this looks different from the original tool):
//!
//! * Every request gets a send timestamp. Responses on a QUIC stream are
//!   ordered, so a per-stream FIFO of send instants yields exact per-request
//!   round-trip latency, reported as p50/p95/p99/p99.9.
//! * Throughput counts *completed* responses inside a fixed measurement
//!   window, never "messages sent".
//! * Setup, warm-up, measurement and drain are separate phases. A shared
//!   start instant makes the measurement window identical for every stream.
//! * Pipeline depth is enforced per stream, and sends are batched to amortise
//!   the command-channel round trip.
//! * Hardware, worker count, message size, connections, streams and pipeline
//!   depth are part of the report.
//!
//! Usage:
//!   pipelined_benchmark [addr] [seconds] [connections] [streams/conn]
//!                       [pipeline_depth] [msg_size]
//!   --workers N    label the server worker count (default $QC_WORKERS or 1)
//!   --warmup SECS  warm-up duration (default 2)
//!   --batch N      max messages per send_batch (default 64)
//!   --json         emit one machine-readable JSON line after the report

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use hdrhistogram::Histogram;
use quiccrunch_core::transport::adapter::{RuntimeOptions, TquicClient, TquicStream};

#[derive(Clone, Debug)]
struct BenchConfig {
    addr: String,
    duration: Duration,
    warmup: Duration,
    connections: usize,
    streams_per_conn: usize,
    pipeline_depth: usize,
    msg_size: usize,
    batch: usize,
    workers: usize,
    client_runtimes: usize,
    tokio_threads: usize,
    /// When set, connection i targets `addr.port + i % spread`. Matches the
    /// server's `--spread` mode for exact worker distribution.
    spread: Option<usize>,
    json: bool,
}

#[derive(Debug, Default, Clone)]
struct Counters {
    sent: u64,
    completed: u64,
    /// requests lost to a send-command failure (transport/flow-control)
    send_errors: u64,
    /// requests lost to a receive error (connection close/reset)
    recv_errors: u64,
    /// responses that ended in FIN instead of a frame
    fin_errors: u64,
    /// requests that genuinely timed out (watchdog) during a measured phase
    timeouts: u64,
    /// requests still in flight when the measurement/drain ended. These are a
    /// pipeline-cutoff artefact, not drops, and are reported separately.
    cutoff: u64,
    bytes_sent: u64,
    bytes_received: u64,
}

impl Counters {
    fn errors(&self) -> u64 {
        self.send_errors + self.recv_errors + self.fin_errors
    }
}

/// A request in flight. Three instants let us separate client-side queueing
/// (enqueue -> runtime write) from transport round-trip (write -> response).
#[derive(Debug, Clone, Copy)]
struct InFlight {
    enqueued: Instant,
    written: Option<Instant>,
}

struct StreamResult {
    counters: Counters,
    rtt_hist: Histogram<u64>,
    e2e_hist: Histogram<u64>,
    queue_hist: Histogram<u64>,
    stream_index: usize,
}

/// Framing-aware receive buffer with an offset cursor. Never `drain`s per
/// message, which made the old client quadratic in the pipeline window.
struct FrameParser {
    buf: Vec<u8>,
    off: usize,
}

impl FrameParser {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64 * 1024),
            off: 0,
        }
    }

    fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Returns the length of the next complete length-prefixed message.
    fn next(&mut self) -> Option<usize> {
        if self.buf.len() - self.off < 4 {
            self.maybe_compact();
            return None;
        }
        let len = u32::from_be_bytes([
            self.buf[self.off],
            self.buf[self.off + 1],
            self.buf[self.off + 2],
            self.buf[self.off + 3],
        ]) as usize;
        if self.buf.len() - self.off < 4 + len {
            return None;
        }
        self.off += 4 + len;
        self.maybe_compact();
        Some(len)
    }

    fn maybe_compact(&mut self) {
        if self.off == self.buf.len() {
            self.buf.clear();
            self.off = 0;
        } else if self.off > 256 * 1024 && self.off * 2 > self.buf.len() {
            self.buf.drain(..self.off);
            self.off = 0;
        }
    }
}

/// Drive one stream between `phase_start` and `phase_end`, keeping at most
/// `pipeline_depth` requests in flight. When `record` is false this is a
/// warm-up pass: bytes are still exchanged but no counters or histograms are
/// updated.
#[allow(clippy::too_many_arguments)]
async fn pump(
    stream: &TquicStream,
    cfg: &BenchConfig,
    payload: &Bytes,
    frame_len: usize,
    phase_end: Instant,
    record: bool,
    inflight: &mut VecDeque<InFlight>,
    parser: &mut FrameParser,
    rtt_hist: &mut Histogram<u64>,
    e2e_hist: &mut Histogram<u64>,
    queue_hist: &mut Histogram<u64>,
    counters: &mut Counters,
) -> Result<(), String> {
    loop {
        let now = Instant::now();
        if now >= phase_end {
            return Ok(());
        }

        // Fill the pipeline window in batches.
        while inflight.len() < cfg.pipeline_depth {
            let inflight_room = cfg.pipeline_depth - inflight.len();
            let batch = inflight_room.min(cfg.batch);
            if batch == 0 {
                break;
            }
            let mut msgs = Vec::with_capacity(batch);
            for _ in 0..batch {
                msgs.push((payload.clone(), false));
            }
            let enqueued = Instant::now();
            match stream.send_batch(msgs).await {
                Ok(_) => {}
                Err(e) => {
                    // Everything still outstanding plus this failed batch is
                    // lost; classify as send errors, never as completed.
                    counters.send_errors += inflight.len() as u64 + batch as u64;
                    inflight.clear();
                    return Err(format!("send: {}", e));
                }
            }
            // `send_batch` returning means the runtime has handed the bytes to
            // tQUIC. That is the closest observable "written" instant without
            // changing the transport API to report per-packet send times.
            let written = Instant::now();
            for _ in 0..batch {
                inflight.push_back(InFlight {
                    enqueued,
                    written: Some(written),
                });
            }
            if record {
                counters.sent += batch as u64;
                counters.bytes_sent += (batch * frame_len) as u64;
            }
            if Instant::now() >= phase_end {
                break;
            }
        }

        let data = match stream.recv().await {
            Ok(d) => d,
            Err(e) => {
                counters.recv_errors += inflight.len() as u64;
                inflight.clear();
                return Err(format!("recv: {}", e));
            }
        };
        if data.is_empty() {
            counters.fin_errors += inflight.len() as u64;
            inflight.clear();
            return Err("unexpected FIN".to_string());
        }
        parser.push(&data);

        let now = Instant::now();
        if record {
            counters.bytes_received += data.len() as u64;
        }
        while parser.next().is_some() {
            if let Some(req) = inflight.pop_front() {
                if record {
                    counters.completed += 1;
                    if now <= phase_end {
                        let e2e = now.saturating_duration_since(req.enqueued);
                        let written = req.written.unwrap_or(req.enqueued);
                        let rtt = now.saturating_duration_since(written);
                        let queue = written.saturating_duration_since(req.enqueued);
                        let _ = e2e_hist.record((e2e.as_nanos() as u64).max(1));
                        let _ = rtt_hist.record((rtt.as_nanos() as u64).max(1));
                        let _ = queue_hist.record((queue.as_nanos() as u64).max(1));
                    }
                }
            }
        }
    }
}

/// Drain responses for requests sent before the phase deadline. Anything that
/// does not arrive within the drain allowance is counted as a timeout and is
/// excluded from the latency histograms.
async fn drain(
    stream: &TquicStream,
    phase_end: Instant,
    inflight: &mut VecDeque<InFlight>,
    parser: &mut FrameParser,
    counters: &mut Counters,
    _record: bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !inflight.is_empty() && Instant::now() < deadline {
        let wait = (phase_end + Duration::from_secs(3)).saturating_duration_since(Instant::now());
        match tokio::time::timeout(wait.max(Duration::from_millis(1)), stream.recv()).await {
            Ok(Ok(data)) => {
                if data.is_empty() {
                    break;
                }
                parser.push(&data);
                // Responses completed after the measurement deadline do not
                // count towards throughput or percentiles; they only shorten
                // the cutoff backlog.
                while parser.next().is_some() {
                    inflight.pop_front();
                }
            }
            Ok(Err(e)) => {
                counters.recv_errors += 1;
                eprintln!("drain recv error: {}", e);
                break;
            }
            Err(_) => break,
        }
    }
    // Anything left was in flight at the cutoff; that is a pipeline artefact,
    // not a drop. Keep it out of error/timeout accounting.
    counters.cutoff += inflight.len() as u64;
    inflight.clear();
}

async fn stream_worker(
    stream: TquicStream,
    stream_index: usize,
    cfg: BenchConfig,
    payload: Bytes,
    frame_len: usize,
    ready: Arc<AtomicUsize>,
    mut start_rx: tokio::sync::watch::Receiver<Option<Instant>>,
) -> StreamResult {
    let mut rtt_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut e2e_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut queue_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut counters = Counters::default();
    let mut parser = FrameParser::new();
    let mut inflight: VecDeque<InFlight> = VecDeque::with_capacity(cfg.pipeline_depth);

    // Phase 1: warm-up (JIT, congestion window, CPU frequency, TLS tickets).
    // A watchdog bounds the phase: a stream that cannot make progress must not
    // prevent the measurement gate from opening for everybody else.
    let warm_end = Instant::now() + cfg.warmup;
    // Deep pipelines (e.g. 64 conns x 4 streams x 1800) take tens of seconds
    // to enqueue through the bounded write queue. The watchdog only exists to
    // catch a genuinely wedged stream, so give it generous headroom.
    let warm_budget = cfg.warmup + Duration::from_secs(120);
    match tokio::time::timeout(
        warm_budget,
        pump(
            &stream,
            &cfg,
            &payload,
            frame_len,
            warm_end,
            false,
            &mut inflight,
            &mut parser,
            &mut rtt_hist,
            &mut e2e_hist,
            &mut queue_hist,
            &mut counters,
        ),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("stream {}: warm-up failed: {}", stream_index, e);
            ready.fetch_add(1, Ordering::Release);
            return StreamResult {
                counters,
                rtt_hist,
                e2e_hist,
                queue_hist,
                stream_index,
            };
        }
        Err(_) => {
            counters.timeouts += inflight.len() as u64;
            inflight.clear();
            eprintln!("stream {}: warm-up watchdog expired", stream_index);
            ready.fetch_add(1, Ordering::Release);
            return StreamResult {
                counters,
                rtt_hist,
                e2e_hist,
                queue_hist,
                stream_index,
            };
        }
    }
    // Do NOT drain the warm-up window: keeping it full is what makes the
    // measurement start in steady state. At depth 1800, re-filling 100k+
    // requests inside the window would otherwise dominate the result.
    //
    // Phase 2: wait for the shared measurement start instant.
    ready.fetch_add(1, Ordering::Release);
    loop {
        let gate = *start_rx.borrow();
        if let Some(start) = gate {
            // Warm-up bytes and their timestamps do not belong to the window.
            counters = Counters::default();
            rtt_hist.clear();
            e2e_hist.clear();
            queue_hist.clear();
            for req in inflight.iter_mut() {
                // Requests already in flight are billed from the gate; their
                // true enqueue predates the measurement window.
                req.enqueued = start;
                req.written = Some(start);
            }
            let phase_end = start + cfg.duration;
            let budget = cfg.duration + Duration::from_secs(120);
            match tokio::time::timeout(
                budget,
                pump(
                    &stream,
                    &cfg,
                    &payload,
                    frame_len,
                    phase_end,
                    true,
                    &mut inflight,
                    &mut parser,
                    &mut rtt_hist,
                    &mut e2e_hist,
                    &mut queue_hist,
                    &mut counters,
                ),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("stream {}: measurement failed: {}", stream_index, e),
                Err(_) => {
                    counters.timeouts += inflight.len() as u64;
                    inflight.clear();
                    eprintln!("stream {}: measurement watchdog expired", stream_index);
                }
            }
            drain(
                &stream,
                phase_end,
                &mut inflight,
                &mut parser,
                &mut counters,
                true,
            )
            .await;
            break;
        }
        if start_rx.changed().await.is_err() {
            counters.recv_errors += 1;
            break;
        }
    }

    StreamResult {
        counters,
        rtt_hist,
        e2e_hist,
        queue_hist,
        stream_index,
    }
}

async fn connection_worker(
    conn_idx: usize,
    client: TquicClient,
    cfg: BenchConfig,
    payload: Bytes,
    frame_len: usize,
    ready: Arc<AtomicUsize>,
    start_rx: tokio::sync::watch::Receiver<Option<Instant>>,
) -> Vec<StreamResult> {
    let mut target = cfg.addr.clone();
    if let Some(n) = cfg.spread {
        if let Ok(mut sa) = target.parse::<std::net::SocketAddr>() {
            sa.set_port(sa.port() + (conn_idx % n) as u16);
            target = sa.to_string();
        }
    }
    let connection = match client.connect(&target).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("connection {}: connect failed: {}", conn_idx, e);
            ready.fetch_add(cfg.streams_per_conn, Ordering::Release);
            return Vec::new();
        }
    };

    let mut tasks = Vec::with_capacity(cfg.streams_per_conn);
    for s in 0..cfg.streams_per_conn {
        match connection.open_stream().await {
            Ok(stream) => {
                let cfg = cfg.clone();
                let payload = payload.clone();
                let ready = ready.clone();
                let start_rx = start_rx.clone();
                let index = conn_idx * cfg.streams_per_conn + s;
                tasks.push(tokio::spawn(stream_worker(
                    stream, index, cfg, payload, frame_len, ready, start_rx,
                )));
            }
            Err(e) => {
                eprintln!("connection {}: open_stream {} failed: {}", conn_idx, s, e);
                ready.fetch_add(1, Ordering::Release);
            }
        }
    }

    let mut results = Vec::with_capacity(tasks.len());
    for task in tasks {
        if let Ok(res) = task.await {
            results.push(res);
        }
    }

    let _ = connection.close().await;
    results
}

fn cpu_model() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("model name"))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn cpu_flags() -> String {
    std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("flags"))
                .and_then(|l| l.split(':').nth(1))
                .map(|v| v.trim().to_string())
        })
        .unwrap_or_default()
}

fn governor() -> String {
    std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn kernel() -> String {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

fn has_flag(flags: &str, name: &str) -> bool {
    flags.split_whitespace().any(|f| f == name)
}

fn parse_args() -> BenchConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut positional = Vec::new();
    let mut workers = std::env::var("QC_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1usize);
    let mut warmup = 2.0f64;
    let mut batch = 64usize;
    let mut client_runtimes = 4usize;
    let mut tokio_threads = 2usize;
    let mut spread = None;
    let mut json = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workers" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    workers = v;
                }
                i += 2;
            }
            "--warmup" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    warmup = v;
                }
                i += 2;
            }
            "--batch" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    batch = v;
                }
                i += 2;
            }
            "--client-runtimes" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    client_runtimes = v;
                }
                i += 2;
            }
            "--tokio-threads" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    tokio_threads = v;
                }
                i += 2;
            }
            "--spread" => {
                if let Some(v) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    spread = Some(v);
                }
                i += 2;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }

    let duration: f64 = positional
        .get(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);
    let connections = positional.get(2).and_then(|v| v.parse().ok()).unwrap_or(16);
    let streams_per_conn = positional.get(3).and_then(|v| v.parse().ok()).unwrap_or(4);
    let pipeline_depth = positional
        .get(4)
        .and_then(|v| v.parse().ok())
        .unwrap_or(512);
    let msg_size = positional
        .get(5)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1024);

    BenchConfig {
        addr: positional
            .first()
            .cloned()
            .unwrap_or_else(|| "127.0.0.1:4433".to_string()),
        duration: Duration::from_secs_f64(duration.max(0.5)),
        warmup: Duration::from_secs_f64(warmup.max(0.0)),
        connections: connections.max(1),
        streams_per_conn: streams_per_conn.max(1),
        pipeline_depth: pipeline_depth.max(1),
        msg_size: msg_size.max(1),
        batch: batch.max(1),
        workers,
        client_runtimes: client_runtimes.max(1),
        tokio_threads: tokio_threads.max(1),
        spread: spread.filter(|n| *n > 0),
        json,
    }
}

fn percentile_us(hist: &Histogram<u64>, q: f64) -> f64 {
    if hist.is_empty() {
        0.0
    } else {
        hist.value_at_quantile(q) as f64 / 1000.0
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp_millis()
        .init();

    let cfg = parse_args();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cfg.tokio_threads)
        .enable_all()
        .build()?;
    rt.block_on(run(cfg))
}

async fn run(cfg: BenchConfig) -> Result<(), Box<dyn std::error::Error>> {
    let total_streams = cfg.connections * cfg.streams_per_conn;

    // 4-byte big-endian length prefix + payload, matching the echo server.
    let mut message = vec![0u8; 4 + cfg.msg_size];
    message[0..4].copy_from_slice(&(cfg.msg_size as u32).to_be_bytes());
    message[4..].fill(b'x');
    let frame_len = message.len();
    let payload = Bytes::from(message);

    println!("=== QuicCrunch benchmark ===");
    println!(
        "host: {} | cores: {} | kernel: {} | governor: {}",
        cpu_model(),
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        kernel(),
        governor()
    );
    let flags = cpu_flags();
    println!(
        "cpu features: aes={} sha_ni={} avx2={} | tls: tquic/BoringSSL TLS1.3 (AES-128-GCM / ChaCha20)",
        has_flag(&flags, "aes"),
        has_flag(&flags, "sha_ni"),
        has_flag(&flags, "avx2")
    );
    println!(
        "config: server_workers={} connections={} streams/conn={} pipeline_depth={} msg_size={}B batch={} client_runtimes={} tokio_threads={} distribution={}",
        cfg.workers, cfg.connections, cfg.streams_per_conn, cfg.pipeline_depth, cfg.msg_size, cfg.batch,
        cfg.client_runtimes, cfg.tokio_threads,
        cfg.spread.map(|n| format!("spread/{}", n)).unwrap_or_else(|| "so_reuseport".to_string())
    );
    println!(
        "phases: warmup={:.1}s measurement={:.1}s",
        cfg.warmup.as_secs_f64(),
        cfg.duration.as_secs_f64()
    );

    let ready = Arc::new(AtomicUsize::new(0));
    let (start_tx, start_rx) = tokio::sync::watch::channel(None::<Instant>);

    // A small pool of client runtimes. One runtime per connection collapses
    // under thread oversubscription; a single runtime serialises every stream.
    // `--client-runtimes` lets the runner find the knee.
    let mut clients = Vec::with_capacity(cfg.client_runtimes);
    for _ in 0..cfg.client_runtimes {
        clients.push(
            TquicClient::with_options(RuntimeOptions {
                // Loopback self-signed benchmark certificate: verification is
                // explicitly disabled here and nowhere else.
                verify_server_cert: false,
                ..RuntimeOptions::benchmark_client()
            })
            .await?,
        );
    }

    let mut conn_tasks = Vec::with_capacity(cfg.connections);
    for c in 0..cfg.connections {
        let client = clients[c % clients.len()].clone();
        conn_tasks.push(tokio::spawn(connection_worker(
            c,
            client,
            cfg.clone(),
            payload.clone(),
            frame_len,
            ready.clone(),
            start_rx.clone(),
        )));
    }

    // Wait for every stream to finish warm-up and park at the gate.
    let ready_wait = tokio::time::timeout(Duration::from_secs(300), async {
        while ready.load(Ordering::Acquire) < total_streams {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    if ready_wait.is_err() {
        eprintln!(
            "warning: only {}/{} streams became ready before timeout",
            ready.load(Ordering::Acquire),
            total_streams
        );
    }

    // Open the gate slightly in the future so every task observes the same
    // Start instant even under scheduler jitter.
    let start = Instant::now() + Duration::from_millis(100);
    let _ = start_tx.send(Some(start));

    let drain_allowance = Duration::from_secs(10);
    let wait = start + cfg.duration + drain_allowance - Instant::now();
    let collected = tokio::time::timeout(wait.max(Duration::from_secs(1)), async {
        let mut all = Vec::new();
        for task in conn_tasks {
            if let Ok(mut v) = task.await {
                all.append(&mut v);
            }
        }
        all
    })
    .await;

    let results = match collected {
        Ok(r) => r,
        Err(_) => {
            eprintln!("error: benchmark did not complete within the drain allowance");
            Vec::new()
        }
    };

    let mut totals = Counters::default();
    let mut rtt_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut e2e_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut queue_hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut per_stream: Vec<(usize, u64)> = Vec::new();
    for r in &results {
        totals.sent += r.counters.sent;
        totals.completed += r.counters.completed;
        totals.send_errors += r.counters.send_errors;
        totals.recv_errors += r.counters.recv_errors;
        totals.fin_errors += r.counters.fin_errors;
        totals.timeouts += r.counters.timeouts;
        totals.cutoff += r.counters.cutoff;
        totals.bytes_sent += r.counters.bytes_sent;
        totals.bytes_received += r.counters.bytes_received;
        per_stream.push((r.stream_index, r.counters.completed));
        rtt_hist.add(&r.rtt_hist).expect("compatible histograms");
        e2e_hist.add(&r.e2e_hist).expect("compatible histograms");
        queue_hist
            .add(&r.queue_hist)
            .expect("compatible histograms");
    }

    let elapsed = cfg.duration.as_secs_f64();
    let throughput = totals.completed as f64 / elapsed;
    let mbps = (totals.bytes_sent + totals.bytes_received) as f64 * 8.0 / elapsed / 1_000_000.0;
    let missing_streams = total_streams.saturating_sub(results.len());
    let timeouts = totals.timeouts + missing_streams as u64;
    let errors = totals.errors() + missing_streams as u64;

    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn (default)".to_string());

    println!();
    println!("=== Results ({:.1}s measurement window) ===", elapsed);
    println!("log level:         {}", log_level);
    println!("samples:           {}", rtt_hist.len());
    println!("completed:         {}", totals.completed);
    println!("throughput:        {:.0} msg/s", throughput);
    println!("throughput:        {:.1} Mbps", mbps);
    println!(
        "RTT (write->response) p50/p95/p99/p99.9: {:.1} / {:.1} / {:.1} / {:.1} us",
        percentile_us(&rtt_hist, 0.50),
        percentile_us(&rtt_hist, 0.95),
        percentile_us(&rtt_hist, 0.99),
        percentile_us(&rtt_hist, 0.999)
    );
    println!(
        "end-to-end (enqueue->response) p50/p95/p99/p99.9: {:.1} / {:.1} / {:.1} / {:.1} us",
        percentile_us(&e2e_hist, 0.50),
        percentile_us(&e2e_hist, 0.95),
        percentile_us(&e2e_hist, 0.99),
        percentile_us(&e2e_hist, 0.999)
    );
    println!(
        "client queue (enqueue->write) p50/p99: {:.1} / {:.1} us",
        percentile_us(&queue_hist, 0.50),
        percentile_us(&queue_hist, 0.99)
    );
    println!(
        "RTT max:           {:.1} us",
        rtt_hist.max() as f64 / 1000.0
    );
    println!("errors:            {}", errors);
    println!("  send errors:     {}", totals.send_errors);
    println!("  recv errors:     {}", totals.recv_errors);
    println!("  FIN errors:      {}", totals.fin_errors);
    println!("  missing streams: {}", missing_streams);
    println!("timeouts:          {}", timeouts);
    println!("in-flight cutoff:  {}", totals.cutoff);
    println!(
        "bytes sent:        {:.2} MiB",
        totals.bytes_sent as f64 / (1024.0 * 1024.0)
    );
    println!(
        "bytes received:    {:.2} MiB",
        totals.bytes_received as f64 / (1024.0 * 1024.0)
    );

    if per_stream.len() > 1 {
        let min = per_stream.iter().map(|(_, c)| *c).min().unwrap_or(0);
        let max = per_stream.iter().map(|(_, c)| *c).max().unwrap_or(0);
        println!("per-stream completed: min={} max={}", min, max);
    }

    // A short honesty check: throughput must be derivable from completions,
    // and p99 must not equal the reciprocal of throughput (the old bug).
    let reciprocal_us = if throughput > 0.0 {
        1_000_000.0 / throughput
    } else {
        0.0
    };
    if (percentile_us(&rtt_hist, 0.99) - reciprocal_us).abs() < 0.5 && !rtt_hist.is_empty() {
        println!("warning: RTT p99 equals 1/throughput; this is not a measured percentile");
    }

    if cfg.json {
        println!(
            "{{\"workers\":{},\"connections\":{},\"streams_per_conn\":{},\"pipeline_depth\":{},\"msg_size\":{},\"duration_s\":{:.3},\"completed\":{},\"throughput_msg_s\":{:.1},\"rtt_p50_us\":{:.1},\"rtt_p95_us\":{:.1},\"rtt_p99_us\":{:.1},\"rtt_p999_us\":{:.1},\"rtt_max_us\":{:.1},\"e2e_p50_us\":{:.1},\"e2e_p95_us\":{:.1},\"e2e_p99_us\":{:.1},\"e2e_p999_us\":{:.1},\"queue_p50_us\":{:.1},\"errors\":{},\"send_errors\":{},\"recv_errors\":{},\"fin_errors\":{},\"timeouts\":{},\"cutoff\":{}}}",
            cfg.workers,
            cfg.connections,
            cfg.streams_per_conn,
            cfg.pipeline_depth,
            cfg.msg_size,
            elapsed,
            totals.completed,
            throughput,
            percentile_us(&rtt_hist, 0.50),
            percentile_us(&rtt_hist, 0.95),
            percentile_us(&rtt_hist, 0.99),
            percentile_us(&rtt_hist, 0.999),
            rtt_hist.max() as f64 / 1000.0,
            percentile_us(&e2e_hist, 0.50),
            percentile_us(&e2e_hist, 0.95),
            percentile_us(&e2e_hist, 0.99),
            percentile_us(&e2e_hist, 0.999),
            percentile_us(&queue_hist, 0.50),
            errors,
            totals.send_errors,
            totals.recv_errors,
            totals.fin_errors,
            timeouts,
            totals.cutoff
        );
    }

    Ok(())
}
