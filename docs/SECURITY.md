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

## Server side

* The adapter's server branch creates `TlsConfig::new_server_config(cert, key,
  alpn, enable_early_data)` and never calls `set_verify(true)` or loads a client
  CA. It therefore does **not** request or verify client certificates: the
  adapter provides one-way TLS only.
* This is a capability gap, not a fail-open verification default. There is no
  code path that *claims* to verify clients and silently does not; mTLS is
  unimplemented. Adding it needs a client-CA option and a deliberate API, so it
  is not part of this fix.
* `distri-kv` needs mTLS/SPIFFE workload identity, which remains a blocker for
  moving the control plane to this adapter.
* The server's `enable_early_data` argument is hard-coded `true`. The client
  config passes `false`, so 0-RTT is not negotiated by the adapter today. If a
  caller enables client early data, it must treat request replay as a risk.

## Separate tQUIC worker-pool path (reported, not changed)

`quiccrunch-core/src/transport/worker.rs` (used by `AsyncTquicClient`) builds a
`TlsConfig` but binds it to `_tls_config` and never calls
`Config::set_tls_config`. It also ignores `WorkerConfig::verify_cert` and
`WorkerConfig::ca_cert_file`. That is a pre-existing bug in a path that is not
the adapter and not the transport measured by `comparison/`. It fails closed in
practice (a QUIC handshake without a TLS config cannot succeed), but it should
be wired up and tested separately before anyone relies on `AsyncTquicClient`
for security decisions.

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

`quiccrunch-core/src/transport/adapter.rs` contains:

* `runtime_options_verify_server_cert_by_default` — pins `true` on
  `RuntimeOptions::default()`, `benchmark()`, `benchmark_client()` and
  `WorkerConfig::default()`.
* `self_signed_server_is_rejected_with_default_options` — starts a server with
  a freshly generated self-signed loopback certificate and asserts that a
  default client cannot complete the handshake (handshake timeout/TLS failure).
* `explicit_opt_out_accepts_self_signed_loopback_server` — proves the opt-out
  path is the only reason the connection is accepted.