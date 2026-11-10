# Backend messaging and peer identity

This document describes how a backend connection's authenticated identity is
carried into QuicCrunch's RPC authorization layer. It complements
[`SECURITY.md`](SECURITY.md), which covers the TLS/mTLS configuration itself.

## Connection identity (adapter path)

`TquicConnection::peer_identity() -> Option<PeerIdentity>` returns the identity
extracted from the **verified** peer certificate chain (SPIFFE X.509-SVID).
On the server this is the client workload; on the client it is the server
workload. `TquicListener::accept_with_identity()` returns the same value next to
the accepted connection.

The identity is `None` when:

* the peer presented no certificate (normal for a one-way TLS client), or
* the certificate has no `spiffe://` URI SAN, or
* mTLS is off on the server (a client certificate was never requested), or
* the local TLS stack did not verify the peer chain (client with
  `verify_server_cert` off). An unverified certificate is never exposed as an
  identity.

On a server configured with `RuntimeOptions::client_ca_file`, `None` cannot
reach the accept path for a live connection: the handshake fails, or the
connection is closed in `on_conn_established`, and
`RuntimeMetrics::mtls_rejections` is incremented.

## From connection to `RpcContext`

The server is responsible for copying the transport identity into the request
context. Use the provided helper so the metadata key cannot drift:

```rust
let mut metadata = HashMap::new();
if let Some(identity) = connection.peer_identity() {
    identity.insert_metadata(&mut metadata); // x-quiccrunch-peer-spiffe-id
}
```

`PeerIdentity::insert_metadata` writes
`x-quiccrunch-peer-spiffe-id = spiffe://<trust-domain>/ns/<ns>/sa/<sa>`.

**Trust boundary:** never copy this key from peer-supplied metadata. It must
only be populated from `TquicConnection::peer_identity()`. The interceptor
cannot tell the difference, so the server-side copy is the enforcement point.

## Method policy

`quiccrunch_core::identity::SpiffePolicy` maps a `Service.Method` key to the
exact SPIFFE IDs allowed to call it:

```rust
let mut policy = SpiffePolicy::new();
policy.allow(
    "Raft.AppendEntries",
    vec![SpiffeId::parse("spiffe://cluster.local/ns/kv/sa/raft")?],
);
policy.deny_all("Admin.Shutdown");
```

* a method with no rule is not restricted by this policy;
* an empty allowlist denies every identity (fail the method closed);
* a missing identity is denied for any protected method.

The `auth`-feature `SpiffeIdentityInterceptor` applies this policy in the
existing interceptor chain. Its `with_require_identity(true)` mode additionally
requires a transport identity for methods without a rule.

This is orthogonal to JWT authentication: SPIFFE proves *which workload
process* opened the connection, while a JWT (or another token) can carry
end-user or tenant claims. A typical server enables both.

## Worker-pool path

`AsyncTquicClient`/`transport/worker.rs` now installs a `TlsConfig`, honours
`WorkerConfig::verify_cert` and `WorkerConfig::ca_cert_file`, presents
`client_cert_file`/`client_key_file`, and can require client certificates via
`client_ca_file` plus `trust_domain`. It does **not** yet expose the peer
identity to the async API, so identity-based authorization should use the
adapter (`TquicClient`/`TquicServer`) until that is added.

## Reproduce

```bash
cd /tmp/opencode/qc-id
nix develop -c env CARGO_TARGET_DIR=/run/media/shift/80c4f9f3-68ab-424f-830b-1c14514bc50f/code/.cargo-target-qc-id \
  cargo test --offline -p quiccrunch-core --test mtls_identity -- --test-threads=1
```

The tests generate a CA, server certificate, client certificates (including a
foreign-CA and an expired one) and a rogue-trust-domain SAN, then run real mTLS
handshakes over loopback UDP.