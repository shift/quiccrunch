# Doctests disabled for the high-level client/server API

`quiccrunch-core` sets `doctest = false` because the rustdoc examples in
`src/lib.rs`, `src/rpc.rs`, and `src/rpc/codegen.rs` document a high-level
client/server surface (`QuicClient`, `QuicServer`, `TlsConfig`,
`QuicTransport::new`, generated `CalculatorClient`/`CalculatorHandler`, etc.)
that is currently commented out or has a different signature.

The modules remain in the tree (`src/client.rs`, `src/server.rs`) but are not
exported from `src/lib.rs`; `quiccrunch-ffi` is therefore excluded from the
workspace until it is ported.

This is tracked so CI does not pretend doctests pass. Re-enable `doctest`
once the high-level API is restored and the examples are rewritten against it.
