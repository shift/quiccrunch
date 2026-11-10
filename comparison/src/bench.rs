//! Shared measurement core for the tonic/h2 vs QuicCrunch comparison.
//!
//! The instrumentation is deliberately the same as
//! `quiccrunch-core/src/bin/pipelined_benchmark.rs`:
//!
//! * every request is timestamped at enqueue, at write hand-off and at response;
//! * RTT is write -> response, end-to-end is enqueue -> response and the
//!   difference is reported as client queueing;
//! * throughput counts completions inside one fixed measurement window;
//! * warm-up, measurement and drain are distinct phases with a shared gate;
//! * errors and timeouts are counted separately and never enter histograms.
//!
//! The only thing that differs between transports is the [`Backend`] impl.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use bytes::Bytes;
use hdrhistogram::Histogram;

/// One measurement cell.
#[derive(Clone, Debug)]
pub struct BenchConfig {
    pub addr: String,
    pub duration: Duration,
    pub warmup: Duration,
    pub connections: usize,
    pub streams_per_conn: usize,
    pub pipeline_depth: usize,
    pub msg_size: usize,
    pub batch: usize,
    pub workers: usize,
    pub delay_ms: f64,
    pub label: String,
}

#[derive(Debug, Default, Clone)]
pub struct Counters {
    pub sent: u64,
    pub completed: u64,
    pub send_errors: u64,
    pub recv_errors: u64,
    pub fin_errors: u64,
    pub timeouts: u64,
    pub cutoff: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl Counters {
    pub fn errors(&self) -> u64 {
        self.send_errors + self.recv_errors + self.fin_errors
    }
}

#[derive(Debug, Clone, Copy)]
struct InFlight {
    enqueued: Instant,
    written: Option<Instant>,
    /// Request was already in flight when the measurement gate opened. Its
    /// enqueue predates the window, so it counts toward completion throughput
    /// (the work really finished inside the window) but must not enter the
    /// latency histograms: its true latency is unobservable and reporting it as
    /// `now - gate` produces a bogus zero-latency spike at deep pipelines.
    warmup: bool,
}

/// Number of response messages and bytes returned by one transport read.
#[derive(Debug, Default, Clone, Copy)]
pub struct RecvBatch {
    pub messages: u32,
    pub bytes: u64,
}

/// One logical RPC stream. `send_batch` must return only once the transport has
/// accepted the whole batch (queue admission), never after a network ACK.
#[async_trait::async_trait]
pub trait ClientStream: Send {
    async fn send_batch(&mut self, payload: &Bytes, count: usize) -> Result<()>;
    async fn recv(&mut self) -> Result<RecvBatch>;
}

/// One logical connection (one TCP connection / one QUIC connection).
#[async_trait::async_trait]
pub trait ClientConnection: Send + Sync {
    async fn open_stream(&self) -> Result<Box<dyn ClientStream>>;
}

/// Transport factory. One instance lives for the whole cell.
#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &'static str;
    /// Extra configuration facts printed in the report.
    fn notes(&self) -> String;
    async fn connect(&self, conn_index: usize, target: &str) -> Result<Box<dyn ClientConnection>>;
}

/// Framing-aware receive buffer with an offset cursor (identical to the
/// QuicCrunch benchmark). QUIC is a byte stream, so echo responses need the
/// 4-byte big-endian length prefix to be split back into messages.
pub struct FrameParser {
    buf: Vec<u8>,
    off: usize,
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(64 * 1024),
            off: 0,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_frame(&mut self) -> Option<usize> {
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

impl Default for FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

pub struct StreamResult {
    pub counters: Counters,
    pub rtt_hist: Histogram<u64>,
    pub e2e_hist: Histogram<u64>,
    pub queue_hist: Histogram<u64>,
    pub stream_index: usize,
}

#[allow(clippy::too_many_arguments)]
async fn pump(
    stream: &mut dyn ClientStream,
    cfg: &BenchConfig,
    payload: &Bytes,
    frame_len: usize,
    phase_end: Instant,
    record: bool,
    inflight: &mut VecDeque<InFlight>,
    rtt_hist: &mut Histogram<u64>,
    e2e_hist: &mut Histogram<u64>,
    queue_hist: &mut Histogram<u64>,
    counters: &mut Counters,
) -> Result<()> {
    loop {
        let now = Instant::now();
        if now >= phase_end {
            return Ok(());
        }

        while inflight.len() < cfg.pipeline_depth {
            let room = cfg.pipeline_depth - inflight.len();
            let batch = room.min(cfg.batch);
            if batch == 0 {
                break;
            }
            let enqueued = Instant::now();
            if let Err(e) = stream.send_batch(payload, batch).await {
                counters.send_errors += inflight.len() as u64 + batch as u64;
                inflight.clear();
                return Err(anyhow!("send: {}", e));
            }
            // Hand-off to the transport. Responsibility for the batch has moved
            // from the application into the transport queue.
            let written = Instant::now();
            for _ in 0..batch {
                inflight.push_back(InFlight {
                    enqueued,
                    written: Some(written),
                    warmup: false,
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

        let got = match stream.recv().await {
            Ok(got) => got,
            Err(e) => {
                counters.recv_errors += inflight.len() as u64;
                inflight.clear();
                return Err(anyhow!("recv: {}", e));
            }
        };
        if got.messages == 0 {
            if got.bytes == 0 {
                counters.fin_errors += inflight.len() as u64;
                inflight.clear();
                return Err(anyhow!("unexpected FIN"));
            }
            continue;
        }

        let now = Instant::now();
        if record {
            counters.bytes_received += got.bytes;
        }
        for _ in 0..got.messages {
            if let Some(req) = inflight.pop_front() {
                if record {
                    counters.completed += 1;
                    if !req.warmup && now <= phase_end {
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

async fn drain(
    stream: &mut dyn ClientStream,
    phase_end: Instant,
    inflight: &mut VecDeque<InFlight>,
    counters: &mut Counters,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !inflight.is_empty() && Instant::now() < deadline {
        let wait = (phase_end + Duration::from_secs(3)).saturating_duration_since(Instant::now());
        match tokio::time::timeout(wait.max(Duration::from_millis(1)), stream.recv()).await {
            Ok(Ok(got)) => {
                if got.messages == 0 {
                    break;
                }
                for _ in 0..got.messages {
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
    counters.cutoff += inflight.len() as u64;
    inflight.clear();
}

#[allow(clippy::too_many_arguments)]
async fn stream_worker(
    mut stream: Box<dyn ClientStream>,
    stream_index: usize,
    cfg: BenchConfig,
    payload: Bytes,
    frame_len: usize,
    ready: Arc<AtomicUsize>,
    mut start_rx: tokio::sync::watch::Receiver<Option<Instant>>,
) -> StreamResult {
    let new_hist =
        || Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut rtt_hist = new_hist();
    let mut e2e_hist = new_hist();
    let mut queue_hist = new_hist();
    let mut counters = Counters::default();
    let mut inflight: VecDeque<InFlight> = VecDeque::with_capacity(cfg.pipeline_depth);

    let warm_end = Instant::now() + cfg.warmup;
    let warm_budget = cfg.warmup + Duration::from_secs(120);
    let warm = tokio::time::timeout(
        warm_budget,
        pump(
            stream.as_mut(),
            &cfg,
            &payload,
            frame_len,
            warm_end,
            false,
            &mut inflight,
            &mut rtt_hist,
            &mut e2e_hist,
            &mut queue_hist,
            &mut counters,
        ),
    )
    .await;
    match warm {
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

    ready.fetch_add(1, Ordering::Release);
    loop {
        let gate = *start_rx.borrow();
        if let Some(start) = gate {
            counters = Counters::default();
            rtt_hist.clear();
            e2e_hist.clear();
            queue_hist.clear();
            for req in inflight.iter_mut() {
                req.enqueued = start;
                req.written = Some(start);
                req.warmup = true;
            }
            let phase_end = start + cfg.duration;
            let budget = cfg.duration + Duration::from_secs(120);
            let measured = tokio::time::timeout(
                budget,
                pump(
                    stream.as_mut(),
                    &cfg,
                    &payload,
                    frame_len,
                    phase_end,
                    true,
                    &mut inflight,
                    &mut rtt_hist,
                    &mut e2e_hist,
                    &mut queue_hist,
                    &mut counters,
                ),
            )
            .await;
            match measured {
                Ok(Ok(())) => {}
                Ok(Err(e)) => eprintln!("stream {}: measurement failed: {}", stream_index, e),
                Err(_) => {
                    counters.timeouts += inflight.len() as u64;
                    inflight.clear();
                    eprintln!("stream {}: measurement watchdog expired", stream_index);
                }
            }
            drain(stream.as_mut(), phase_end, &mut inflight, &mut counters).await;
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
    backend: Arc<dyn Backend>,
    cfg: BenchConfig,
    payload: Bytes,
    frame_len: usize,
    ready: Arc<AtomicUsize>,
    start_rx: tokio::sync::watch::Receiver<Option<Instant>>,
) -> Vec<StreamResult> {
    let connection = match backend.connect(conn_idx, &cfg.addr).await {
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
    results
}

pub fn percentile_us(hist: &Histogram<u64>, q: f64) -> f64 {
    if hist.is_empty() {
        0.0
    } else {
        hist.value_at_quantile(q) as f64 / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(len: usize) -> Vec<u8> {
        let mut v = vec![0u8; 4 + len];
        v[..4].copy_from_slice(&(len as u32).to_be_bytes());
        v[4..].fill(b'x');
        v
    }

    #[test]
    fn frame_parser_handles_partial_and_coalesced_frames() {
        let mut p = FrameParser::new();
        let a = frame(1024);
        let b = frame(1024);
        // Partial header.
        p.push(&a[..2]);
        assert_eq!(p.next_frame(), None);
        p.push(&a[2..]);
        assert_eq!(p.next_frame(), Some(1024));
        assert_eq!(p.next_frame(), None);
        // Two frames in one read.
        let mut both = a.clone();
        both.extend_from_slice(&b);
        p.push(&both);
        assert_eq!(p.next_frame(), Some(1024));
        assert_eq!(p.next_frame(), Some(1024));
        assert_eq!(p.next_frame(), None);
        // Empty payload is a valid zero-length frame.
        p.push(&frame(0));
        assert_eq!(p.next_frame(), Some(0));
    }

    #[test]
    fn percentiles_are_microseconds() {
        let mut h = Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).unwrap();
        for v in [1_000u64, 2_000, 3_000, 4_000] {
            h.record(v).unwrap();
        }
        assert!((percentile_us(&h, 0.50) - 2.0).abs() < 0.01);
        assert!((percentile_us(&h, 0.99) - 4.0).abs() < 0.01);
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    pub transport: String,
    pub notes: String,
    pub log_level: String,
    pub cfg: BenchConfig,
    pub counters: Counters,
    pub throughput_msg_s: f64,
    pub rtt_p50_us: f64,
    pub rtt_p95_us: f64,
    pub rtt_p99_us: f64,
    pub rtt_p999_us: f64,
    pub rtt_max_us: f64,
    pub e2e_p50_us: f64,
    pub e2e_p95_us: f64,
    pub e2e_p99_us: f64,
    pub e2e_p999_us: f64,
    pub queue_p50_us: f64,
    pub queue_p99_us: f64,
    pub missing_streams: u64,
}

impl Report {
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "transport": self.transport,
            "notes": self.notes,
            "log_level": self.log_level,
            "label": self.cfg.label,
            "workers": self.cfg.workers,
            "connections": self.cfg.connections,
            "streams_per_conn": self.cfg.streams_per_conn,
            "pipeline_depth": self.cfg.pipeline_depth,
            "msg_size": self.cfg.msg_size,
            "delay_ms": self.cfg.delay_ms,
            "duration_s": self.cfg.duration.as_secs_f64(),
            "warmup_s": self.cfg.warmup.as_secs_f64(),
            "completed": self.counters.completed,
            "throughput_msg_s": self.throughput_msg_s,
            "rtt_p50_us": self.rtt_p50_us,
            "rtt_p95_us": self.rtt_p95_us,
            "rtt_p99_us": self.rtt_p99_us,
            "rtt_p999_us": self.rtt_p999_us,
            "rtt_max_us": self.rtt_max_us,
            "e2e_p50_us": self.e2e_p50_us,
            "e2e_p95_us": self.e2e_p95_us,
            "e2e_p99_us": self.e2e_p99_us,
            "e2e_p999_us": self.e2e_p999_us,
            "queue_p50_us": self.queue_p50_us,
            "queue_p99_us": self.queue_p99_us,
            "errors": self.counters.errors() + self.missing_streams,
            "send_errors": self.counters.send_errors,
            "recv_errors": self.counters.recv_errors,
            "fin_errors": self.counters.fin_errors,
            "missing_streams": self.missing_streams,
            "timeouts": self.counters.timeouts + self.missing_streams,
            "cutoff": self.counters.cutoff,
            "bytes_sent": self.counters.bytes_sent,
            "bytes_received": self.counters.bytes_received,
        })
        .to_string()
    }
}

/// Run one cell with identical phase and pacing logic for every transport.
pub async fn run(backend: Arc<dyn Backend>, cfg: BenchConfig) -> Result<Report> {
    let total_streams = cfg.connections * cfg.streams_per_conn;
    // QUIC is a byte stream: every message carries the 4-byte big-endian
    // length prefix used by the existing QuicCrunch benchmark so responses can
    // be split back into messages. gRPC's protobuf/h2 framing does the same job
    // for tonic. The application payload is `msg_size` opaque bytes for both.
    let payload = if backend.name() == "quic" {
        let mut message = vec![0u8; 4 + cfg.msg_size];
        message[0..4].copy_from_slice(&(cfg.msg_size as u32).to_be_bytes());
        message[4..].fill(b'x');
        Bytes::from(message)
    } else {
        Bytes::from(vec![b'x'; cfg.msg_size])
    };
    let frame_len = payload.len();

    let ready = Arc::new(AtomicUsize::new(0));
    let (start_tx, start_rx) = tokio::sync::watch::channel(None::<Instant>);

    let mut conn_tasks = Vec::with_capacity(cfg.connections);
    for c in 0..cfg.connections {
        conn_tasks.push(tokio::spawn(connection_worker(
            c,
            backend.clone(),
            cfg.clone(),
            payload.clone(),
            frame_len,
            ready.clone(),
            start_rx.clone(),
        )));
    }

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
    let new_hist =
        || Histogram::<u64>::new_with_bounds(1, 60_000_000_000, 3).expect("valid histogram bounds");
    let mut rtt_hist = new_hist();
    let mut e2e_hist = new_hist();
    let mut queue_hist = new_hist();
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
        rtt_hist.add(&r.rtt_hist)?;
        e2e_hist.add(&r.e2e_hist)?;
        queue_hist.add(&r.queue_hist)?;
    }

    let elapsed = cfg.duration.as_secs_f64();
    let throughput = totals.completed as f64 / elapsed;
    let missing_streams = (total_streams.saturating_sub(results.len())) as u64;

    Ok(Report {
        transport: backend.name().to_string(),
        notes: backend.notes(),
        log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "warn (default)".to_string()),
        throughput_msg_s: throughput,
        rtt_p50_us: percentile_us(&rtt_hist, 0.50),
        rtt_p95_us: percentile_us(&rtt_hist, 0.95),
        rtt_p99_us: percentile_us(&rtt_hist, 0.99),
        rtt_p999_us: percentile_us(&rtt_hist, 0.999),
        rtt_max_us: rtt_hist.max() as f64 / 1000.0,
        e2e_p50_us: percentile_us(&e2e_hist, 0.50),
        e2e_p95_us: percentile_us(&e2e_hist, 0.95),
        e2e_p99_us: percentile_us(&e2e_hist, 0.99),
        e2e_p999_us: percentile_us(&e2e_hist, 0.999),
        queue_p50_us: percentile_us(&queue_hist, 0.50),
        queue_p99_us: percentile_us(&queue_hist, 0.99),
        counters: totals,
        cfg,
        missing_streams,
    })
}
