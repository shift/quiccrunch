# Pull request

## Summary

<!-- What changed, and why. One or two paragraphs. -->

## Type of change

- [ ] Bug fix
- [ ] Transport or RPC behaviour change
- [ ] Benchmark or comparison methodology change
- [ ] Documentation only
- [ ] Build, CI or Nix change
- [ ] Refactor with no behaviour change

## Related issue

<!-- Fixes #NNN, or "none". Explain if there is no issue. -->

## Verification

<!-- Paste the commands you ran and their result. The local gate is in CONTRIBUTING.md. -->

```bash
nix develop -c cargo fmt --all -- --check
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo build --workspace --all-targets
nix develop -c cargo test --workspace --no-fail-fast
nix develop -c cargo test --doc --workspace --exclude quiccrunch-core
nix shell nixpkgs#cargo-audit -c cargo audit
```

## Performance impact

<!--
Required for anything on the data path. State the cell and paste the summary
line, e.g. 4 workers / 16 connections / depth 1. If you did not benchmark, say
so and why. Do not add benchmark runs to CI.
-->

- [ ] No effect on the measured path
- [ ] Measured (details below)
- [ ] Not measured (explain why)

## Documentation

- [ ] Behaviour, wire format, or performance claims are reflected in `docs/`
- [ ] New documents are linked from `docs/README.md`
- [ ] No new unverified numbers (performance claims must come from `scripts/bench.sh`)

## Checklist

- [ ] The change is focused; unrelated refactors are in a separate PR
- [ ] No dependency was added to silence an advisory
- [ ] No secrets, tokens, certificates or private hostnames are included
- [ ] CI is green (do not merge with failing or pending required checks)
- [ ] I did not rewrite published history
