//! QuicCrunch: A high-performance RPC library using QUIC and Cap'n Proto

mod error;
pub mod transport;
mod stream;
pub mod test_utils;
pub mod metrics;
pub mod power;


pub use error::{Result, TransportError};
pub use transport::{QuicTransport, Transport, Connection, TlsConfig, Incoming};
pub use stream::QuicStream;
pub use metrics::{Metrics, Operation};

pub mod schema {
    pub mod frame_capnp {
        include!(concat!(env!("OUT_DIR"), "/schema/frame_capnp.rs"));
    }

    // Re-export types
    pub use frame_capnp::{
        message_header,
        trace_context,

    };
}

// Convenience re-exports for generated code
pub use schema::frame_capnp::{
    message_header,
    trace_context,
    frame,
};