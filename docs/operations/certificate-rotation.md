# Certificate and CRL rotation operations

This note is for operators of the QuicCrunch tQUIC adapter. It covers rotating
server certificates, client certificates, trust anchors and CRLs without a
restart, and publishing CRLs safely.

The implementation is in `quiccrunch-core/src/revocation.rs` and
`quiccrunch-core/src/transport/adapter.rs`. The security policy is in
[`../SECURITY.md`](../SECURITY.md).

## Golden rules

1. **Never edit certificate, key or CRL files in place.** Write the complete
   new object to a temporary file in the same directory, `fsync` it, then
   `rename` it over the target. The `rename(2)` is atomic; readers see either
   the old file or the complete new file, never a partial one.
   `quiccrunch_core::revocation::write_atomic(path, bytes)` does this.
2. **A bad reload never replaces good material.** If the new file fails to
   parse, or a CRL signature does not verify against its issuer, the reload
   fails, the old material stays active and `*_reload_failure` is incremented.
   Fix the file and reload again.
3. **Rotation is atomic per material set.** The adapter builds a whole
   candidate TLS configuration/CRL bundle before swapping a single `Arc`.
   Existing connections keep their verified identity; only new handshakes see
   the new material. Nothing is dropped.
4. **Fail closed.** When CRLs are configured, a peer whose issuer has no
   currently valid CRL is rejected. Expired CRLs reject handshakes. Do not use
   `CrlPolicy::IfPresent` unless you have explicitly accepted soft-fail.

## Configuring revocation

```rust
RuntimeOptions {
    client_ca_file: Some("/etc/quiccrunch/clients-ca.pem".into()),
    trust_domain: Some("cluster.local".into()),
    crl_files: vec![
        "/etc/quiccrunch/crls/clients.crl.pem".into(),
        "/etc/quiccrunch/crls/partners.crl.der".into(),
    ],
    crl_policy: CrlPolicy::Required,          // default
    reload_interval: Some(Duration::from_secs(60)), // default
    ..RuntimeOptions::default()
}
```

* PEM and DER CRLs are accepted. A PEM file may contain more than one CRL.
* Each CRL issuer must be present in the configured trust anchors (the client
  CA bundle on a server, `ca_cert_file` on a client). A CRL from an unknown
  issuer is a configuration/reload error.
* A CRL without `nextUpdate` is rejected as unusable: it can never be shown to
  be current.
* CRLs are enforced on both sides: a server checks the client chain, a client
  checks the server chain, whenever that side verified the peer chain.

## Publishing a CRL

1. Generate the new CRL with your CA tooling, signed by the same issuer key and
   with a fresh, monotonically increasing `crlNumber`.
2. Set `thisUpdate` to now and `nextUpdate` to the next scheduled publication.
   Keep the validity window shorter than the publication interval so a missed
   publication fails closed rather than silently accepting stale data.
3. Verify the CRL locally:

   ```bash
   openssl crl -in clients.crl.pem -noout -issuer -lastupdate -nextupdate
   openssl crl -in clients.crl.pem -noout -text | grep -A3 'Revoked Certificates'
   ```

4. Publish atomically on every node that verifies the issuer:

   ```bash
   install -m 0644 clients.crl.pem /etc/quiccrunch/crls/.clients.crl.pem.tmp
   sync /etc/quiccrunch/crls/.clients.crl.pem.tmp
   mv /etc/quiccrunch/crls/.clients.crl.pem.tmp /etc/quiccrunch/crls/clients.crl.pem
   sync /etc/quiccrunch/crls
   ```

5. The bounded reloader picks the file up at the next interval. To force it
   immediately, call `TquicServer::reload_tls_material()` /
   `TquicClient::reload_tls_material()` from the application, for example from a
   SIGHUP handler. SIGHUP is not installed by the library itself; wiring the
   signal to this method is an application decision and avoids surprising an
   embedding process.

6. Confirm `crl_reload_success` increased and `crl_reload_failure` did not. Check
   `crl_material_age_secs` to confirm the expected bundle is active.

A revoked peer's **next** connection is rejected. Connections that are already
established are not torn down, because that would be a transport-layer policy
change mid-flight; use short-lived certificates if you need revocation to take
effect within a bounded time on live connections.

## Rotating a server certificate/key pair

1. Issue the new certificate from the same (or a trusted) CA with a distinct
   serial. Stage both the certificate chain and its matching private key.
2. Publish both files with `write_atomic`/`rename`. Replace the certificate and
   key as close together as possible; a reload acquires a consistent pair from
   the files as they exist at that moment.
3. Trigger `reload_tls_material()`. `cert_reload_success` increments,
   `cert_reload_failure` stays flat, and `cert_material_age_secs` resets.
4. Verify with a **new** connection (for example
   `openssl s_client -quic` or a client that prints
   `TquicConnection::peer_identity()`): it must show the new certificate/SPIFFE
   ID. Existing connections continue to work.
5. If the new material is wrong, republish the previous pair atomically and
   reload again. The old pair remains on disk/backup for rollback; the adapter
   never needs a restart.

## Rotating trust anchors / the client CA bundle

1. Stage a bundle containing the old **and** new CA certificates during the
   overlap window so both old and new peers are accepted.
2. Publish the bundle atomically and reload. New handshakes use the combined
   bundle; established connections are unaffected.
3. Roll clients/servers to the new CA.
4. Stage a bundle with only the new CA, publish atomically and reload. Peers
   still using the removed CA are now rejected on new handshakes.
5. Watch `mtls_rejections` during the final step to catch workloads that missed
   the rollout.

## Metrics to alarm on

| Metric | Meaning / action |
| --- | --- |
| `cert_reload_failure` | New certificate/key/trust-anchor material did not load. Previous material is still active; fix and reload. |
| `crl_reload_failure` | CRL bundle did not load (malformed, bad signature, unknown issuer, missing nextUpdate). Previous bundle is still active. |
| `cert_material_age_secs` / `crl_material_age_secs` | Age of the active material. Alert when a CRL approaches `nextUpdate` or material is older than the rotation SLO. |
| `revocation_rejections_revoked` | A peer presented a serial listed in a CRL. |
| `revocation_rejections_unusable_crl` | No valid (current) CRL for the peer's issuer. Check publication and clocks. |
| `revocation_rejections_missing_crl` | No CRL at all for the peer's issuer. Add the issuer's CRL or remove the issuer from the trusted bundle. |
| `revocation_rejections_malformed_certificate` | The verified chain's leaf could not be parsed; investigate the peer/CA. |
