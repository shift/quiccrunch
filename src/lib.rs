// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! QuicCrunch: A high-performance QUIC benchmarking framework using Cap'n Proto serialization
//!
//! This library provides comprehensive QUIC protocol benchmarking capabilities with support for
//! multiple backends (Quinn and tQUIC), performance metrics collection, and visualization tools.

mod error;
pub mod transport;
mod stream;
pub mod test_utils;
pub mod metrics;
pub mod power;
pub mod config;
pub mod client;
pub mod server;

pub use error::{Result, TransportError};
pub use transport::{QuicTransport, Transport, Connection, TlsConfig, Incoming};
pub use stream::QuicStream;
pub use metrics::{Metrics, Operation};
pub use client::{QuicClient, QuicConnection};
pub use server::{QuicServer, QuicListener, MessageHandler, EchoHandler, run_echo_server, run_server_with_handler};

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