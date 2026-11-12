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

* Revocation is checked at handshake time (CRL only; see below). A certificate
  that was valid when a connection was established is not re-checked for the
  lifetime of that connection. Revoking a peer guarantees that its *next*
  connection is rejected, not that an already-open connection is torn down.
* OCSP (including stapled OCSP) is not implemented; the reason and the interim
  stance are documented below.
* Trust anchors, the server certificate/key, the client CA bundle and CRLs can
  be hot-rotated (see below). The worker-pool path (`AsyncTquicClient`/`worker.rs`)
  still installs its TLS config at construction; use the adapter
  (`TquicClient`/`TquicServer`) when rotation is required.
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

## Certificate revocation: CRL

Revocation is implemented with `x509-parser` in
`quiccrunch-core/src/revocation.rs`. BoringSSL has no CRL hook in this vendored
tQUIC 1.6.0 build (no `X509_STORE_add_crl`/`CRL_CHECK` wrapper and no access to
the verified chain from the TLS context), so the adapter checks the chain that
BoringSSL accepted in `on_conn_established`, **before** the connection is
reported as established and before any application stream is accepted. A
revoked peer can complete the TLS handshake at the protocol level but is
immediately closed with QUIC application error `0x102`; no application data
flows.

Configuration (`RuntimeOptions`):

| Option | Meaning |
| --- | --- |
| `crl_files: Vec<String>` | PEM or DER CRLs. Non-empty turns revocation on. |
| `crl_policy: CrlPolicy` | `Required` (default) or explicit `IfPresent`. |
| `reload_interval: Option<Duration>` | Bounded reload interval (default 60s); `None` disables periodic reloads. |

Policy, in order:

1. Each CRL is parsed and its signature verified against the issuer
   certificate found in the same trust anchors BoringSSL uses. The issuer must
   be a configured trust anchor; an unknown issuer, malformed CRL, bad
   signature or missing `nextUpdate` makes the whole bundle unusable. At
   startup that is a configuration error; on reload the previous bundle stays
   active.
2. Validity (`thisUpdate`/`nextUpdate`) is evaluated at every handshake, with a
   5-minute clock-skew tolerance. An expired or not-yet-valid CRL for the
   peer's issuer is a **handshake rejection** under `Required`; this is
   deliberately fail closed even if the CRL was valid when it was loaded.
3. A peer whose issuer has no loaded CRL is rejected under `Required`. A peer
   from an issuer with a valid CRL is rejected only if its serial is listed.
4. `CrlPolicy::IfPresent` is the only soft-fail mode. It is never selected
   implicitly: it keeps enforcing any valid CRL but allows a handshake when no
   valid CRL exists for the issuer. Do not use it where revocation is a
   compliance requirement.

CRL files are re-read when their metadata changes (periodic interval) or when
an operator calls `TquicServer::reload_tls_material()` /
`TquicClient::reload_tls_material()` (for example from an application SIGHUP
handler). Established connections keep their identity; new handshakes use the
new list.

## Hot rotation of TLS material

`TlsConfig` is no longer installed as an immutable object. The adapter installs
a custom `tquic::TlsConfigSelector` that holds
`Arc<RwLock<Arc<TlsConfig>>>`; every new handshake asks the selector for the
current configuration, while existing connections keep the `TlsSession` they
created. A reload:

1. builds a complete candidate `TlsConfig` from the configured cert/key,
   client certificate/key and trust-anchor paths (all parsing and
   BoringSSL loading happens against the candidate);
2. only then swaps the `Arc` under the write lock;
3. on any parse/load failure keeps the old `Arc` active and increments a
   failure metric. A partially written file therefore either fails to parse
   (old material stays) or is never seen as valid until it is complete.

Operators must publish material with write-to-temp + `fsync` + `rename`;
`quiccrunch_core::revocation::write_atomic` implements this. See
[`operations/certificate-rotation.md`](operations/certificate-rotation.md) for
the procedure, CRL publishing and a rollback checklist.

Metrics are exposed through `RuntimeMetrics::snapshot()`:

* `cert_reload_success` / `cert_reload_failure`
* `crl_reload_success` / `crl_reload_failure`
* `cert_material_age_secs` / `crl_material_age_secs`
* `revocation_rejections` with the reason counters
  `revocation_rejections_revoked`, `revocation_rejections_unusable_crl`,
  `revocation_rejections_missing_crl`,
  `revocation_rejections_malformed_certificate`.

## OCSP: not implemented, and why

OCSP is **not** supported. This is a deliberate, documented non-implementation
rather than a silent soft-fail:

* the vendored tQUIC/BoringSSL `TlsConfig`/`Context` surface exposes no OCSP
  stapling verification callback (`SSL_CTX_set_tlsext_status_cb`) and no
  `SSL_get_tlsext_status_ocsp_resp` accessor;
* `Connection` exposes only the peer certificate chain (via the local
  peer-identity patch), not the TLS session or the stapled OCSP response;
* tQUIC has no OCSP configuration or relay API, so there is nowhere to install
  a responder URL, cache, nonce or soft/hard-fail policy.

Adding OCSP would require a new patch to BoringSSL/tQUIC that exposes the
stapled response and a verification hook, plus an HTTP client and response
cache. Until then the interim stance is: **use CRLs and short-lived
certificates**. CRLs are enforced fail-closed at handshake as described above;
there is no on-line revocation fetch and no claim of OCSP support.

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

`quiccrunch-core/tests/revocation_rotation.rs` covers revocation and hot
rotation over real loopback UDP sockets with in-test PKI and CRLs:

* revoked client serial rejected, non-revoked client accepted;
* expired CRL for a configured issuer fails the handshake closed;
* missing CRL for the peer's issuer fails closed;
* malformed and unverifiable-signature CRL reloads are rejected with a clear
  error and the previous bundle stays active;
* a CRL reload adds a newly revoked serial without a restart;
* the bounded reload interval picks up a new CRL without a restart;
* server certificate/key rotation serves the new certificate on new handshakes
  while an established connection keeps working;
* trust-anchor rotation accepts a certificate from the added CA and rejects one
  from the removed CA, without a restart.

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