# Transport security

This document records the TLS defaults and known gaps of the QuicCrunch
transport. It is scoped to the tQUIC adapter (`quiccrunch-core/src/transport/adapter.rs`),
which is the real data path used by the benchmarks and the comparison harness.

## Client certificate verification: ON by default

As of the security fix on `bench/vs-grpc`:

| Setting | Default |
| --- | --- |
| `RuntimeOptions::verify_server_cert` (`Default`, `benchmark`, `benchmark_client`) | `true` |
| `WorkerConfig::verify_cert` | `true` |
| `RuntimeOptions::ca_cert_file` | `None` (platform trust store) |

The effective client policy is the **logical AND** of the two switches:

```text
verify = RuntimeOptions::verify_server_cert && WorkerConfig::verify_cert
```

Either switch can turn verification off, but both must be on for it to be
enabled. A caller cannot accidentally re-enable verification on a transport it
deliberately marked insecure.

When `verify` is true:

* the client calls `TlsConfig::set_verify(true)` (`SSL_VERIFY_PEER` in
  BoringSSL);
* if `ca_cert_file` (or `WorkerConfig::ca_cert_file`) is set, that PEM file is
  loaded as the trust anchor;
* otherwise the platform trust store loaded by `Context::new()` is used.

When `verify` is false, the adapter emits a `warn!` naming both switches and
stating that this is only acceptable for loopback self-signed test
certificates. **There is no `allow_insecure_tls` feature or flag in this
repository**; the two switches above are the only paths that disable
verification on the adapter.

## Explicit opt-outs

Only loopback tests with self-signed certificates disable verification, and
every such call site says so inline:

```rust
TquicClient::with_options(RuntimeOptions {
    // Loopback self-signed test certificate: verification is explicitly off.
    verify_server_cert: false,
    ..RuntimeOptions::default()
})
```

Call sites: `comparison` uses verification **on** (with a CA); the benchmark
binary `pipelined_benchmark`, the `pipelined_client`/`quic_client` bins, the
tQUIC benches, examples and tests, and one `WorkerConfig` example opt out
explicitly. `RuntimeOptions::insecure_loopback_test()` was deliberately not
added: a helper would hide the decision behind a function name.

## Client certificate presentation

Clients present a certificate when `RuntimeOptions::client_cert_file` and
`RuntimeOptions::client_key_file` are both set (or the equivalent
`WorkerConfig` fields). Setting only one is a configuration error. The same
fields exist on the worker-pool `WorkerConfig`.

## Server-side mTLS

The adapter's server branch is configured through `RuntimeOptions`:

| Option | Meaning |
| --- | --- |
| `client_ca_file` | PEM file **or** directory of client trust anchors. Presence turns mTLS on. |
| `trust_domain` | Required with `client_ca_file`; the SPIFFE trust domain peer SANs must match. |

When `client_ca_file` is set the server:

1. loads those anchors into BoringSSL's verify store;
2. calls the patched `TlsConfig::set_verify_client_cert(true)`, which sets
   `SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT` and therefore requests a
   client certificate and makes a missing or unverifiable one a handshake
   failure;
3. re-checks the presented chain in `on_conn_established` as defence in depth,
   closes the connection with application error code `0x100`/`0x101` if no
   verified identity is available, and does not report the connection as
   established to the runtime;
4. increments `RuntimeMetrics::mtls_rejections` on every rejection and
   `mtls_handshakes` only after the SPIFFE SAN has been validated.
   `RuntimeMetrics::mtls_enabled` reports whether this runtime is an mTLS
   server.

A non-existent CA path is a startup error. This matters because tQUIC's
`set_ca_certs` treats a non-file path as a directory and BoringSSL accepts a
missing directory as an empty trust store; the adapter refuses the path before
handing it to BoringSSL.

When `client_ca_file` is **not** set the server stays on one-way TLS and emits
a `warn!` saying mTLS is OFF. It does not claim to authenticate clients.

## Peer certificate extraction: patched tQUIC 1.6.0

Finding: tQUIC 1.6.0 *does* expose `TlsSession::peer_cert()` and
`TlsSession::peer_cert_chain()` internally (backed by
`SSL_get0_peer_certificates`), but `Connection` exposes no accessor and
`TlsConfig::set_verify` only sets `SSL_VERIFY_PEER`; there is no safe public way
for an application to (a) require a client certificate or (b) read the verified
peer chain.

We vendor the unmodified upstream crate under `vendor/tquic/` and apply the
minimal patch in [`../patches/tquic-1.6.0-peer-identity.patch`](../patches/tquic-1.6.0-peer-identity.patch):

* `Connection::peer_cert_chain() -> Option<Vec<Vec<u8>>>` and
  `Connection::peer_cert() -> Option<Vec<u8>>` return the verified peer chain in
  DER (leaf first);
* `TlsConfig::set_verify_client_cert(bool)` and the BoringSSL
  `Context::set_verify_client_cert(bool)` set
  `SSL_VERIFY_PEER | SSL_VERIFY_FAIL_IF_NO_PEER_CERT`.

`[patch.crates-io] tquic = { path = "vendor/tquic" }` in the workspace
`Cargo.toml` selects the patched copy. Maintenance cost is honest and bounded:
three source files, ~60 lines, no changes to QUIC/TLS internals. Upgrading
tQUIC means re-applying this patch against the new tag and re-running the mTLS
tests. The patch is written in upstream style and is a reasonable upstream PR
(peer-chain accessors are generally useful; the verify mode is a standard
BoringSSL knob).

## SPIFFE identity model

The verified leaf certificate must contain exactly one `spiffe://` URI SAN.
`quiccrunch-core/src/identity.rs` parses it, normalises it, and rejects:

* a missing or non-`spiffe://` SAN (`MissingSpiffeSan`);
* more than one SPIFFE URI, an empty trust domain or path, uppercase trust
  domains, userinfo/port components, and empty/`.`/`..` path segments
  (`MalformedSpiffeId`);
* a SAN whose trust domain differs from `RuntimeOptions::trust_domain`
  (`TrustDomainMismatch`).

A successful identity is exposed as `PeerIdentity` via
`TquicConnection::peer_identity()` (both client and server sides) and
`TquicListener::accept_with_identity()`. On the client the identity is exposed
only when `verify_server_cert`/`WorkerConfig::verify_cert` are on; an
unverified server chain is never turned into an identity. Kubernetes-style
paths expose `SpiffeId::namespace()` and `SpiffeId::service_account()`.

RBAC mapping: the server copies `PeerIdentity::spiffe_id()` into request
metadata under `x-quiccrunch-peer-spiffe-id` when it builds the `RpcContext`
(never from client-supplied metadata). `SpiffePolicy` maps a method
`Service.Method` to an explicit allowlist of SPIFFE IDs and denies everything
else, including a missing identity. The `auth`-feature
`SpiffeIdentityInterceptor` in `interceptor/identity.rs` applies that policy in
the existing interceptor chain. Methods without a rule are unchanged; use
`with_require_identity(true)` to fail closed for the whole service.

## Remaining gaps

* Certificate revocation (CRL/OCSP) is not checked. The trust anchor and
  expiry are checked by BoringSSL; a revoked-but-unexpired certificate is
  still accepted.
* Trust anchors and the server certificate are loaded at startup. Issuing a
  second client certificate from the same CA works without a restart (tested);
  rotating the CA or the server certificate still requires a restart.
* SPIFFE bundle-endpoint federation and JWT-SVID support are not implemented;
  this is X.509-SVID identity only.
* `AsyncTquicClient`/`worker.rs` now installs its `TlsConfig`, honours
  `verify_cert`/`ca_cert_file`, presents a client certificate and can require
  a client CA, but it does not yet expose the peer identity to callers. Use the
  adapter (`TquicClient`/`TquicServer`) for identity-based authorization.
* The client verifies the server certificate chain but the adapter has no
  client-side SPIFFE *policy* for server identities; the server identity is
  exposed for callers to check.
* 0-RTT is disabled on the adapter (`new_server_config(..., false)` and client
  early data `false`). If a caller enables it, request replay must be treated
  as a risk.

## Revocation and trust-anchor rotation

Both are unimplemented, and callers must plan around them:

* **No CRL or OCSP.** A certificate that verified at handshake time remains
  trusted for the lifetime of the connection. There is no online revocation
  check and no reload hook for a revocation list. Revoking a peer means
  removing its CA from the trust anchor and restarting the client.
* **No CA rotation without a restart.** `RuntimeOptions::ca_cert_file` (or
  `WorkerConfig::ca_cert_file`) is read once when the client is constructed.
  Replacing the PEM on disk does not affect running clients. `CertificateRotator`
  in `src/production.rs` is a standalone helper; no binary wires it to the
  adapter's trust anchor.
* The policy and disclosure process are in [`../SECURITY.md`](../SECURITY.md).

## Tests

`quiccrunch-core/tests/mtls_identity.rs` generates all certificates in-test and
runs over real loopback UDP sockets:

* matching CA + client certificate completes mTLS and exposes the client
  `spiffe://...` identity on the server;
* server rejects: no client certificate, a certificate from a foreign CA, an
  expired certificate, and a SPIFFE SAN outside the configured trust domain;
* client rejects a server certificate signed by a foreign CA by default;
* the SPIFFE identity reaches `SpiffePolicy` and a method allowlist denies an
  identity it does not contain;
* a second client certificate from the same CA is accepted without restarting
  the server.

`quiccrunch-core/src/transport/adapter.rs` also keeps:

* `runtime_options_verify_server_cert_by_default` — pins `true` on
  `RuntimeOptions::default()`, `benchmark()`, `benchmark_client()` and
  `WorkerConfig::default()`.
* `self_signed_server_is_rejected_with_default_options`.
* `explicit_opt_out_accepts_self_signed_loopback_server`.

Reproduce:

```bash
cd /tmp/opencode/qc-id
nix develop -c env CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-id \
  cargo test --offline -p quiccrunch-core --test mtls_identity -- --test-threads=1
```