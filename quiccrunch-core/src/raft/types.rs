//! Raft RPC contract types.
//!
//! These types are the logical (Rust) view of the Cap'n Proto contract in
//! `schema/raft.capnp`. They deliberately do not depend on the wire format so
//! an `InMemoryRaftTransport` and a QUIC transport are interchangeable.

use std::fmt;
use std::time::Duration;

/// Version of the Raft RPC contract carried in every request and response.
pub const RAFT_CONTRACT_VERSION: u16 = 1;

/// Maximum encoded frame size, including the 4-byte length prefix.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of log entries in one AppendEntries RPC.
pub const MAX_ENTRIES: usize = 4096;
/// Maximum size of one log entry payload.
pub const MAX_ENTRY_BYTES: usize = 4 * 1024 * 1024;
/// Maximum size of one snapshot chunk payload.
pub const MAX_SNAPSHOT_CHUNK_BYTES: usize = 4 * 1024 * 1024;
/// Maximum length of a leader address string.
pub const MAX_ID_BYTES: usize = 128;
/// Default per-RPC deadline.
pub const DEFAULT_RPC_TIMEOUT: Duration = Duration::from_secs(5);
/// Default connection establishment timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// Default maximum concurrent in-flight RPCs per peer.
pub const DEFAULT_MAX_IN_FLIGHT_PER_PEER: usize = 64;
/// Default time a caller waits for a per-peer queue slot before shedding.
pub const DEFAULT_QUEUE_WAIT: Duration = Duration::from_millis(25);
/// Default maximum reconnect attempts before returning Unavailable.
pub const DEFAULT_MAX_CONNECT_ATTEMPTS: u32 = 6;
/// Default base reconnect backoff.
pub const DEFAULT_RECONNECT_BASE: Duration = Duration::from_millis(25);
/// Default maximum reconnect backoff.
pub const DEFAULT_RECONNECT_MAX: Duration = Duration::from_secs(2);
/// Default snapshot chunk size used by the high-level upload helper.
pub const DEFAULT_SNAPSHOT_CHUNK_SIZE: usize = 256 * 1024;

/// A Raft peer identity and address.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RaftPeer {
    pub node_id: u64,
    pub addr: String,
}

impl RaftPeer {
    pub fn new(node_id: u64, addr: impl Into<String>) -> Self {
        Self {
            node_id,
            addr: addr.into(),
        }
    }
}

/// Leader redirect information. Mirrors the `x-distri-kv-leader` metadata
/// contract: callers can retry at `addr` without another discovery round trip.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LeaderHint {
    pub leader_id: u64,
    pub leader_addr: String,
}

impl LeaderHint {
    pub fn new(leader_id: u64, leader_addr: impl Into<String>) -> Self {
        Self {
            leader_id,
            leader_addr: leader_addr.into(),
        }
    }

    pub fn is_complete(&self) -> bool {
        // Node id 0 is a valid Raft id; a hint is usable if it names a peer.
        !self.leader_addr.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ErrorCode {
    Ok = 0,
    Unavailable = 1,
    NotLeader = 2,
    TermMismatch = 3,
    SnapshotStale = 4,
    FrameTooLarge = 5,
    InvalidRequest = 6,
    DeadlineExceeded = 7,
    Backpressure = 8,
    Internal = 9,
    Protocol = 10,
}

impl ErrorCode {
    pub fn from_u16(value: u16) -> Self {
        match value {
            0 => Self::Ok,
            1 => Self::Unavailable,
            2 => Self::NotLeader,
            3 => Self::TermMismatch,
            4 => Self::SnapshotStale,
            5 => Self::FrameTooLarge,
            6 => Self::InvalidRequest,
            7 => Self::DeadlineExceeded,
            8 => Self::Backpressure,
            9 => Self::Internal,
            _ => Self::Protocol,
        }
    }

    pub fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::Unavailable | Self::Backpressure | Self::DeadlineExceeded
        )
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
            Self::NotLeader => "not-leader",
            Self::TermMismatch => "term-mismatch",
            Self::SnapshotStale => "snapshot-stale",
            Self::FrameTooLarge => "frame-too-large",
            Self::InvalidRequest => "invalid-request",
            Self::DeadlineExceeded => "deadline-exceeded",
            Self::Backpressure => "backpressure",
            Self::Internal => "internal",
            Self::Protocol => "protocol",
        };
        f.write_str(name)
    }
}

/// An explicit Raft transport error. `code` is the stable part of the
/// contract; `message` is diagnostic only and must never be parsed by callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaftError {
    pub code: ErrorCode,
    pub message: String,
    pub leader_hint: Option<LeaderHint>,
    pub retryable: bool,
    pub retry_after_ms: Option<u64>,
}

impl RaftError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            leader_hint: None,
            retryable: code.is_retryable(),
            retry_after_ms: None,
        }
    }

    pub fn not_leader(hint: LeaderHint) -> Self {
        Self {
            code: ErrorCode::NotLeader,
            message: "not the leader for this partition".into(),
            leader_hint: Some(hint),
            retryable: false,
            retry_after_ms: None,
        }
    }

    pub fn term_mismatch(current_term: u64) -> Self {
        Self::new(
            ErrorCode::TermMismatch,
            format!("stale term; current term is {current_term}"),
        )
    }

    pub fn snapshot_stale(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::SnapshotStale, message)
    }

    pub fn frame_too_large(size: usize, max: usize) -> Self {
        Self::new(
            ErrorCode::FrameTooLarge,
            format!("frame too large: {size} > {max}"),
        )
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }

    pub fn deadline(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::DeadlineExceeded, message)
    }

    pub fn backpressure(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Backpressure, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Protocol, message)
    }

    pub fn with_retry_after(mut self, ms: u64) -> Self {
        self.retry_after_ms = Some(ms);
        self
    }
}

impl fmt::Display for RaftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(hint) = &self.leader_hint {
            write!(f, " (leader {} at {})", hint.leader_id, hint.leader_addr)?;
        }
        Ok(())
    }
}

impl std::error::Error for RaftError {}

pub type RaftResult<T> = std::result::Result<T, RaftError>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogEntry {
    pub term: u64,
    pub index: u64,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppendEntriesRequest {
    pub partition_id: u64,
    pub term: u64,
    pub leader_id: u64,
    pub prev_log_index: u64,
    pub prev_log_term: u64,
    pub entries: Vec<LogEntry>,
    pub leader_commit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppendEntriesResponse {
    pub term: u64,
    pub success: bool,
    pub match_index: u64,
    pub conflict_index: u64,
    pub conflict_term: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestVoteRequest {
    pub partition_id: u64,
    pub term: u64,
    pub candidate_id: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestVoteResponse {
    pub term: u64,
    pub vote_granted: bool,
}

/// One frame of a streaming InstallSnapshot RPC.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallSnapshotChunk {
    pub partition_id: u64,
    pub term: u64,
    pub leader_id: u64,
    pub last_index: u64,
    pub last_term: u64,
    pub offset: u64,
    pub chunk: Vec<u8>,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InstallSnapshotAck {
    pub term: u64,
    pub success: bool,
    pub bytes_accepted: u64,
    pub next_offset: u64,
    pub done: bool,
}

/// A whole snapshot to stream. The transport splits `data` into chunks and
/// resumes from the last acknowledged offset if the stream is interrupted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotUpload {
    pub partition_id: u64,
    pub term: u64,
    pub leader_id: u64,
    pub last_index: u64,
    pub last_term: u64,
    pub data: Vec<u8>,
    pub start_offset: u64,
    pub chunk_size: usize,
}

impl SnapshotUpload {
    pub fn new(
        partition_id: u64,
        term: u64,
        leader_id: u64,
        last_index: u64,
        last_term: u64,
        data: Vec<u8>,
    ) -> Self {
        Self {
            partition_id,
            term,
            leader_id,
            last_index,
            last_term,
            data,
            start_offset: 0,
            chunk_size: DEFAULT_SNAPSHOT_CHUNK_SIZE,
        }
    }
}

/// Per-transport counters. `queue_depth` is the current number of RPCs waiting
/// for a per-peer in-flight slot; `max_queue_depth` and `shed_total` make
/// backpressure visible to operators.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RaftMetricsSnapshot {
    pub append_entries_total: u64,
    pub request_vote_total: u64,
    pub snapshot_chunks_total: u64,
    pub queue_depth: u64,
    pub max_queue_depth: u64,
    pub shed_total: u64,
    pub timeout_total: u64,
    pub reconnect_total: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub not_leader_total: u64,
}
