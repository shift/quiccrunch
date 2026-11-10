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
| Client | 4 client runtimes, 2 Tokio worker threads |

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

### 4.1 Spread mode, 4 workers, 4 streams/connection, 1 KiB

`WARMUP=3 DURATION=10 MODE=spread WORKERS=4 scripts/bench.sh`

| conns | depth | msg/s | p50 µs | p95 µs | p99 µs | max µs | err | timeout |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 8 | 1 | 8,466 | 1,702 | 7,479 | 11,788 | 34,439 | 0 | 0 |
| 8 | 64 | 28,602 | 54,952 | 159,646 | 262,013 | 403,440 | 0 | 0 |
| 8 | 1800 | 24,812 | 1,577,058 | 5,368,709 | 7,960,789 | 8,967,422 | 0 | 0 |
| 16 | 1 | 15,003 | 1,965 | 8,626 | 12,632 | 48,136 | 0 | 0 |
| **16** | **64** | **38,707** | **72,810** | **273,154** | **473,170** | **764,936** | **0** | **0** |
| 16 | 1800 | 25,140 | 3,118,465 | 8,149,533 | 8,959,033 | 9,831,448 | 0 | 0 |
| 32 | 1 | 12,653 | 5,071 | 17,728 | 29,327 | 124,387 | 0 | 0 |
| 32 | 64 | 22,765 | 246,940 | 908,067 | 1,387,266 | 2,835,350 | 0 | 0 |
| 32 | 1800 | 32,319 | 4,045,406 | 8,581,546 | 9,378,464 | 9,814,671 | 0 | 0 |
| 64 | 1 | 17,509 | 7,664 | 29,606 | 43,647 | 108,528 | 0 | 0 |
| 64 | 64 | 34,179 | 343,671 | 1,093,665 | 2,627,732 | 3,625,976 | 0 | 0 |
| 64 | 1800 | 37,780 | 4,609,540 | 9,336,521 | 9,915,335 | 9,999,220 | 0 | 0 |

**0 errors and 0 watchdog timeouts in every cell.** Latency grows roughly with
`pipeline_depth × streams / throughput`, which is Little's law: these are
queueing delays inside the pipeline window, not network RTT. Run-to-run
throughput variance is ±12% (see §4.2); single-cell numbers above are one run
each and the 16/64 cell repeated at 33–42k.

### 4.2 Worker scaling, 16 connections, depth 64, spread

Single runs, then a 3-run repeat of the 4-worker cell:

| workers | msg/s | p50 µs | p95 µs | p99 µs | max µs | err | timeout |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 21,832 | 108,921 | 625,476 | 1,033,896 | 1,357,906 | 0 | 0 |
| 2 | 35,998 | 72,024 | 373,293 | 553,124 | 669,516 | 0 | 0 |
| 4 (run A) | 42,082 | 66,388 | 225,575 | 458,752 | 648,020 | 0 | 0 |
| 4 (run B) | 34,751 | 68,354 | 313,262 | 594,018 | 1,151,336 | 0 | 0 |
| 4 (run C) | 33,184 | 63,144 | 281,805 | 557,842 | 6,924,796 | 0 | 0 |
| 4 (run D) | 35,541 | 76,087 | 299,631 | 475,267 | 1,077,936 | 0 | 0 |
| 6 | 39,660 | 63,210 | 248,119 | 479,461 | 2,787,115 | 0 | 0 |

**Honest assessment:** the 4-worker median over four observations is
**~35.0k msg/s** with a spread of 33.2k–42.1k (±12%). The `>40k msg/s` target is
**not reliably met**: one run crossed it, three did not. p99 is ~0.5 ms, not
<200 µs. Six workers do not improve throughput: the client runtimes and Tokio
threads on the remaining cores become the limit, not the server.

The 42,082 row in earlier drafts of this document was a single favourable run;
it is retained only for traceability and must not be quoted as the headline.

### 4.3 Production default (SO_REUSEPORT kernel hash), 16 connections, depth 64

| workers | msg/s | p50 µs | p99 µs | err | timeout |
| --- | --- | --- | --- | --- | --- |
| 1 | 13,095 | 195,428 | 1,632,633 | 0 | 0 |
| 2 | 22,983 | 113,770 | 742,392 | 0 | 0 |
| 4 | 32,749 | 81,789 | 587,203 | 0 | 0 |
| 6 | 24,347 | 83,820 | 1,429,209 | 0 | 0 |

The kernel hash is the dominant source of variance: 6 workers are *slower*
than 4 because several connections pile onto the same worker. No errors.

### 4.4 Latency-oriented cells, 4 workers, 1 stream/connection

| conns | depth | msg/s | p50 µs | p95 µs | p99 µs | max µs | err | timeout |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 16 | 1 | 8,341 | 850 | 4,010 | 7,139 | 23,101 | 0 | 0 |
| 16 | 64 | 34,504 | 18,219 | 63,373 | 96,141 | 150,208 | 0 | 0 |

The minimum observed RTT p50 is **850 µs** with 16 in-flight requests. The
target of p99 < 200 µs is **not met**: the per-request path still includes two
command-channel handoffs (tokio → runtime → tokio), each costing hundreds of
microseconds at p99. This is the next engineering item (push-based reads and a
wait-free send path), not a measurement artefact.

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
  `phase + 10 s`. None in the matrix.
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
3. **Per-message command round trips.** Each send/recv is an mpsc command plus
   a oneshot wake. With one in-flight request per stream this dominates and
   caps a connection at a few thousand msg/s. Batching (`send_batch`) amortises
   it; batching is what makes depth ≥ 64 performant.
4. **Single runtime thread per worker.** All connections on a worker share one
   event loop; more streams do not add parallelism beyond that thread. This is
   the remaining scaling limit.
5. **SO_REUSEPORT skew.** See §3.
6. **Cap'n Proto is not on the measured path.** The echo benchmark is raw
   length-prefixed bytes; encode/decode copies in `rpc` do not affect these
   numbers. Measuring the RPC path is separate work.

## 7. Readiness for production wiring

Not yet. The transport now produces honest, error-free measurements and a
sub-millisecond p50 at low pipeline depth, but before wiring it into a control
plane it still needs:

- eBPF reuseport steering (or a connection-aware dispatcher) so 16-connection
  deployments do not skew 10/4/2/0 across workers;
- a push-based read path to remove one command round trip per chunk and raise
  per-worker throughput;
- per-worker metrics (connections, packets, drops, retransmits) and graceful
  drain;
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
```