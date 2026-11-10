//! Same-shape head-to-head benchmark: tonic/h2 over TCP+TLS vs QuicCrunch.
//!
//! Both transports use the identical client instrumentation in [`bench`]; only
//! the [`bench::Backend`] implementation differs. See `comparison/README.md`
//! and `docs/GRPC_VS_QUICCRUNCH.md`.

pub mod bench;
pub mod grpc;
pub mod pb {
    //! Generated gRPC types.
    tonic::include_proto!("comparison");
}
pub mod quic;

pub const DEFAULT_CA: &str = "comparison/certs/ca.pem";
pub const DEFAULT_DOMAIN: &str = "localhost";
