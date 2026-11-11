# Legacy sources (not built)

These files target APIs that no longer exist in the current `quiccrunch-core`
(the pre-RPC high-level client/server surface, the old worker-pool and
`QuicTransport::with_settings` construction paths, and a `bdd` example that is
not in the workspace). They were moved here instead of being deleted so the
history and test intent are preserved.

They are intentionally outside `quiccrunch-core/tests|benches|examples`, so
Cargo does not compile them and `cargo build/test --all-targets` reflects only
maintained code. Port them against the current API before moving them back.

`quiccrunch-ffi` is likewise excluded from the workspace for the same reason
(it depends on the disabled high-level client/server API).
