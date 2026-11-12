# QuicCrunch fuzz targets

These targets cover the hand-written wire decoders. They are **not** part of
the stable CI test job: `cargo-fuzz` needs nightly, while the repository pins
Rust 1.89.0. The same decoders are exercised on every PR by the stable
`proptest` suite in `quiccrunch-core/tests/prop_wire.rs`.

## Targets

| Target | Surface |
| --- | --- |
| `frame_codec` | `FrameHeader` length/trace-context codec and the messaging `u32` length-prefixed envelope |
| `messaging_decode` | Cap'n Proto messaging envelope and every request/response/watch/error payload type |
| `raft_rpc_decode` | Raft `AppendEntries`, `RequestVote` and `InstallSnapshot` request/response decoders |

Each target asserts the same invariant: the decoder must not panic, must not
read out of bounds, must not allocate without a bound, and must either return a
clean `Err` or a valid message. Accepted messages are checked against the
documented limits (4 MiB messaging envelope, 8 MiB Raft frame, 4 MiB entry or
snapshot chunk, 4096 entries, 1024 events, 64 metadata entries).

## Running

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
rustup run nightly scripts/fuzz.sh   # 60 s per target
FUZZ_SECONDS=120 rustup run nightly scripts/fuzz.sh
```

`scripts/fuzz.sh` invokes `cargo fuzz`, so the active `cargo` must be the
nightly toolchain (CI installs nightly as the default).

`scripts/fuzz.sh` sets `CARGO_TARGET_DIR` to `target/fuzz` unless it is already
set. CI runs the same script from the manual `Fuzz (nightly)` job.

## Adding a target

1. Add a `[[bin]]` entry to `fuzz/Cargo.toml`.
2. Add `fuzz/fuzz_targets/<name>.rs` with a `fuzz_target!` body.
3. Add the target name to the default list in `scripts/fuzz.sh`.
4. Add a stable `proptest` case for the same decoder in
   `quiccrunch-core/tests/prop_wire.rs` so PRs keep coverage.
