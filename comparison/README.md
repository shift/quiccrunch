# tonic/gRPC vs QuicCrunch — same-shape comparison harness

This directory contains a deliberately boring benchmark: one logical RPC, two
transports, one client instrumentation, one delay proxy. It exists to answer a
single question with numbers instead of intuition: **does QUIC give the
distri-kv control plane anything measurable that tonic does not?**

The answer, and the full table, is in
[`../docs/GRPC_VS_QUICCRUNCH.md`](../docs/GRPC_VS_QUICCRUNCH.md). This file is
the methodology and reproduction guide.

## What is compared

| | transport A | transport B |
| --- | --- | --- |
| stack | tonic 0.10 / hyper h2 | QuicCrunch (tQUIC 1.6) |
| transport | HTTP/2 over TCP | QUIC over UDP |
| TLS | rustls 0.21, ALPN `h2` | BoringSSL (tQUIC), ALPN `h3`/`http/0.9` |
| server | `comparison/src/bin/grpc-server.rs`, 4 tokio worker threads | `quiccrunch-core/src/bin/multi_worker_server.rs --bench`, 4 worker threads |
| client | `comparison/src/bin/compare-client.rs --transport grpc` | the same binary, `--transport quic` |
| payload | 1 KiB opaque bytes, echoed | 1 KiB opaque bytes, echoed |
| per-conn parallelism | 4 bidirectional h2 streams | 4 QUIC streams |
| pipeline depth | 1 / 64 / 1800 requests in flight per stream | same |

Both sides terminate TLS and **verify the server certificate**. The harness
sets `RuntimeOptions::verify_server_cert = true` (a new, opt-in field on the
QUIC adapter; its default is unchanged for compatibility) and loads a throwaway
CA from `comparison/certs/`. Neither side uses mTLS: client certificates and
SPIFFE identity, which the distri-kv control plane needs, are not implemented
by either harness (see caveats).

The RPC is intentionally empty of business logic:

```proto
service Echo { rpc Echo(stream EchoRequest) returns (stream EchoResponse); }
message EchoRequest  { bytes payload = 1; }
message EchoResponse { bytes payload = 1; }
```

QUIC's echo server is byte-oriented (it echoes received chunks without parsing),
so the QUIC wire format is `4-byte big-endian length + 1 KiB` per message, exactly
as in the published QuicCrunch matrix. gRPC uses protobuf/h2 framing for the same
1 KiB payload. Framing overhead is therefore included on both sides and is part
of what is being measured.

## Instrumentation (identical on both transports)

`comparison/src/bench.rs` is a transport-agnostic port of the instrumentation
already used by `quiccrunch-core/src/bin/pipelined_benchmark.rs`. It was not
rewritten from scratch and the original binaries are untouched.

For every request:

* `enqueued` — sampled immediately before handing the batch to the transport;
* `written` — sampled when the transport accepted the whole batch (bounded
  queue admission: tQUIC's command queue for QUIC, the per-stream `tokio::mpsc`
  that feeds tonic for gRPC). Neither timestamp waits for a network ACK.
* `response` — sampled when the response is parsed/decoded.

Reported:

* **RTT** = `response - written` (p50/p95/p99/p99.9/max);
* **end-to-end** = `response - enqueued` (p50/p95/p99);
* **client queue** = `written - enqueued` (p50/p99).

Phases: connect + 3 s warm-up (excluded), one shared 5 s measurement window
(opened by a single `watch` gate 100 ms in the future), then a bounded drain.
Throughput is `completed_in_window / window`, never sends. Errors are classified
(send, receive, FIN, missing stream, watchdog timeout) and never enter the
histograms. Requests still in flight when the window closes are reported as
`cutoff`, not as errors.

Requests that were already in flight when the gate opened count toward
completion throughput — the server really did finish that work inside the
window — but they are excluded from the latency histograms, because their
enqueue predates the window and `now - gate` would report a bogus zero-latency
spike at depth 1800. This is the one behavioural difference from the original
`pipelined_benchmark` instrumentation, and it applies identically to both
transports.

Every cell is repeated **3 times**, transports run in **alternating order**
within each repeat, and the report uses the **median** of the three runs. The
1-minute load average is captured immediately before each run and stored in the
CSV next to the result. The effective log level is printed by the client and
recorded per row.

## Matrix

* connections: 1, 8, 16, 64
* pipeline depth per stream: 1, 64, 1800
* one-way delay: 0 ms (direct loopback), 1 ms, 10 ms (userspace proxy)
* server workers: 4; plus a QuicCrunch-only 1-worker scaling check at 16 and
  64 connections for all three depths
* payload: 1 KiB (empty-payload baseline for proxy calibration)
* client threads: 10 total on both sides — gRPC: 10 tokio threads; QUIC: 8
  `TquicClient` runtime threads + 2 tokio threads. This equalises CPU budget;
  it is also an honest statement of the QUIC client's architecture.

## Delay proxy and its overhead

`tc netem` is unavailable on this host (no `CAP_NET_ADMIN`), so delay is injected
in userspace by `comparison/src/bin/delay-proxy.rs`. **One implementation serves
both transports**: `--mode tcp` relays TCP with `TCP_NODELAY`, `--mode udp`
relays each client 4-tuple over its own upstream UDP socket, preserving datagram
boundaries. Every read chunk/datagram is stamped `now + delay` and drained by a
single per-direction FIFO writer, so ordering is preserved and the read loop is
not serialised by the sleep. TLS is end-to-end; the proxy never terminates it.

Measured overhead (16 connections, depth 64, 1 KiB and 0-byte payloads, direct
vs. proxied, one run each; full data in
`results/matrix-proxy.csv`):

0-byte payload, direct vs. proxied (RTT p50):

| transport | delay | direct | proxied | delta |
| --- | --- | --- | --- | --- |
| tonic | 0 ms | 798 µs | 799 µs | +0.5 µs |
| tonic | 1 ms | 812 µs | 5,095 µs | +4.28 ms |
| tonic | 10 ms | 806 µs | 22,610 µs | +21.8 ms |
| QUIC | 0 ms | 1,266 µs | 1,274 µs | +8 µs |
| QUIC | 1 ms | 1,279 µs | 4,964 µs | +3.69 ms |
| QUIC | 10 ms | 1,285 µs | 22,790 µs | +21.5 ms |

At 1 KiB and high message rate the proxy costs QUIC about 10 % throughput at
delay 0 (113k → 101k msg/s) and tonic nothing measurable, which is why 0 ms
cells use direct loopback. The configured RTT increase is 2 ms and 20 ms; the
residual **~1.5–2.3 ms per RTT** is the timer overshoot described below.

The residual is dominated by tokio's ~1 ms timer granularity: a requested 1 ms
sleep typically fires about 1–1.5 ms late per direction. It is a **constant
additive RTT shift, identical for both transports**, and is disclosed rather
than silently subtracted. At high queue occupancy the FIFO writer coalesces
packets and the overshoot amortises, so the deep-pipeline cells pay
proportionally less. No jitter, loss or bandwidth limit is emulated; QUIC loss
recovery is therefore *not* exercised, only delayed-ACK behaviour.

## Reproduction

```bash
cd /path/to/quiccrunch
export CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-quiccrunch

# full matrix, 3 repeats, worker-scaling check and proxy calibration (~45-60 min)
nix develop -c bash scripts/bench-comparison.sh

# spot check: one transport, one cell
QUIC_ONLY=1 CONNS=16 DEPTHS=64 DELAYS=0 RUNS=1 \\
  nix develop -c bash scripts/bench-comparison.sh

# fast smoke
CONNS=1 DEPTHS=1 DELAYS="0 1" RUNS=1 WORKER_SCALING=0 CALIBRATE=0 \\
  DURATION=1 WARMUP=1 OUT=/tmp/smoke.csv \\
  nix develop -c bash scripts/bench-comparison.sh
```

Outputs:

* `comparison/results/matrix.csv` — one row per run, all percentiles and the
  load average;
* `comparison/results/matrix-summary.md` — medians and per-cell ratios;
* `comparison/results/matrix-proxy.csv` — proxy calibration.

Certificates are generated into `comparison/certs/` on first run (throwaway CA,
SAN `DNS:localhost,DNS:127.0.0.1,IP:127.0.0.1`). They are git-ignored.

## Hardware and host conditions

| Item | Value |
| --- | --- |
| CPU | Intel Core i5-8400T (6C/6T, 1.7 GHz base, 3.3 GHz max) |
| RAM | 16 GiB |
| Kernel | 6.18.36 |
| Governor | performance |
| Toolchain | rustc 1.89.0, release (`opt-level=3`, thin LTO, `target-cpu=native`) |
| Topology | client and server co-located on loopback |
| Host | **shared**. During the run: `Suricata-Main -T` at ~99 % of one core, Home Assistant, Kodi, Alloy and the agent runtime. 1-minute load average ≈ 5–11, recorded per cell. |

The host is shared and throughput varies by 2–3× under load. Every cell carries
its measured load average, and every result is a median of three alternating
runs. Treat ±20 % as noise; only differences that survive the medians are
discussed in the verdict.

## Caveats — read these before quoting a number

1. **Loopback, co-located.** No real NIC, no bandwidth limit, no competing
   traffic. Kernel loopback is significantly cheaper than a datacentre path.
2. **Lossless path.** The proxy delays but does not drop packets. QUIC's loss
   recovery, PTO and connection migration are not measured. The 10 ms cells
   exercise ACK timing and window growth, nothing more.
3. **TLS is not identical.** tQUIC uses BoringSSL TLS 1.3; tonic uses rustls.
   Cipher suites and handshake costs differ, but the handshake is outside the
   measurement window.
4. **Server threading differs by design.** QuicCrunch runs one pinned event
   loop per worker with `SO_REUSEPORT`; tonic runs a multi-threaded tokio
   runtime behind one accept loop. That difference *is* part of the comparison
   (hypothesis 3), but it means CPU affinity is not controlled identically.
5. **QUIC client fan-out is coarse.** Eight client runtimes share one UDP socket
   each, so 64 connections reach the server as eight source flows. The
   `SO_REUSEPORT` hash therefore distributes 8 flows, not 64 connections; this
   is a property of the adapter, and it is exactly what a distri-kv pod would
   see unless the client sharding changes.
6. **Deep pipelines measure queueing.** At depth 1800 the p50/p99 are dominated
   by in-flight queueing (`depth × streams / throughput`), not transport RTT.
   Compare transports at the same depth; do not read a deep p99 as a per-request
   latency.
7. **gRPC bidi plumbing.** tonic 0.10 has no client-side `Sink` for bidi
   responses, so the client feeds a bounded `mpsc` (2048) that hyper pulls from.
   The server maps the inbound stream directly with no channel hop. QUIC batches
   up to 64 messages per command. These are the idiomatic APIs of each stack,
   and the extra queue is counted inside RTT via the `written` timestamp.
8. **One payload size.** 1 KiB only; no large-message or small-RPC crossover.
9. **Control-plane work is absent.** No fsync, no Raft, no serialisation beyond
   the echo payload. distri-kv's control plane is storage-bound; this harness
   isolates transport cost.
10. **Certificates are throwaway** and the CA/key live in a git-ignored
    directory. Do not reuse them.