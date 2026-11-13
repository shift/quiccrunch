# Security policy

QuicCrunch is an experimental transport prototype, not production software. It
has no supported release series and no security-response SLA. Even so, reports
about the TLS path, authentication, or memory safety of untrusted input are
welcome and will be handled as described below.

The technical detail lives in [`docs/SECURITY.md`](docs/SECURITY.md): what the
adapter verifies, which call sites opt out, and which paths are known to be
incomplete. This file is the policy and the disclosure process.

## Reporting a vulnerability

Email **shift+security+waf-r@someone.section.me**. If that alias bounces, write
to **shift@someone.section.me** with `[security]` in the subject. Please do not
open a public issue for a suspected vulnerability.

Include, as far as you can:

- the commit or branch you tested;
- the affected component (transport adapter, JWT/RBAC interceptor, rate limiter,
  Raft RPC, benchmark harness);
- a reproduction (command, config, or a minimal patch);
- the impact you believe it has.

There is no PGP key for this project; if you need encrypted transport, say so in
a first plain-text message and a key will be provided.

Expect an acknowledgement within 7 days. Because this is an unfunded prototype,
the fix timeline is best-effort: the aim is a private branch and a coordinated
disclosure within 90 days, or an agreed public write-up if the issue cannot be
fixed in that window. Credit is given in the advisory unless you ask otherwise.
No bug bounty is offered.

## Supported versions

| Version | Supported | Notes |
| --- | --- | --- |
| `v0.1.0-alpha.1` (pre-release) | Best-effort fixes only | First tagged alpha; API and wire formats may change. See [`RELEASE_NOTES.md`](RELEASE_NOTES.md) and [`docs/VERSIONING.md`](docs/VERSIONING.md) |
| `main` (0.1.x prototype) | Best-effort fixes only | Development head; API and wire formats may change without notice |
| Other topic branches | No | Development branches; report against `main` where possible |
| Any published crate or binary | None exist | The crates are not published to crates.io |

## Security model on `main`

- The client verifies the server certificate by default
  (`RuntimeOptions::verify_server_cert = true`, `WorkerConfig::verify_cert =
  true`). The effective policy is the logical AND of the two switches.
- Verification is disabled only for loopback tests with self-signed
  certificates, and each such call site says so inline. There is no
  `allow_insecure_tls` flag.
- On the adapter path (`TquicClient`/`TquicServer`) a server configured with a
  client CA requires a client certificate and rejects a handshake without one.
  The verified chain must carry exactly one SPIFFE URI SAN in the configured
  trust domain, and `SpiffePolicy` enforces per-method RBAC.
- CRLs are enforced at handshake with `CrlPolicy::Required` (fail closed) as
  the default; certificates, keys, trust anchors and CRLs can be hot-rotated.
  See [`docs/SECURITY.md`](docs/SECURITY.md) for the accurate detail.
- `AsyncTquicClient` (`transport::worker`) installs its TLS config but does not
  yet expose the peer identity to callers. Use the adapter path for
  identity-based authorization.

## Known gaps and caveats

These are deliberate, documented limitations, not vulnerabilities:

- **No OCSP.** CRLs are supported, but OCSP (including stapled OCSP) is not.
  A certificate that was valid when a connection was established is not
  re-checked for the lifetime of that connection, so revocation applies to the
  next handshake.
- **No CRL coverage for the server's own chain** beyond the peer CA anchors and
  CRLs that are explicitly configured.
- **Worker-pool rotation and identity.** `AsyncTquicClient` installs TLS at
  construction and does not expose peer identity; hot rotation and
  identity-based authorization use the adapter path.
- **Early data.** The server config hard-codes `enable_early_data = true`, but
  the client passes `false`, so 0-RTT is not negotiated by the adapter. If a
  caller enables client early data, request replay is a risk it must handle.
- **Prototype dependencies.** The dependency set is audited by `cargo audit` in
  CI under the policy in
  [`docs/known-issues/audit-policy.md`](docs/known-issues/audit-policy.md).
  Informational and unmaintained advisories are printed but do not fail the
  build.

## What is in scope

- TLS verification bypass, downgrade, or a code path that claims to verify and
  does not.
- Authentication or authorization bypass in `interceptor::auth`.
- Panics, unbounded allocation, or memory unsafety reachable from untrusted
  network input.
- Rate-limit or DDoS-protection bypass in `rpc::rate_limit`.
- Remote denial of service in the transport adapter or benchmark server.

## What is out of scope

- Missing features that are documented as missing (OCSP, worker-pool peer
  identity, SPIFFE federation, connection migration, loss recovery tuning).
- Performance differences against tonic; those are measured and published in
  [`docs/GRPC_VS_QUICCRUNCH.md`](docs/GRPC_VS_QUICCRUNCH.md).
- Reports that require running the code with verification explicitly disabled.
- Issues in dependencies that have no exploitable path here; report those
  upstream and open a normal issue if the lockfile needs an update.
