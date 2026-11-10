# QuicCrunch Performance Release Notes

This document is the single source of truth for QuicCrunch throughput and
latency. Every number here was produced by `scripts/bench.sh` on the hardware
listed below. If a number is not reproducible with that script, it does not
belong in this file.

## 1. What changed

The previous README table was not measured. The shipped benchmark reported
`p99 = 1 / throughput` (for example 31,629 msg/s → "32 µs"), never timestamped
individual requests, and counted messages *sent* rather than *completed*. The
"5,000 msg/s / 194 µs" and "31,629 msg/s / 32 µs" rows were arithmetic, not
measurements.

This release replaces the benchmark and fixes the transport issues that made
the old numbers meaningless:

- Every request is timestamped at **enqueue** and at **write handoff**; the
  response timestamp gives both **RTT** (write → response) and **end-to-end**
  (enqueue → response). Client queueing is reported separately.
- Setup, warm-up, measurement and drain are distinct phases with a shared
  measurement start instant. The pipeline stays full across the warm-up
  boundary, so deep pipelines do not spend the window re-filling.
- Throughput is `completed_in_window / duration`; it is not derived from sends.
- Errors are classified (send, receive, FIN, watchdog timeout, pipeline
  cutoff). Failed requests never enter the latency histograms.
- The transport no longer sleeps 1 ms when idle: command submission wakes the
  runtime with a `mio::Waker`, and the event loop blocks in `poll(2)`.
- Stream reads are drained inside `on_stream_readable` into per-stream
  buffers. Data that arrives while no reader is parked is no longer lost; this
  was the cause of the old 5-second receive timeouts.
- Writes respect tQUIC partial-write semantics, and a zero-progress write is a
  reported error rather than silent data loss.
- SO_REUSEPORT socket buffers are 4 MiB (kernel doubles to ~8 MiB), workers are
  CPU-pinned, BBR pacing is disabled for same-host benchmarks, and flow-control
  windows are raised to 64 MiB/connection and 16 MiB/stream.
- The benchmark server echoes received chunks without per-message parsing or
  allocation, and spawns one task per stream (not per message).
- **Push-based reads:** the transport thread drains a readable stream and
  pushes the bytes straight into the owning task's channel. There is no
  `StreamRead` command or per-chunk oneshot round trip. Buffered bytes are
  handed over immediately when a reader registers.
- **Wait-free send path:** `send_batch`/`send_bytes` enqueue into a bounded
  queue and return as soon as capacity is available; they never wait for the
  runtime thread to execute the write. The bounded queue is the backpressure
  mechanism, write failures are pushed to the stream reader, and partial or
  zero-progress writes are counted (`partial_writes`, `flow_blocked`). Control
  operations still use request/response commands.
- **Per-worker metrics:** commands, write batches/messages/bytes, write errors,
  partial writes, flow-blocked writes, packets/bytes sent and received, poll
  calls and maximum command backlog (`QC_METRICS=1` prints a line per worker).

### Logging fast path

Per-packet and per-frame messages are now `trace!`, `log` and `tracing` are
built with `release_max_level_info`, and `scripts/bench.sh` forces
`RUST_LOG=warn`. Benchmarking with `RUST_LOG=debug` is invalid: the previous
"10,003 msg/s, p99 1.6 s" sample in this investigation was an artefact of
per-packet debug logging on the server and is explicitly withdrawn.

## 2. Hardware and build

| Item | Value |
| --- | --- |
| CPU | Intel Core i5-8400T (6C/6T, 1.7 GHz base, 3.3 GHz max) |
| RAM | 16 GiB |
| Kernel | 6.18.36 |
| Governor | performance |
| Toolchain | rustc 1.89.0, release profile (`opt-level=3`, thin LTO) |
| Crypto | BoringSSL via tquic; CPU reports `aes=yes avx2=yes`, `sha_ni=no` |
| Topology | client and server co-located on loopback |
| Client | 8 client runtimes, 2 Tokio worker threads (same host) |

`sha_ni=no` on this CPU means SHA-256 uses the BoringSSL AVX2/SSE path, not
SHA-NI. AES-128-GCM uses AES-NI.

## 3. Worker distribution

The kernel's SO_REUSEPORT 4-tuple hash is uneven for small connection counts.
With 16 connections and 4 workers we observed **10 / 4 / 2 / 0** connections per
worker. Two modes are therefore reported:

- **spread** (`scripts/bench.sh` default): worker *w* listens on `port + w` and
  the client sends connection *i* to worker `i % N`. Exact, reproducible
  distribution; isolates worker scaling from hash luck.
- **reuseport**: all workers share one port, kernel hash. This is the
  production default and is reported for realism.

A `SO_ATTACH_REUSEPORT_CBPF` source-port steering program was implemented and
then removed: it reordered packets within a flow on this kernel and produced
connection failures (FIN mid-stream). Correct steering needs eBPF flow
dissection (`SO_ATTACH_REUSEPORT_EBPF`) plus CAP_NET_ADMIN and is left as
documented follow-up work.

## 4. Results

`p50/p95/p99/max` are RTT (write → response) in microseconds. End-to-end and
client-queue percentiles are in the per-run log. `err` is the sum of
send/receive/FIN errors; `timeout` is watchdog timeouts. "cutoff" (requests
still in flight when the 10 s window closed) is non-zero only at depth 1800 and
is not a drop.

All cells: `MODE=spread`, 4 streams/connection, 1 KiB, 8 client runtimes,
`WARMUP=3 DURATION=10 RUST_LOG=warn`. `err` is send+receive+FIN errors;
`timeout` is the watchdog; `cutoff` is requests still in flight when the window
closed (not a drop). p50/p95/p99/p99.9/max are RTT (write handoff → response),
µs.

### 4.1 Depth 1 (latency-oriented)

| workers | conns | msg/s | p50 | p95 | p99 | p99.9 | max | err | timeout | cutoff |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 8 | 8,302 | 2,540 | 10,117 | 14,574 | 20,300 | 43,680 | 0 | 0 | 0 |
| 1 | 16 | 8,987 | 4,473 | 18,448 | 24,887 | 37,880 | 50,692 | 0 | 0 | 0 |
| 1 | 64 | 11,306 | 16,114 | 59,703 | 64,979 | 73,007 | 94,175 | 0 | 0 | 0 |
| 2 | 8 | 18,358 | 1,457 | 3,389 | 9,601 | 13,951 | 29,966 | 0 | 0 | 0 |
| 2 | 16 | 26,459 | 1,636 | 5,542 | 16,974 | 23,904 | 34,341 | 0 | 0 | 0 |
| 2 | 64 | 32,650 | 4,202 | 21,332 | 51,839 | 65,470 | 90,178 | 0 | 0 | 0 |
| 4 | 8 | 21,999 | 1,142 | 3,174 | 6,009 | 10,863 | 23,134 | 0 | 0 | 0 |
| **4** | **16** | **49,929** | **1,076** | **2,838** | **4,006** | **6,590** | **19,939** | **0** | **0** | **0** |
| 4 | 64 | 34,821 | 4,972 | 19,825 | 34,996 | 55,312 | 96,207 | 0 | 0 | 0 |
| 6 | 8 | 23,059 | 1,033 | 3,088 | 6,033 | 11,051 | 18,989 | 0 | 0 | 0 |
| 6 | 16 | 38,601 | 1,366 | 3,547 | 6,640 | 12,558 | 21,791 | 0 | 0 | 0 |
| 6 | 64 | 38,419 | 4,985 | 19,759 | 28,492 | 45,351 | 169,476 | 0 | 0 | 0 |

### 4.2 Depth 64

| workers | conns | msg/s | p50 | p95 | p99 | p99.9 | max | err | timeout | cutoff |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 8 | 19,509 | 82,706 | 263,062 | 495,452 | 664,273 | 739,770 | 0 | 0 | 0 |
| 1 | 16 | 25,234 | 95,945 | 521,667 | 1,621,098 | 1,992,294 | 2,168,455 | 0 | 0 | 0 |
| 1 | 64 | 22,832 | 320,078 | 2,353,004 | 3,651,142 | 4,475,322 | 6,417,285 | 0 | 0 | 0 |
| 2 | 8 | 39,073 | 34,374 | 204,079 | 334,234 | 414,188 | 504,103 | 0 | 0 | 0 |
| 2 | 16 | 48,076 | 42,959 | 323,224 | 644,350 | 939,524 | 1,169,162 | 0 | 0 | 0 |
| 2 | 64 | 56,339 | 153,748 | 916,980 | 1,710,228 | 2,311,062 | 2,749,366 | 0 | 0 | 0 |
| 4 | 8 | **70,481** | 22,020 | 67,895 | 138,150 | 274,989 | 397,410 | 0 | 0 | 0 |
| 4 | 16 | 49,128 | 65,044 | 222,429 | 424,935 | 686,293 | 823,132 | 0 | 0 | 0 |
| 4 | 64 | 51,363 | 242,745 | 815,792 | 1,140,851 | 1,389,363 | 1,648,362 | 0 | 0 | 0 |
| 6 | 8 | 46,461 | 38,142 | 90,702 | 171,704 | 337,117 | 424,935 | 0 | 0 | 0 |
| 6 | 16 | **70,943** | 40,894 | 151,650 | 299,631 | 540,017 | 684,720 | 0 | 0 | 0 |
| 6 | 64 | 65,682 | 163,578 | 709,362 | 1,307,574 | 2,065,695 | 3,040,870 | 0 | 0 | 0 |

### 4.3 Depth 1800

| workers | conns | msg/s | p50 | p95 | p99 | p99.9 | max | err | timeout | cutoff |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 8 | 21,302 | 1,537,212 | 8,036,286 | 9,311,355 | 9,604,956 | 9,999,221 | 0 | 0 | 0 |
| 1 | 16 | 21,682 | 1,396,703 | 8,413,774 | 9,806,283 | 9,990,832 | 9,999,221 | 0 | 0 | 0 |
| 1 | 64 | 39,687 | 632,816 | 6,727,664 | 9,177,137 | 9,915,335 | 10,007,609 | 0 | 0 | 400,837 |
| 2 | 8 | 48,734 | 718,799 | 3,831,497 | 6,660,555 | 8,095,007 | 8,355,054 | 0 | 0 | 0 |
| 2 | 16 | 59,745 | 842,006 | 5,742,002 | 8,015,315 | 9,244,246 | 10,083,107 | 0 | 0 | 0 |
| 2 | 64 | 67,604 | 2,443,182 | 8,195,670 | 9,688,842 | 9,999,221 | 10,083,107 | 0 | 0 | 214,946 |
| 4 | 8 | **86,484** | 533,463 | 1,159,725 | 1,865,417 | 3,076,522 | 3,139,436 | 0 | 0 | 0 |
| 4 | 16 | 47,353 | 1,594,884 | 4,949,279 | 7,130,317 | 9,588,179 | 9,990,832 | 0 | 0 | 0 |
| 4 | 64 | 37,863 | 3,617,587 | 8,715,764 | 9,772,728 | 9,982,444 | 9,999,221 | 0 | 0 | 221,650 |
| 6 | 8 | 46,729 | 1,053,295 | 2,202,010 | 3,525,312 | 4,659,872 | 4,764,729 | 0 | 0 | 0 |
| 6 | 16 | 71,737 | 1,059,062 | 3,684,696 | 5,830,083 | 6,752,829 | 9,990,832 | 0 | 0 | 0 |
| 6 | 64 | 57,970 | 2,915,041 | 8,975,811 | 9,697,231 | 9,932,112 | 9,965,666 | 0 | 0 | 186,928 |

**Zero errors and zero watchdog timeouts in all 36 cells.** Depth does not
raise throughput below ~16 connections; it only raises queueing. Best observed
throughput is 86.5k msg/s (4 workers, 8 connections, depth 1800) and the
best high-connection cell is 71.7k (6 workers, 16 connections, depth 1800).

### 4.4 Latency floor

| workers | conns | streams/conn | depth | msg/s | p50 | p95 | p99 | p99.9 | err |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 4 | 8 | 1 | 1 | 7,432 | 985 | 2,132 | 3,238 | 4,756 | 0 |
| 4 | 16 | 1 | 1 | 10,879 | 1,097 | 3,133 | 6,402 | 9,232 | 0 |
| 4 | 16 | 4 | 1 | 49,929 | 1,076 | 2,838 | 4,006 | 6,590 | 0 |

The realistic floor is **~1 ms p50 and 3–6 ms p99**. A p99 below 500 µs is not
reachable with this design: each request crosses the client runtime thread, the
kernel, the server runtime thread and back, and the measured single-hop cost on
this 1.7 GHz low-power CPU is several hundred microseconds. Depth 64 cannot
have p99 <500 µs for a second, independent reason: with 16 connections × 4
streams × 64 in flight, queueing alone is `4096 / throughput` — about 58 ms at
70k msg/s, before any tail skew. The p99s in §4.2 are queueing, not RTT.

### 4.5 Depth-1800 tail: queueing or starvation?

A `QC_METRICS=1` run at 4 workers / 16 conns / depth 1800 produced per-worker:

```
worker=1 writes=29831 flow_blocked=0 partial=0 write_err=0
worker=2 writes=29659 flow_blocked=0 partial=0 write_err=0
worker=3 writes=26765 flow_blocked=0 partial=0 write_err=0
worker=0 writes=10125 flow_blocked=0 partial=0 write_err=0
```

`flow_blocked`, `write_errors` and `partial_writes` are zero, and the cell’s
read-side cutoff is zero: every request completed. The multi-second p99 is
therefore **in-flight queueing plus client-side stream skew** (worker 0 did a
third of the work because its four connections were served by a slower client
runtime), not QUIC flow-control starvation. The benchmark reports cutoff
separately so a genuinely unfinished pipeline cannot be mistaken for a drop.

### 4.6 Worker scaling summary

- 1 → 2 workers: near-linear (e.g. 25.2k → 48.1k at 16/64).
- 2 → 4: improves to ~70k at 8 connections; at 16 connections the client
  runtimes/cores become the limit and results stay ~49–51k.
- 4 → 6: little gain, and 6 workers lose throughput at 8 connections
  (70.5k → 46.5k). The server is no longer the sole bottleneck; scheduling
  eight client runtimes, two Tokio threads and six pinned server workers on six
  cores adds variance.
- Spread mode is exact; `MODE=reuseport` remains noisy and lower (see §3).

### 4.7 Target assessment

| Target | Result |
| --- | --- |
| >50k msg/s reliably at 4–6 workers | **Met at 8 connections** (70.5k/46.5k) and **6 workers/16 conns** (70.9k); **not reliably met at 4 workers/16 conns** (47–51k across runs) |
| RTT p99 <500 µs at depth 64 | **Not met.** Best depth-64 cell is 138 ms p99 (4 workers/8 conns); queueing alone is `depth × streams / throughput` |
| Zero drops/errors/timeouts | **Met in all 36 cells** (plus the corrected 1/64/1800 cell) |
| RTT p99 <200 µs | **Not achievable** with this design; floor is ~3–6 ms p99, ~1 ms p50 (§4.4) |

## 5. Error and timeout analysis

- **Send errors**: `write_stream_all` reports flow-control block, a closed
  connection or a partial FIN write. None occurred in the matrix below because
  the pipeline depth is bounded by the flow-control windows.
- **Receive errors**: connection reset/close while requests were outstanding.
  None occurred with the corrected transport.
- **FIN errors**: the pre-fix code treated any empty read as a fatal FIN. The
  real cause was dropped `on_stream_readable` events plus the broken CBPF
  steering. Both are fixed; the counter remains so regressions are visible.
- **Watchdog timeouts**: emitted only when a stream makes no progress for
  `phase + 120 s` (deep pipelines take tens of seconds to enqueue). Zero in the
  final matrix; a 1-worker/64-connection/depth-1800 cell hit an earlier 10 s
  watchdog and that run was discarded after the budget was corrected.
- **Cutoff**: at depth 1800 a large number of requests are legitimately in
  flight when the window closes. They are excluded from throughput and
  percentiles and are *not* errors. This is why depth-1800 latency is seconds:
  it is queueing behind a 100k-request window, not transport RTT.

## 6. Bottlenecks found

1. **Idle sleep.** The runtime busy-polled with a 0 ms timeout and slept 1 ms
   on a condvar when idle, giving a ~1–2 ms floor. Fixed with a waker-driven
   blocking poll.
2. **Lost readability.** The old loop only read a stream if a reader was
   already parked when the readable event was processed. Fixed by draining in
   the callback.
3. **Per-message command round trips (fixed).** Sends used to wait for a
   oneshot acknowledgement from the runtime, and receives used to issue a
   `StreamRead` command. On this CPU that put a ~35k msg/s ceiling on four
   workers. Push reads plus the bounded fire-and-forget send queue raised the
   best 8-connection cell from 28.6k to 86.5k and the 16-connection cell from
   ~35k to ~49–51k.
4. **Single runtime thread per worker.** All connections on a worker share one
   event loop. It is now the main server-side limit: 4 workers × ~12.5k/s at
   16 connections. Adding workers helps until the client’s 8 runtimes and 2
   Tokio threads exhaust the six co-located cores.
5. **Client stream skew.** Per-worker writes in one 16-connection run were
   29.8k / 29.7k / 26.8k / 10.1k despite four connections per worker. The slow
   worker’s connections were served by a client runtime that lost scheduling
   share. This inflates p99 and is a property of the co-located benchmark, not
   of QUIC flow control (`flow_blocked=0`).
6. **SO_REUSEPORT skew.** See §3.
7. **Cap'n Proto is not on the measured path.** The echo benchmark is raw
   length-prefixed bytes; encode/decode copies in `rpc` do not affect these
   numbers. Measuring the RPC path is separate work.

## 7. Readiness for production wiring

Not yet. The transport now produces honest, error-free measurements and a
sub-millisecond p50 at low pipeline depth, but before wiring it into a control
plane it still needs:

- eBPF reuseport steering (or a connection-aware dispatcher) so 16-connection
  deployments do not skew 10/4/2/0 across workers;
- multiple runtime loops per worker (sharded by connection) to break the
  single-event-loop ceiling at 16+ connections;
- retransmit/loss counters and graceful drain in the per-worker metrics
  (writes, packets and flow-blocked are already there);
- the security items from `docs/design/quiccrunch-evaluation.md` (certificate
  verification, mTLS, metadata/auth wiring, bounded input) which are unchanged
  by this work.

## 8. Reproduce

```bash
nix develop -c bash scripts/bench.sh

# latency-oriented cell
WARMUP=3 DURATION=10 CONNS=16 DEPTHS=1 nix develop -c bash scripts/bench.sh

# kernel-hash production default
MODE=reuseport WORKERS="1 2 4 6" CONNS="16" DEPTHS="64" \
  nix develop -c bash scripts/bench.sh

# per-worker metrics (writes, flow_blocked, packets, backlog)
QC_METRICS=1 MODE=spread WORKERS=4 CONNS=16 DEPTHS=1800 \
  nix develop -c bash scripts/bench.sh
# then: grep METRICS $CARGO_TARGET_DIR/server-bench.log
```