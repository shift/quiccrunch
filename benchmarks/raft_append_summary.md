# Raft AppendEntries benchmark — QuicCrunch

Command: `./scripts/bench_raft.sh` (3 runs, 5 s each, 512-byte entry, 3 nodes
on loopback, TLS on, development profile `opt-level=1`). Raw data:
`raft_append_quic.csv`.

| run | load1 | ops | rate/s | p50 µs | p95 µs | p99 µs | errors |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 19.47 | 6218 | 1244 | 287 | 2135 | 10431 | 0 |
| 2 | 19.36 | 3138 | 628 | 1249 | 3005 | 11599 | 0 |
| 3 | 19.17 | 4991 | 998 | 286 | 2915 | 11215 | 0 |
| **median** | **19.36** | — | **998** | **287** | **2915** | **11215** | **0** |

The host was shared (load ~19). The service is a transport echo: no durable
log, fsync or consensus work. Do not treat these as capacity numbers. No
comparable tonic Raft harness exists in either repository, so no
QuicCrunch/tonic ratio is reported for AppendEntries; see
`docs/RAFT_RPC.md` §7 and `docs/GRPC_VS_QUICCRUNCH.md`.