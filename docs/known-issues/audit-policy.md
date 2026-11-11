# `cargo audit` policy

CI runs `cargo audit` from `taiki-e/install-action` against `Cargo.lock`.

Policy:

- **Fail** on an advisory with a real vulnerability that has a patched release
  available (i.e. a normal `cargo audit` failure).
- **Do not fail** on `unmaintained`, `unsound` (without an exploitable path in
  this project), or `informational` advisories. `cargo audit` exits zero for
  these by default; CI does not pass `--deny warnings`, so they are printed
  for review without breaking the build.
- When an advisory is ignored deliberately, add a dated `[advisories] ignore`
  entry to `.cargo/audit.toml` with the reason and the tracking issue/PR.

The dependency set was recently cleaned; do not add a dependency to silence an
advisory.
