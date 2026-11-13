# Versioning and downstream pinning

QuicCrunch is an experimental, pre-1.0 project. This document records the
current release, the exact pin a consumer should use, and what `0.1.x` is
allowed to change. It does not promise SemVer stability.

## Current release

| Item | Value |
| --- | --- |
| Workspace version | `0.1.0-alpha.1` |
| Git tag | `v0.1.0-alpha.1` |
| Commit | `be6258187114a205f79b155228ab537811537300` |
| Registry | **Not published to crates.io** |
| GitHub release | Pre-release, Linux x86_64 CLI asset only |

The crates (`quiccrunch-core`, `quiccrunch-macros`, `quiccrunch-cli`,
`quiccrunch-security`) are not on crates.io. Pin the repository, not the
registry.

## Recommended pin

Pin the exact commit, not a branch and not a floating tag. Tags are easier to
read; the `rev` is what makes the build reproducible if a tag is ever moved.

```toml
# Reproducible: exact commit behind v0.1.0-alpha.1.
quiccrunch-core = { git = "https://github.com/shift/quiccrunch", rev = "be6258187114a205f79b155228ab537811537300" }
```

If you prefer the tag, keep the commit in a comment so an auditor can verify
that the tag still points where it did:

```toml
quiccrunch-core = { git = "https://github.com/shift/quiccrunch", tag = "v0.1.0-alpha.1" }
# v0.1.0-alpha.1 -> be6258187114a205f79b155228ab537811537300
```

Resolve the commit yourself with:

```bash
git ls-remote https://github.com/shift/quiccrunch.git refs/tags/v0.1.0-alpha.1
# annotated tag: peel it to the commit
git rev-parse v0.1.0-alpha.1^{commit}
```

For a workspace that also uses `quiccrunch-macros` or `quiccrunch-cli`, pin
every QuicCrunch dependency to the same `rev`; the workspace crates are
versioned together.

## Compatibility policy for `0.1.x`

`0.1.x` is alpha. The compatibility promise is deliberately narrow.

### What may change in a `0.1.x` release

- **Rust API.** Public types, function signatures, feature names and defaults
  may change, including in patch releases, when correctness or security
  requires it. Read the release notes before upgrading.
- **Configuration and defaults.** TOML fields, TLS defaults and tuning knobs
  may change. A new default may require a config change.
- **Performance.** Throughput and latency are measurements, not contracts.
- **Binaries and platforms.** The release matrix may add or drop targets. This
  release ships Linux x86_64 only; Windows and macOS are unverified.
- **Dependencies.** Upgrades may raise the effective toolchain or native
  dependency floor. This release itself does not bump dependencies.

### What is intended to stay stable within `0.1.x`

- **Messaging wire contract v1.** `Envelope.contractVersion` is the major
  version and stays `1` for additive changes. Optional fields use new ordinals;
  existing ordinals and types are not reused. A new required field, removed
  field, type change or semantic change bumps `contractVersion`. A v1 peer
  rejects an unknown major version before decoding the payload.
- **Schema-evolution rules.** Unknown fields are ignored by Cap'n Proto
  readers; the server rejects unknown contract versions with an error naming
  both versions. The tests in
  `quiccrunch-core/tests/schema_evolution.rs` are the executable version of
  this policy.
- **Raft RPC version gate.** The Raft schema follows the same rules; its gate
  is in `quiccrunch-core/src/raft/codec.rs`.

There is no promise that a `0.1.x` client and server can be mixed across minor
versions, and no migration tooling. Both ends should be upgraded together
unless the release notes say otherwise.

### What is explicitly not promised

- Production support, a security-response SLA, or a release cadence.
- API stability for the partly disabled high-level client/server API.
- Wire stability beyond the `contractVersion` gate above.
- Behaviour of topic branches, `main` between tags, or unmerged pull requests.

## Release mechanics

A release is an annotated tag `v<workspace-version>` pushed to `main`.
`.github/workflows/release.yml` verifies that every workspace crate shares the
tagged version, builds the Linux binary, publishes a GitHub **pre-release**
with `RELEASE_NOTES.md` as the body, and does not publish to crates.io. The
workflow intentionally ships no Windows or macOS artefacts: those builds were
previously allowed to fail with `continue-on-error`, which produced a green
release with silently missing binaries. They return when the targets are
actually verified.

When upgrading a consumer, bump the `rev`, run its own tests, and check
[`RELEASE_NOTES.md`](../RELEASE_NOTES.md) for changes and known limits.
