# raft-compare: tonic vs QuicCrunch AppendEntries

Same logical Raft contract, same payload bytes, same 3-node loopback topology,
same TLS, same client instrumentation. The only difference is the transport.

## Contract mapping

| Raft field | QuicCrunch | tonic harness |
|---|---|---|
| partition id | `partition_id` | `partition_id` |
| term / leader | `term`, `leader_id` | `term`, `leader_id` |
| log entry | `LogEntry { term, index, data }` | `RaftLogEntry { term, index, command }` |
| entry payload | opaque bytes | identical opaque bytes |
| response | `AppendEntriesResponse` | same fields |

The entry payload is `bincode` of `BenchCommand { seq, term, payload }`, padded
to exactly 512 B or 4096 B. Both transports carry the same byte vector; neither
service inspects it.

## Measurement protocol

- Three server processes on loopback, one listener each.
- One client process per transport run (tonic or QuicCrunch), 3 channels /
  pooled peers, matching the 3-node topology.
- Each cell is `(payload, depth)` with depth 1, 3, 5 concurrent append loops.
- Enqueue timestamp is taken before request construction, write timestamp
  immediately before the transport call, response timestamp after the call.
  RTT is write → response; end-to-end is enqueue → response.
- Completed-only throughput: failed calls increment `errors` and are excluded
  from the latency histograms.
- Server-side service time is recorded independently by the in-process service
  handler (`server_p99_us`), so transport/scheduling tail can be separated
  from service work.
- A 1 ms heartbeat task records scheduler wake lateness
  (`sched_late_p99_us`). A flat latency curve at depth 1 with high scheduler
  lateness is host scheduling; growth from depth 1 → 5 is queueing. Service
  time is reported separately.
- Load average is sampled at cell start and end (`load_start`, `load_end`).

## Shared-host protocol

`scripts/bench_raft_vs_grpc.sh` does not wait for a quiet window. It builds
first, then runs three repeats with alternating transport order. Every cell
records the 1-minute load average at start and end; the medians script prints
the observed load range. If the host was loaded, the results are labelled
provisional in the docs rather than presented as final.

## Reproduce

```bash
export CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-raft
export TMPDIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.tmp-qc-raft
nix develop -c ./scripts/bench_raft_vs_grpc.sh
nix develop -c python3 scripts/raft_bench_medians.py benchmarks/raft_append_vs_grpc.csv
```

Output: `benchmarks/raft_append_vs_grpc.csv` (one row per cell/repeat) plus a
markdown median/ratio table. Three repeats alternate transport order. The
host is shared; median load is recorded with every cell.

## Caveats

- The service is a transport echo. It does not fsync, apply a log or run
  consensus; real distri-kv replication adds disk and consensus time.
- The comparison is one process per transport, not a mixed cluster.
- Absolute rates are release-profile loopback numbers. Ratios are the useful
  output; the recorded load range must be quoted with them, and a loaded host
  makes the result provisional.