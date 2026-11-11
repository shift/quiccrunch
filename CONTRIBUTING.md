# Contributing to QuicCrunch

QuicCrunch is an experimental prototype. Contributions that make the transport,
the benchmark, or the documentation more honest and reproducible are welcome;
contributions that add features to an unproven transport are a harder sell.
Open an issue before starting anything larger than a bug fix so the design can
be discussed first.

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md). Report
security issues privately as described in [SECURITY.md](SECURITY.md), not in a
public issue.

## Development environment

Nix is the reference environment. `flake.nix` is the single source of truth for
the toolchain and native dependencies.

```bash
nix develop
```

Inside the shell, `rustc`, `cargo`, `clippy`, `rustfmt`, `capnproto`, OpenSSL,
`cmake` and `clang` are already on `PATH`.

Without Nix you need stable Rust plus `cmake`, `clang`, `pkg-config` and OpenSSL
headers (`libssl-dev` on Debian/Ubuntu). CI builds with the latest stable
toolchain; no MSRV is declared.

### Build

```bash
nix develop -c cargo build --workspace
```

`quiccrunch-ffi` is deliberately excluded from the workspace: it targets a
high-level client/server API that is currently commented out. Do not add it back
to `members` as a drive-by change. `quiccrunch-security` is not a workspace
member either.

### `CARGO_TARGET_DIR`

Build artifacts are large, so keep them out of the source tree when the checkout
lives on a slow or shared disk:

```bash
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/quiccrunch-target}"
```

`.cargo/config.toml` sets `target-dir = "target"`; the `CARGO_TARGET_DIR`
environment variable overrides it. `scripts/bench.sh` honours the same variable
(`TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"`) and looks for the release
binaries there. If you switch target directories, rebuild before benchmarking;
the script only rebuilds when the binaries are missing.

Do not add `-C incremental=on` to `RUSTFLAGS`. rustc treats the value as a
directory path, not a boolean, and writes a large `./on` tree into the checkout.
Cargo's `[build] incremental = true` already enables incremental compilation.

## The local gate

CI must be green before a PR is merged, and these are the same checks it runs.
Run all of them locally before pushing:

```bash
nix develop -c cargo fmt --all -- --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo build --workspace --all-targets
nix develop -c cargo test --workspace --no-fail-fast
nix develop -c cargo test --doc --workspace --exclude quiccrunch-core
nix develop -c cargo doc --workspace --no-deps
nix shell nixpkgs#cargo-audit -c cargo audit
```

Notes:

- `cargo clippy` uses `-D warnings`: warnings are errors in CI.
- `quiccrunch-core`'s doctests are disabled because the rustdoc examples
  document an API that is commented out. See
  [`docs/known-issues/doctests-high-level-api.md`](docs/known-issues/doctests-high-level-api.md).
  Do not "fix" that by deleting the doc examples; restore the API instead.
- `cargo audit` follows the policy in
  [`docs/known-issues/audit-policy.md`](docs/known-issues/audit-policy.md).
  Real vulnerabilities with a patched release fail the gate; unmaintained and
  informational advisories are printed. Do not add a dependency to silence an
  advisory.
- The `auth` feature is not part of the default build, and it does not compile
  on `main` (see
  [`docs/known-issues/auth-feature-does-not-build.md`](docs/known-issues/auth-feature-does-not-build.md)).
  Restoring it is welcome; do not delete the interceptor to make the build pass.

## Benchmarks

Benchmarks are noisy on shared machines and are never run on pull requests. Do
not add benchmark runs to CI. To measure locally:

```bash
nix develop -c bash scripts/bench.sh
```

Use an otherwise idle host, keep the CPU governor on `performance` if you can,
and leave `RUST_LOG` at the script's default (`warn`). Benchmarking with
`RUST_LOG=debug` is invalid: per-packet logging dominates the measurement.
Every number in [`docs/PERFORMANCE_RELEASE.md`](docs/PERFORMANCE_RELEASE.md)
must be reproducible with that script; if it is not, it does not belong in the
document.

## Commit style

Use [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):

```
type(scope): imperative description
```

Examples from this repository:

```
perf(transport): push reads, wait-free sends, honest benchmarks
bench(compare): tonic vs QuicCrunch same-shape matrix
ci: make the pipeline green and the repo publishable
docs(security): record the TLS defaults and the mTLS gap
```

- Keep commits atomic: one logical change per commit, and each commit should
  build.
- Branch names use `type/short-description`, for example
  `docs/public-readiness` or `feat/mtls-identity`.
- Do not rewrite published history. Fix forward with a new commit.

## Pull requests

- Keep the PR focused. Unrelated refactors and formatting churn make review
  harder and are usually asked to be split out.
- Describe what changed, why, and how it was verified. Link the issue it
  closes.
- Include a test for behaviour changes. For transport changes, say which
  benchmark cell you ran and paste the summary line.
- Update the docs in the same PR when you change behaviour, the wire format, or
  the performance story. New documents go in `docs/` and must be linked from
  [`docs/README.md`](docs/README.md).
- Dependency changes need a justification in the PR body. Run
  `nix develop -c cargo update` after editing `Cargo.toml` and include the
  resulting `Cargo.lock` change.
- Do not merge with failing or pending required checks. If a check is flaky,
  say so in the PR instead of retrying until it passes.
- Draft PRs are fine for work in progress; mark them ready when the local gate
  passes.

## License

Unless you state otherwise, your contribution is dual-licensed under the
Apache-2.0 and MIT licenses, matching [LICENSE-APACHE](LICENSE-APACHE) and
[LICENSE-MIT](LICENSE-MIT). No contributor license agreement is required.
