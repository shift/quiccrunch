# Fuzzing coverage

Status: **wire decoders covered; higher layers still unfuzzed.**

The nightly `cargo-fuzz` targets in `fuzz/` and the stable `proptest` suite in
`quiccrunch-core/tests/prop_wire.rs` cover:

- the hand-written frame header and messaging length-prefix codec;
- the Cap'n Proto messaging envelope and every request/response/watch/error
  payload type;
- the Raft `AppendEntries`, `RequestVote` and `InstallSnapshot` request and
  response decoders.

Each target enforces the documented bounds (4 MiB messaging envelope, 8 MiB
Raft frame, 4 MiB entry/snapshot chunk, 4096 entries, 1024 events, 64 metadata
entries) and asserts that a decoder either returns a clean error or a valid
message that re-encodes.

Still genuinely unfuzzed:

- TLS handshake and certificate/SPIFFE identity parsing (BoringSSL/tQUIC);
- the JWT/RBAC interceptor path (the `auth` feature does not build yet; see
  `auth-feature-does-not-build.md`);
- watch/stream state machines, retention eviction and reconnect logic;
- the QUIC transport itself (packet parsing is tQUIC's responsibility);
- snapshot session state (`(partitionId, lastIndex, lastTerm)` resume and
  partial-snapshot retention).

These are tracked here rather than claimed as covered. Add a target and a
matching stable proptest case before moving an item out of this list.
