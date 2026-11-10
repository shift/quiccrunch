//! Raft RPC surface over QuicCrunch.
//!
//! This module provides:
//! - a versioned Cap'n Proto contract for `AppendEntries`, `RequestVote` and a
//!   streaming `InstallSnapshot` (`codec`);
//! - a transport trait that distri-kv can implement against (`RaftTransport`);
//! - a QUIC implementation with per-peer connection reuse, reconnect backoff,
//!   deadlines, bounded in-flight queues, metrics and leader hints (`quic`);
//! - an in-process reference implementation (`InMemoryRaftTransport`).
//!
//! See `docs/RAFT_RPC.md` for the contract, limits and adapter guide.

pub mod codec;
pub mod quic;
pub mod transport;
pub mod types;

pub use codec::{
    decode_request, decode_response, encode_append_entries, encode_append_entries_response,
    encode_error_response, encode_request_vote, encode_request_vote_response, encode_snapshot_ack,
    encode_snapshot_chunk, peek_kind, RequestMessage, ResponseMessage,
};
pub use quic::{serve_connection, serve_listener, QuicRaftConfig, QuicRaftTransport};
pub use transport::{InMemoryRaftTransport, RaftMetrics, RaftService, RaftTransport};
pub use types::*;
