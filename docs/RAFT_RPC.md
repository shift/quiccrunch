# Raft RPC over QuicCrunch

Status: **transport prototype / adapter seam**. QuicCrunch provides the
node-to-node Raft RPC surface described here, but it is not the default
distri-kv transport. The measured recommendation in
[`GRPC_VS_QUICCRUNCH.md`](GRPC_VS_QUICCRUNCH.md) still applies: tonic is faster
for deep pipelines, and the control plane is fsync-bound. This work removes the
"there is no Raft-shaped QUIC contract" objection and makes the transport
swappable.

## 1. Contract

Schema: `quiccrunch-core/schema/raft.capnp`.

Every request and response is a length-prefixed Cap'n Proto envelope:

```
u32 big-endian body length | flat Cap'n Proto Request/Response
```

`Request.contractVersion` and `Response.contractVersion` are mandatory and
currently `1`. A server rejects an unknown version before interpreting the
payload. New fields use new ordinals; existing field numbers and types are
never reused.

### AppendEntries

```
AppendEntries(
  partitionId, term, leaderId,
  prevLogIndex, prevLogTerm,
  entries[{term,index,data}],
  leaderCommit
) -> {
  term, success, matchIndex,
  conflictIndex,   // first index the follower wants on failure
  conflictTerm     // term of the conflicting entry, 0 if unknown
}
```

The response is intentionally richer than the current distri-kv protobuf
(`term`, `success`): `matchIndex` and the conflict hints let the leader repair
`nextIndex` in one round trip instead of decrementing blindly.

### RequestVote

```
RequestVote(
  partitionId, term, candidateId,
  lastLogIndex, lastLogTerm
) -> { term, voteGranted }
```

### InstallSnapshot (streaming)

`InstallSnapshot` is a stream of frames on one bidirectional QUIC stream:

```
InstallSnapshotChunk(
  partitionId, term, leaderId,
  lastIndex, lastTerm,
  offset, chunk, done
) -> {
  term, success,
  bytesAccepted, nextOffset, done
}
```

The receiver acks **every** frame with the next contiguous offset it has
accepted. A session is identified by `(partitionId, lastIndex, lastTerm)`.
After a reconnect the sender continues at the last acknowledged `nextOffset`;
a replayed offset is accepted idempotently, so an ack lost with a broken stream
does not corrupt the snapshot. A frame whose offset is beyond the accepted
prefix is rejected with `snapshot-stale`.

### Leader hints

`Response.leaderHint { leaderId, leaderAddr }` mirrors the existing
`x-distri-kv-leader` behaviour. A follower that receives a write/forwarded
request returns:

```
code = notLeader, leaderHint = { leaderId, leaderAddr }
```

The caller can retry against `leaderAddr` without a second discovery round trip.
Node id `0` is valid; a hint is usable when `leaderAddr` is non-empty.

### Error codes

`ErrorCode` is wire-level and stable. `message` is diagnostic and must not be
parsed.

| code | value | retryable | meaning |
|---|---:|---|---|
| `ok` | 0 | – | success |
| `unavailable` | 1 | yes | connection/stream failure |
| `notLeader` | 2 | no | retry at `leaderHint` |
| `termMismatch` | 3 | no | caller term is stale; response `term` is current |
| `snapshotStale` | 4 | no | offset/snapshot identity mismatch |
| `frameTooLarge` | 5 | no | exceeded a documented bound |
| `invalidRequest` | 6 | no | malformed/unsupported version |
| `deadlineExceeded` | 7 | yes | RPC deadline |
| `backpressure` | 8 | yes | bounded per-peer queue full |
| `internal` | 9 | no | service failure |
| `protocol` | 10 | no | wire/union mismatch |

## 2. Limits

Bounds are enforced before writing and again on decode. Exceeding a bound is an
explicit `frame-too-large`/`invalid-request` error, never a truncated write.

| Limit | Value |
|---|---:|
| encoded frame incl. 4-byte prefix (`MAX_FRAME_BYTES`) | 8 MiB |
| entries per AppendEntries (`MAX_ENTRIES`) | 4096 |
| one log-entry payload (`MAX_ENTRY_BYTES`) | 4 MiB |
| one snapshot chunk (`MAX_SNAPSHOT_CHUNK_BYTES`) | 4 MiB |
| leader address (`MAX_ID_BYTES`) | 128 bytes |
| Cap'n Proto traversal | 8 MiB / 8 words |
| nesting limit | 32 |
| default RPC deadline | 5 s |
| default connect timeout | 3 s |
| default in-flight RPCs per peer | 64 |
| default queue wait before shedding | 25 ms |
| default reconnect attempts / backoff | 6 / 25 ms → 2 s + jitter |

A Raft entry is normally tens of KB to a few MB; 4 MiB per entry and 4 MiB per
snapshot chunk leave room for multi-entry batching while staying under the
8 MiB frame limit. Snapshot bodies are streamed, not carried in one message.

## 3. Transport behaviour

Implementation: `quiccrunch-core/src/raft/quic.rs`.

- **Connection reuse and pooling.** One QUIC connection is kept per peer
  address. All partitions share it. Each RPC opens a new bidirectional stream,
  so a slow partition cannot block another (QUIC stream independence). The
  test `concurrent_partitions_share_a_connection_without_hol_blocking` proves
  this with a 350 ms delay on partition 1 while partition 2 completes.
- **Reconnect.** A failed connection is dropped and the next call performs a
  bounded exponential backoff with full jitter (`25 ms * 2^n`, capped at 2 s,
  plus uniform jitter, 6 attempts by default). A half-open pooled connection is
  invalidated on send or read failure.
- **No silent request duplication.** A request is never retried after it may
  have been written. If `open_stream`, `send`, or the response read fails, the
  caller receives `unavailable` and decides. Snapshot chunks are the deliberate
  exception: they are idempotent by offset and are explicitly resumed.
- **Deadlines.** The client wraps the whole round trip in `rpc_timeout`; the
  server wraps request read and service dispatch in the same deadline. A timed
  out client drops its stream, so no half-consumed stream is reused. A timed
  out server sends `deadline-exceeded` and closes the stream.
- **Backpressure.** Each peer has a semaphore of `max_in_flight_per_peer`.
  A caller that cannot get a slot within `queue_wait` receives retryable
  `backpressure` instead of waiting in unbounded memory. `RaftMetricsSnapshot`
  exposes `queue_depth`, `max_queue_depth` and `shed_total`, plus RPC, timeout,
  reconnect, byte and not-leader counters. `QuicRaftTransport::metrics()`
  returns a snapshot; no Prometheus exporter is wired yet.
- **Large messages.** See §2. The codec checks list counts and per-field sizes
  on both sides. The transport does not split a frame; an oversized entry is
  rejected rather than truncated.

## 4. Adapter seam

`quiccrunch-core/src/raft/transport.rs` defines:

```rust
#[async_trait]
pub trait RaftTransport: Send + Sync {
    async fn append_entries(&self, peer: &RaftPeer, request: AppendEntriesRequest)
        -> RaftResult<AppendEntriesResponse>;
    async fn request_vote(&self, peer: &RaftPeer, request: RequestVoteRequest)
        -> RaftResult<RequestVoteResponse>;
    async fn install_snapshot_chunk(&self, peer: &RaftPeer, request: InstallSnapshotChunk)
        -> RaftResult<InstallSnapshotAck>;
    fn metrics(&self) -> RaftMetricsSnapshot;
    async fn install_snapshot(&self, peer: &RaftPeer, upload: SnapshotUpload)
        -> RaftResult<InstallSnapshotAck> { /* chunk + resume default */ }
}

#[async_trait]
pub trait RaftService: Send + Sync {
    async fn append_entries(&self, request: AppendEntriesRequest)
        -> RaftResult<AppendEntriesResponse>;
    async fn request_vote(&self, request: RequestVoteRequest)
        -> RaftResult<RequestVoteResponse>;
    async fn install_snapshot_chunk(&self, request: InstallSnapshotChunk)
        -> RaftResult<InstallSnapshotAck>;
    fn leader_hint(&self, partition_id: u64) -> Option<LeaderHint> { None }
}
```

Implementations:

- `QuicRaftTransport` — the QuicCrunch client (pool, backoff, deadlines,
  queueing, metrics).
- `InMemoryRaftTransport` — reference implementation used by unit tests and
  colocated partitions; same trait, no sockets.
- `serve_listener` / `serve_connection` — server dispatch. Each stream is one
  unary RPC or one streaming snapshot session.

The test harness (`quiccrunch-core/tests/raft_common/mod.rs`) implements
`RaftService` on top of a small Raft state machine, so the same trait is
exercised by both the in-process and QUIC implementations.

## 5. What distri-kv must change

No distri-kv code was modified for this task. To adopt the transport:

1. **Add a trait seam around the tonic client.** Define in distri-kv a
   `PartitionRaftTransport` with the three methods above (or depend on
   `quiccrunch-core` and use `RaftTransport` directly). The rest of
   `PartitionRaft`/`ezraft` keeps its current `AppendEntries`,
   `RequestVote` and `InstallSnapshot` messages.
2. **Map the existing messages.** `NodeId`/`Term`/`LogIndex` are already `u64`.
   Convert `AppendEntriesRequest`/`Response` and
   `RequestVoteRequest`/`Response` to the structs in
   `quiccrunch_core::raft`. Keep bincode only inside the log entry `data`
   (or replace it with another opaque payload encoding); the RPC envelope is
   Cap'n Proto.
3. **Replace one-shot snapshots.** distri-kv's current
   `InstallSnapshotRequest { data: Vec<u8> }` becomes
   `SnapshotUpload`, or call `install_snapshot_chunk` directly from the
   replication loop. Persist `nextOffset` so a leader can resume after a
   restart; the follower must retain partial snapshot state keyed by
   `(partition, lastIndex, lastTerm)`.
4. **Serve the RPCs.** Implement `RaftService` on the existing partition
   handler and run `serve_listener` alongside the tonic server during a
   migration. Return `RaftError::not_leader(LeaderHint::new(id, addr))` from
   the non-leader path; this replaces the metadata-only redirect.
5. **Add a config switch and feature flag.** Suggested:
   `raft.transport = "tonic" | "quiccrunch"` and Cargo feature
   `transport-quiccrunch`, defaulting to `tonic`. Keep the tonic path for one
   release; run both behind the trait in tests.
6. **Wire TLS identity.** The QUIC path verifies server certificates. distri-kv
   must supply the CA bundle and, for production, a client certificate/SPIFFE
   identity. tQUIC 1.6 still does not expose the peer certificate to the
   application (see `BACKEND_MESSAGING.md`), so identity binding remains the
   blocking production item.

## 6. Tests

Real sockets, TLS 1.3 on, server-certificate verification enabled (throwaway
CA + leaf with both IP and DNS SANs).

`cargo test -p quiccrunch-core --test raft_memory`:

- the same `RaftTransport`/`RaftService` seam over `InMemoryRaftTransport`
  (election, append, snapshot, metrics), proving the trait is transport-neutral.

`cargo test -p quiccrunch-core --test raft_wire`:

- field-preserving round trips for all three RPCs;
- version rejection;
- explicit error code + leader hint round trip;
- oversized entry/chunk/frame and too-many-entries rejection.

`cargo test -p quiccrunch-core --test raft_quic_e2e -- --test-threads=1`:

- 3-node leader election, append replication, follower restart and log catch-up;
- 200 KiB snapshot stream interrupted by an injected mid-stream failure,
  resumed from the acknowledged offset, byte-for-byte comparison;
- follower redirect: `notLeader` + hint, retry at the hinted leader;
- server-side and client-side deadline enforcement;
- slow-peer shedding with `backpressure`, queue-depth and shed metrics;
- concurrent partitions over one shared connection with no cross-partition
  head-of-line blocking;
- 1 MiB append entry over TLS plus an oversized chunk rejected without
  truncation.

The follower-restart test starts the restarted node on a fresh loopback port to
avoid an `SO_REUSEPORT` race with the old runtime socket; same-address
reconnect is exercised by the snapshot interruption test and by invalidating a
dead pooled connection during replication.

## 7. Benchmark

`quiccrunch-core/examples/raft_append_bench.rs`, driven by
`scripts/bench_raft.sh`. Three servers on loopback, TLS on, 512-byte entry,
sequential AppendEntries round trips. The service is a transport echo (no
durable log or consensus work). Host was shared; every run records the
1-minute load average. Three runs, medians below.

| run | load1 | ops | rate/s | p50 µs | p95 µs | p99 µs | errors |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 19.47 | 6218 | 1244 | 287 | 2135 | 10431 | 0 |
| 2 | 19.36 | 3138 | 628 | 1249 | 3005 | 11599 | 0 |
| 3 | 19.17 | 4991 | 998 | 286 | 2915 | 11215 | 0 |
| **median** | **19.36** | — | **998** | **287** | **2915** | **11215** | **0** |

These are development-profile numbers (`opt-level = 1`) on a host at load ~19,
not a capacity claim. The spread between runs (628–1244 ops/s) is host noise;
the p50 is bimodal because a scheduled-out client thread shows up as a whole
scheduler slice. Absolute numbers matter less than repeatability and the
absence of errors.

**QuicCrunch/tonic ratio: not measured for Raft.** There is no comparable
tonic AppendEntries harness in either repository; the existing tonic comparison
in `comparison/` is a 1 KiB echo matrix, not Raft. Do not read a transport ratio
into the numbers above. The honest summary remains the one in
[`GRPC_VS_QUICCRUNCH.md`](GRPC_VS_QUICCRUNCH.md): on the echo matrix tonic wins
most depth ≥64 cells, and distri-kv's control plane is fsync-bound, so this work
does not establish a performance reason to switch. A tonic Raft comparison
would need the same 3-node topology, the same service work, and the same load
recording; that harness does not exist yet.

## 8. Remaining gaps

- **Reconnect semantics under partition.** In-flight AppendEntries are not
  retried (correctly), but a leader that loses the response cannot distinguish
  "not applied" from "applied, ack lost". Raft reconciles this with
  `prevLogIndex`/`prevLogTerm` on the next round; the transport documents the
  boundary but does not add request IDs. `notLeader`/`termMismatch` hints cover
  the common cases.
- **Snapshot GC.** The receiver retains partial snapshots by identity. There is
  no expiry, size cap or background GC yet; a leader that abandons a snapshot
  leaves partial state until the process restarts. Production needs a TTL and
  a bound on retained partials per partition.
- **Term-tracking edge cases.** A cached leader hint can be stale after an
  election. Callers must treat `notLeader` as advisory and re-check `term`.
  PreVote and membership-change RPCs are not part of this contract; adding
  them is a schema version bump. Vote persistence (term + votedFor across
  crash) remains the application's responsibility.
- **Server-side global backpressure.** The client bounds per-peer in-flight
  work. The server spawns one task per accepted stream and relies on the QUIC
  stream limit and the service's own limits; there is no global server shed
  counter yet.
- **Metrics export.** Counters exist but are not exported to Prometheus/OTel.
- **Identity.** Server-certificate verification is on; mTLS peer-certificate
  access is still blocked by tQUIC 1.6 (same gap as the WAF messaging
  prototype).
- **Comparison harness.** A tonic `PartitionRaft` benchmark with identical
  service work and a recorded-load median protocol is still missing.