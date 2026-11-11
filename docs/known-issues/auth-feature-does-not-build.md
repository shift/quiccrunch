# The `auth` feature does not compile

Status: **open**. Confirmed on `88989bd` with rustc 1.89.0.

`quiccrunch-core` exposes JWT/RBAC support in `src/interceptor/` behind the
non-default `auth` feature:

```bash
nix develop -c cargo build -p quiccrunch-core --features auth
```

That build fails with 13 errors. The default build and CI do not enable the
feature, so the failures are not visible in the normal pipeline:

- `src/interceptor/auth.rs` uses `RpcError::Internal`, which does not exist on
  `rpc::error::RpcError` (10 occurrences).
- `AuthInterceptor::before_request` takes `service::RpcContext`, but the
  `Interceptor` trait expects `rpc::RpcContext`.
- `src/interceptor/mod.rs` awaits `circuit_breaker.record_success()` and
  `record_failure()`, which return `()`.

Consequences:

- The README does not include an authentication snippet; the previous one could
  not compile.
- `scripts/bench.sh`, the comparison harness and the Raft RPC tests never touch
  this code path, so none of the measured results depend on it.
- Do not describe authentication as available. It is not part of any working
  build.

Fixing it requires, at minimum: mapping the missing error variant onto an
existing `RpcError`, unifying the `RpcContext` type, and removing the spurious
`.await`s. The interceptor also has no end-to-end test, so a fix should add a
JWT round-trip test before the feature is advertised.