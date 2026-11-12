# Raft AppendEntries benchmark — QuicCrunch vs tonic

The earlier development-profile QuicCrunch-only numbers are withdrawn: they
were not release measurements, the host was at load ~19, and there was no tonic
comparison.

The corrected release-profile comparison is the `raft-compare` harness. Three
alternating repeats, 3-second cells, 512 B and 4 KiB bincode entries, 3-node
loopback with TLS, 1/3/5 in-flight appends. The shared host was not quiet:
per-cell 1-minute load was **4.77–8.71**, so the numbers are **provisional**.

| payload | depth | tonic ops/s | QUIC ops/s | QUIC/tonic | tonic p50 µs | QUIC p50 µs | tonic p99 µs | QUIC p99 µs | winner |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---|
| 512 B | 1 | 8,095 | 7,357 | 0.91 | 118 | 131 | 179 | 210 | tonic |
| 512 B | 3 | 24,339 | 9,464 | 0.39 | 113 | 303 | 297 | 553 | tonic |
| 512 B | 5 | 33,300 | 12,138 | 0.36 | 141 | 384 | 263 | 769 | tonic |
| 4 KiB | 1 | 7,804 | 4,836 | 0.62 | 122 | 195 | 186 | 280 | tonic |
| 4 KiB | 3 | 23,882 | 6,863 | 0.29 | 118 | 419 | 275 | 713 | tonic |
| 4 KiB | 5 | 29,724 | 8,396 | 0.28 | 154 | 571 | 378 | 1,040 | tonic |

Errors: 0 in every cell. Tonic wins all six cells. The service is a transport
echo (no durable log, fsync or consensus work), so these numbers bound
serialisation + transport + scheduling, not a real distri-kv append.

Raw data: `raft_append_vs_grpc.csv`. Method, per-cell loads and caveats:
[`../docs/RAFT_TRANSPORT_PERFORMANCE.md`](../docs/RAFT_TRANSPORT_PERFORMANCE.md).
