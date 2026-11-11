# Documentation index

QuicCrunch is an experimental prototype. This index says what each document
covers and how much to trust it today, because several documents predate the
current transport adapter and describe intentions rather than shipped code.

Two rules for readers:

- **Performance claims** live only in
  [`PERFORMANCE_RELEASE.md`](PERFORMANCE_RELEASE.md) and
  [`GRPC_VS_QUICCRUNCH.md`](GRPC_VS_QUICCRUNCH.md). Any number elsewhere in the
  repository is either reproduced from those files or stale.
- **Known gaps** are tracked in [`known-issues/`](known-issues/) and
  [`../SECURITY.md`](../SECURITY.md). If a feature is described as complete in an
  older document but missing from the code, the known-issues folder wins.

## Current

| Document | What it tells you | Currency |
| --- | --- | --- |
| [`PERFORMANCE_RELEASE.md`](PERFORMANCE_RELEASE.md) | Measured throughput and RTT for the QUIC transport: hardware, methodology, all 36 matrix cells, error analysis, bottlenecks, and how to reproduce with `scripts/bench.sh`. | **Current.** Single source of truth for QUIC numbers; every figure comes from the checked-in benchmark. |
| [`GRPC_VS_QUICCRUNCH.md`](GRPC_VS_QUICCRUNCH.md) | The tonic/gRPC head-to-head: same RPC, same instrumentation, 36 cells at three delays. Concludes tonic wins bulk throughput and recommends staying on tonic. | **Current.** Generated from `comparison/results/matrix.csv`; methodology in `comparison/README.md`. |
| [`SECURITY.md`](SECURITY.md) | TLS defaults, certificate verification, explicit opt-outs, the missing mTLS/SPIFFE identity, the unwired `AsyncTquicClient` path, and the tests that pin the defaults. | **Current** for `main`. mTLS arrives in PR #6; update this file in that PR. |
| [`RAFT_RPC.md`](RAFT_RPC.md) | The versioned Raft RPC contract over Cap'n Proto: `AppendEntries`, `RequestVote`, `InstallSnapshot`, envelope framing and error mapping. | **Current** (merged in PR #4), but the RPC surface has not been benchmarked against tonic. |
| [`known-issues/audit-policy.md`](known-issues/audit-policy.md) | What `cargo audit` is allowed to fail on, and how deliberate ignores are recorded. | **Current.** Matches the `security` CI job. |
| [`known-issues/doctests-high-level-api.md`](known-issues/doctests-high-level-api.md) | Why `quiccrunch-core` doctests are disabled and why `quiccrunch-ffi` is outside the workspace. | **Current.** Remove only when the high-level client/server API is restored. |
| [`known-issues/auth-feature-does-not-build.md`](known-issues/auth-feature-does-not-build.md) | The non-default `auth` feature fails to compile; the error list and what a fix needs. | **Current.** Authentication is not available in any working build. |
| [`../comparison/README.md`](../comparison/README.md) | The comparison harness: what is compared, the fairness contract, how to rerun the matrix, and the host-load caveats. | **Current.** Owns the reproduction steps behind `GRPC_VS_QUICCRUNCH.md`. |

## Historical or aspirational

| Document | What it tells you | Currency |
| --- | --- | --- |
| [`TQUIC_INTEGRATION_ANALYSIS.md`](TQUIC_INTEGRATION_ANALYSIS.md) | A 2025-10-21 analysis of the worker-pool path that returned empty data and could not transmit. Useful as a record of the integration gap. | **Superseded.** The measured adapter (`transport::adapter`) now moves data and is benchmarked; the `AsyncTquicClient` worker path it describes is still not the measured path and still lacks TLS wiring (see `SECURITY.md`). |
| [`QUICCRUNCH_ARCHITECTURE.md`](QUICCRUNCH_ARCHITECTURE.md) | The intended layered architecture: transport, RPC service layer, hardware crypto, observability. | **Partly aspirational.** Written before the current adapter; the crypto-acceleration subsystem it describes is not exported from the crate. Treat the diagram as a design sketch, not a map of `main`. |
| [`HARDWARE_CRYPTO_API.md`](HARDWARE_CRYPTO_API.md) | API reference for the in-tree `crypto_accel` module (AES-NI/AVX/SHA-NI detection, batch crypto, fallbacks). | **Stale.** The module is commented out of `src/lib.rs`, so this API is not reachable from the crate. The transport relies on BoringSSL (via tQUIC) for hardware dispatch. |
| [`HARDWARE_CRYPTO_PERFORMANCE.md`](HARDWARE_CRYPTO_PERFORMANCE.md) | Criterion benchmark tables for that module on a 2019 Core i7-9750H with Rust 1.70. | **Stale and unverified.** Not reproducible from the current tree; do not quote these numbers. |
| [`research/2025-08-30-kernel-bypass-general.md`](research/2025-08-30-kernel-bypass-general.md) | A 2025-08-30 survey of DPDK, AF_XDP and eBPF for QUIC I/O, with integration trade-offs. | **Background research.** No kernel-bypass work is planned or in the tree; it is explicitly out of scope for the prototype. |

## Related files outside `docs/`

| File | Purpose |
| --- | --- |
| [`../README.md`](../README.md) | Project status, the one canonical performance cell, build and run instructions. |
| [`../SECURITY.md`](../SECURITY.md) | Disclosure process, supported versions, and the security caveats (no CRL/OCSP, no CA rotation without restart). |
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Nix and Cargo builds, the exact local gate, commit style, PR expectations. |
| [`../CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) | Contributor Covenant 2.1 and the enforcement contact. |
| [`../.github/PULL_REQUEST_TEMPLATE.md`](../.github/PULL_REQUEST_TEMPLATE.md) | The checklist every PR is expected to fill in. |
