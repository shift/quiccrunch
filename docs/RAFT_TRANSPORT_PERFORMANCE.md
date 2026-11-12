# Raft transport performance: tonic vs QuicCrunch

Status: **provisional**. The benchmark host is shared and was not quiet during
this run. Every cell records the 1-minute load average at start and end; the
observed range across all 36 cells was **4.77–8.71**. Treat the absolute rates
as a lower bound and the ratios as indicative, not as a capacity claim.

## What is compared

The same logical `AppendEntries` contract is carried over two transports:

- **tonic** — gRPC over HTTP/2 with TLS (`raft-compare/proto/raft.proto`).
- **QuicCrunch** — the versioned Raft RPC surface over QUIC with TLS
  (`quiccrunch-core/src/raft`).

Both use the same 3-node loopback topology, the same bincode command bytes
(512 B and 4 KiB), the same 1/3/5 concurrent append loops, and the same client
instrumentation. The service is a transport echo: it does not fsync, apply a
log or run consensus. The numbers therefore bound serialisation + transport +
scheduling, not a real distri-kv append.

## Method

- Release profile (`opt-level = 3`, thin LTO), 3-second cells.
- Three repeats with alternating transport order (tonic→QUIC, QUIC→tonic,
  tonic→QUIC) so load drift does not favour one side.
- Completed requests only; errors are counted and excluded from percentiles.
- RTT is write → response; end-to-end is enqueue → response. Server handler
  time and 1 ms scheduler lateness are recorded separately.
- Medians of the three repeats are reported below. Raw rows:
  `benchmarks/raft_append_vs_grpc.csv`.

## Results (medians of 3, provisional)

| payload | depth | tonic ops/s | QUIC ops/s | QUIC/tonic | tonic p50 µs | QUIC p50 µs | tonic p99 µs | QUIC p99 µs | winner |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---|
| 512 B | 1 | 8,095 | 7,357 | 0.91 | 118 | 131 | 179 | 210 | tonic |
| 512 B | 3 | 24,339 | 9,464 | 0.39 | 113 | 303 | 297 | 553 | tonic |
| 512 B | 5 | 33,300 | 12,138 | 0.36 | 141 | 384 | 263 | 769 | tonic |
| 4 KiB | 1 | 7,804 | 4,836 | 0.62 | 122 | 195 | 186 | 280 | tonic |
| 4 KiB | 3 | 23,882 | 6,863 | 0.29 | 118 | 419 | 275 | 713 | tonic |
| 4 KiB | 5 | 29,724 | 8,396 | 0.28 | 154 | 571 | 378 | 1,040 | tonic |

Errors: **0 in every cell**. Max sustained median rate: tonic **33,300 ops/s**
(512 B, depth 5) and **29,724 ops/s** (4 KiB, depth 5); QuicCrunch
**12,138 ops/s** and **8,396 ops/s**.

**Which transport wins which cell:** tonic wins all six cells. The gap is
smallest at depth 1 (QuicCrunch reaches 0.91× at 512 B and 0.62× at 4 KiB) and
grows to roughly 2.5–3.6× at depths 3 and 5, where the QUIC path's per-stream
queueing and serialisation dominate. QuicCrunch's p50 and p99 are higher in
every cell.

## Interpretation

- `server_p99_us` is 1–2 µs for both transports, so the service handler is not
  the bottleneck.
- Scheduler lateness is ~1.0 ms for tonic and ~1.7–2.0 ms for QuicCrunch. The
  RTT p99 is below that at depth 1, so the depth-1 tail is host scheduling.
- The growth from depth 1 to depth 5 at flat service time is queueing in the
  transport/client loops. QuicCrunch's p99 grows from 210 to 769 µs (512 B) and
  280 to 1,040 µs (4 KiB); tonic's from 179 to 263 µs and 186 to 378 µs.
- The host load (4.77–8.71 on 6 cores) is high enough that these numbers are
  **provisional**. A quiet-host rerun is required before quoting them as final.

## Decision

QuicCrunch loses the append benchmark on this host. The transport swap is not
justified by performance: the remaining work (identity, GC, metrics,
operational hardening) would buy a slower append path. Keep tonic as the
distri-kv Raft transport; retain the QuicCrunch Raft seam as an optional
backend or a future QUIC experiment, not as a default.

## Reproduce

```bash
export CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-raft
export TMPDIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.tmp-qc-raft
nix develop -c ./scripts/bench_raft_vs_grpc.sh
nix develop -c python3 scripts/raft_bench_medians.py benchmarks/raft_append_vs_grpc.csv
```

The script does not wait for a quiet window. It records load per cell and
alternates transport order across three repeats; the medians script prints the
observed load range.
