//! Cap'n Proto codec for the Raft RPC contract.
//!
//! Every frame is `u32` big-endian length followed by a flat Cap'n Proto
//! message. Both directions enforce the same bounds before allocating or
//! reading variable-length fields.

use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;

use crate::schema::raft_capnp::{
    self, append_entries, append_entries_response, install_snapshot_ack, install_snapshot_chunk,
    leader_hint, log_entry, request, request_vote, request_vote_response, response,
    ErrorCode as WireErrorCode, RpcKind,
};

use super::types::*;

const MAX_TRAVERSAL_WORDS: usize = MAX_FRAME_BYTES / 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestMessage {
    AppendEntries(AppendEntriesRequest),
    RequestVote(RequestVoteRequest),
    InstallSnapshot(InstallSnapshotChunk),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseMessage {
    AppendEntries(AppendEntriesResponse),
    RequestVote(RequestVoteResponse),
    InstallSnapshot(InstallSnapshotAck),
}

fn with_len_prefix(body: Vec<u8>) -> RaftResult<Vec<u8>> {
    let len = body.len();
    if len > MAX_FRAME_BYTES {
        return Err(RaftError::frame_too_large(len, MAX_FRAME_BYTES));
    }
    let mut frame = Vec::with_capacity(4 + len);
    frame.extend_from_slice(&(len as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

fn finish(builder: &Builder<capnp::message::HeapAllocator>) -> RaftResult<Vec<u8>> {
    let body = serialize::write_message_to_words(builder);
    with_len_prefix(body)
}

// ---------------------------------------------------------------------------
// Request encoding
// ---------------------------------------------------------------------------

pub fn encode_append_entries(input: &AppendEntriesRequest) -> RaftResult<Vec<u8>> {
    if input.entries.len() > MAX_ENTRIES {
        return Err(RaftError::new(
            ErrorCode::InvalidRequest,
            format!(
                "too many entries: {} > {}",
                input.entries.len(),
                MAX_ENTRIES
            ),
        ));
    }
    for entry in &input.entries {
        if entry.data.len() > MAX_ENTRY_BYTES {
            return Err(RaftError::frame_too_large(
                entry.data.len(),
                MAX_ENTRY_BYTES,
            ));
        }
    }

    let mut builder = Builder::new_default();
    {
        let mut req = builder.init_root::<request::Builder>();
        req.set_contract_version(RAFT_CONTRACT_VERSION);
        req.set_kind(RpcKind::AppendEntries);
        let mut b = req.reborrow().init_append_entries();
        b.set_partition_id(input.partition_id);
        b.set_term(input.term);
        b.set_leader_id(input.leader_id);
        b.set_prev_log_index(input.prev_log_index);
        b.set_prev_log_term(input.prev_log_term);
        b.set_leader_commit(input.leader_commit);
        let mut entries = b.reborrow().init_entries(input.entries.len() as u32);
        for (i, entry) in input.entries.iter().enumerate() {
            let mut out = entries.reborrow().get(i as u32);
            out.set_term(entry.term);
            out.set_index(entry.index);
            out.set_data(&entry.data);
        }
    }
    finish(&builder)
}

pub fn encode_request_vote(input: &RequestVoteRequest) -> RaftResult<Vec<u8>> {
    let mut builder = Builder::new_default();
    {
        let mut req = builder.init_root::<request::Builder>();
        req.set_contract_version(RAFT_CONTRACT_VERSION);
        req.set_kind(RpcKind::RequestVote);
        let mut b = req.reborrow().init_request_vote();
        b.set_partition_id(input.partition_id);
        b.set_term(input.term);
        b.set_candidate_id(input.candidate_id);
        b.set_last_log_index(input.last_log_index);
        b.set_last_log_term(input.last_log_term);
    }
    finish(&builder)
}

pub fn encode_snapshot_chunk(input: &InstallSnapshotChunk) -> RaftResult<Vec<u8>> {
    if input.chunk.len() > MAX_SNAPSHOT_CHUNK_BYTES {
        return Err(RaftError::frame_too_large(
            input.chunk.len(),
            MAX_SNAPSHOT_CHUNK_BYTES,
        ));
    }
    let mut builder = Builder::new_default();
    {
        let mut req = builder.init_root::<request::Builder>();
        req.set_contract_version(RAFT_CONTRACT_VERSION);
        req.set_kind(RpcKind::InstallSnapshot);
        let mut b = req.reborrow().init_install_snapshot();
        b.set_partition_id(input.partition_id);
        b.set_term(input.term);
        b.set_leader_id(input.leader_id);
        b.set_last_index(input.last_index);
        b.set_last_term(input.last_term);
        b.set_offset(input.offset);
        b.set_chunk(&input.chunk);
        b.set_done(input.done);
    }
    finish(&builder)
}

// ---------------------------------------------------------------------------
// Request decoding
// ---------------------------------------------------------------------------

fn reader_options() -> ReaderOptions {
    let mut options = ReaderOptions::new();
    options.traversal_limit_in_words(Some(MAX_TRAVERSAL_WORDS));
    options.nesting_limit(32);
    options
}

fn split_frame(frame: &[u8]) -> RaftResult<&[u8]> {
    if frame.len() < 4 {
        return Err(RaftError::protocol("frame shorter than length prefix"));
    }
    let len = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(RaftError::frame_too_large(len, MAX_FRAME_BYTES));
    }
    if frame.len() != 4 + len {
        return Err(RaftError::protocol(format!(
            "length prefix {len} does not match body {}",
            frame.len().saturating_sub(4)
        )));
    }
    Ok(&frame[4..])
}

fn decode_append_entries(reader: append_entries::Reader<'_>) -> RaftResult<AppendEntriesRequest> {
    let entries_reader = reader
        .get_entries()
        .map_err(|e| RaftError::protocol(e.to_string()))?;
    if entries_reader.len() as usize > MAX_ENTRIES {
        return Err(RaftError::new(
            ErrorCode::InvalidRequest,
            format!(
                "too many entries: {} > {}",
                entries_reader.len(),
                MAX_ENTRIES
            ),
        ));
    }
    let mut entries = Vec::with_capacity(entries_reader.len() as usize);
    for entry in entries_reader.iter() {
        let data = entry
            .get_data()
            .map_err(|e| RaftError::protocol(e.to_string()))?
            .to_vec();
        if data.len() > MAX_ENTRY_BYTES {
            return Err(RaftError::frame_too_large(data.len(), MAX_ENTRY_BYTES));
        }
        entries.push(LogEntry {
            term: entry.get_term(),
            index: entry.get_index(),
            data,
        });
    }
    Ok(AppendEntriesRequest {
        partition_id: reader.get_partition_id(),
        term: reader.get_term(),
        leader_id: reader.get_leader_id(),
        prev_log_index: reader.get_prev_log_index(),
        prev_log_term: reader.get_prev_log_term(),
        entries,
        leader_commit: reader.get_leader_commit(),
    })
}

fn decode_request_vote(reader: request_vote::Reader<'_>) -> RequestVoteRequest {
    RequestVoteRequest {
        partition_id: reader.get_partition_id(),
        term: reader.get_term(),
        candidate_id: reader.get_candidate_id(),
        last_log_index: reader.get_last_log_index(),
        last_log_term: reader.get_last_log_term(),
    }
}

fn decode_snapshot_chunk(
    reader: install_snapshot_chunk::Reader<'_>,
) -> RaftResult<InstallSnapshotChunk> {
    let chunk = reader
        .get_chunk()
        .map_err(|e| RaftError::protocol(e.to_string()))?
        .to_vec();
    if chunk.len() > MAX_SNAPSHOT_CHUNK_BYTES {
        return Err(RaftError::frame_too_large(
            chunk.len(),
            MAX_SNAPSHOT_CHUNK_BYTES,
        ));
    }
    Ok(InstallSnapshotChunk {
        partition_id: reader.get_partition_id(),
        term: reader.get_term(),
        leader_id: reader.get_leader_id(),
        last_index: reader.get_last_index(),
        last_term: reader.get_last_term(),
        offset: reader.get_offset(),
        chunk,
        done: reader.get_done(),
    })
}

pub fn decode_request(frame: &[u8]) -> RaftResult<RequestMessage> {
    let body = split_frame(frame)?;
    // Copy the body so the Cap'n Proto root is 8-byte aligned; the frame
    // prefix is four bytes, which would otherwise make the slice unaligned.
    let owned = body.to_vec();
    let mut slice = owned.as_slice();
    let message = serialize::read_message_from_flat_slice(&mut slice, reader_options())
        .map_err(|e| RaftError::protocol(format!("capnp decode failed: {e}")))?;
    let req = message
        .get_root::<request::Reader>()
        .map_err(|e| RaftError::protocol(format!("root decode failed: {e}")))?;

    if req.get_contract_version() != RAFT_CONTRACT_VERSION {
        return Err(RaftError::new(
            ErrorCode::InvalidRequest,
            format!(
                "unsupported contract version {}",
                req.get_contract_version()
            ),
        ));
    }
    let kind = req
        .get_kind()
        .map_err(|e| RaftError::protocol(format!("unknown RPC kind: {e}")))?;

    match req
        .which()
        .map_err(|e| RaftError::protocol(format!("unknown request union: {e}")))?
    {
        raft_capnp::request::Which::AppendEntries(reader) => {
            if kind != RpcKind::AppendEntries {
                return Err(RaftError::protocol("kind/payload mismatch"));
            }
            Ok(RequestMessage::AppendEntries(decode_append_entries(
                reader.map_err(|e| RaftError::protocol(e.to_string()))?,
            )?))
        }
        raft_capnp::request::Which::RequestVote(reader) => {
            if kind != RpcKind::RequestVote {
                return Err(RaftError::protocol("kind/payload mismatch"));
            }
            Ok(RequestMessage::RequestVote(decode_request_vote(
                reader.map_err(|e| RaftError::protocol(e.to_string()))?,
            )))
        }
        raft_capnp::request::Which::InstallSnapshot(reader) => {
            if kind != RpcKind::InstallSnapshot {
                return Err(RaftError::protocol("kind/payload mismatch"));
            }
            Ok(RequestMessage::InstallSnapshot(decode_snapshot_chunk(
                reader.map_err(|e| RaftError::protocol(e.to_string()))?,
            )?))
        }
    }
}

/// Best-effort kind read used to build an error response for a malformed
/// request.
pub fn peek_kind(frame: &[u8]) -> Option<RpcKind> {
    let body = split_frame(frame).ok()?;
    let owned = body.to_vec();
    let mut slice = owned.as_slice();
    let message = serialize::read_message_from_flat_slice(&mut slice, reader_options()).ok()?;
    let req = message.get_root::<request::Reader>().ok()?;
    req.get_kind().ok()
}

// ---------------------------------------------------------------------------
// Response encoding
// ---------------------------------------------------------------------------

fn write_error(resp: &mut response::Builder<'_>, err: &RaftError) {
    resp.set_code(wire_error_code(err.code));
    resp.set_retryable(err.retryable);
    resp.set_retry_after_ms(err.retry_after_ms.unwrap_or(0));
    let message: String = err.message.chars().take(512).collect();
    resp.set_message(message.as_str());
    if let Some(hint) = &err.leader_hint {
        let mut b = resp.reborrow().init_leader_hint();
        b.set_leader_id(hint.leader_id);
        let addr: String = hint.leader_addr.chars().take(MAX_ID_BYTES).collect();
        b.set_leader_addr(addr.as_str());
    }
}

fn wire_error_code(code: ErrorCode) -> WireErrorCode {
    match code {
        ErrorCode::Ok => WireErrorCode::Ok,
        ErrorCode::Unavailable => WireErrorCode::Unavailable,
        ErrorCode::NotLeader => WireErrorCode::NotLeader,
        ErrorCode::TermMismatch => WireErrorCode::TermMismatch,
        ErrorCode::SnapshotStale => WireErrorCode::SnapshotStale,
        ErrorCode::FrameTooLarge => WireErrorCode::FrameTooLarge,
        ErrorCode::InvalidRequest => WireErrorCode::InvalidRequest,
        ErrorCode::DeadlineExceeded => WireErrorCode::DeadlineExceeded,
        ErrorCode::Backpressure => WireErrorCode::Backpressure,
        ErrorCode::Internal => WireErrorCode::Internal,
        ErrorCode::Protocol => WireErrorCode::Protocol,
    }
}

pub fn encode_append_entries_response(input: &AppendEntriesResponse) -> RaftResult<Vec<u8>> {
    let mut builder = Builder::new_default();
    {
        let mut resp = builder.init_root::<response::Builder>();
        resp.set_contract_version(RAFT_CONTRACT_VERSION);
        resp.set_kind(RpcKind::AppendEntries);
        resp.set_code(WireErrorCode::Ok);
        let mut b = resp.reborrow().init_append_entries();
        b.set_term(input.term);
        b.set_success(input.success);
        b.set_match_index(input.match_index);
        b.set_conflict_index(input.conflict_index);
        b.set_conflict_term(input.conflict_term);
    }
    finish(&builder)
}

pub fn encode_request_vote_response(input: &RequestVoteResponse) -> RaftResult<Vec<u8>> {
    let mut builder = Builder::new_default();
    {
        let mut resp = builder.init_root::<response::Builder>();
        resp.set_contract_version(RAFT_CONTRACT_VERSION);
        resp.set_kind(RpcKind::RequestVote);
        resp.set_code(WireErrorCode::Ok);
        let mut b = resp.reborrow().init_request_vote();
        b.set_term(input.term);
        b.set_vote_granted(input.vote_granted);
    }
    finish(&builder)
}

pub fn encode_snapshot_ack(input: &InstallSnapshotAck) -> RaftResult<Vec<u8>> {
    let mut builder = Builder::new_default();
    {
        let mut resp = builder.init_root::<response::Builder>();
        resp.set_contract_version(RAFT_CONTRACT_VERSION);
        resp.set_kind(RpcKind::InstallSnapshot);
        resp.set_code(WireErrorCode::Ok);
        let mut b = resp.reborrow().init_install_snapshot();
        b.set_term(input.term);
        b.set_success(input.success);
        b.set_bytes_accepted(input.bytes_accepted);
        b.set_next_offset(input.next_offset);
        b.set_done(input.done);
    }
    finish(&builder)
}

pub fn encode_error_response(kind: RpcKind, err: &RaftError) -> RaftResult<Vec<u8>> {
    let mut builder = Builder::new_default();
    {
        let mut resp = builder.init_root::<response::Builder>();
        resp.set_contract_version(RAFT_CONTRACT_VERSION);
        resp.set_kind(kind);
        write_error(&mut resp, err);
    }
    finish(&builder)
}

// ---------------------------------------------------------------------------
// Response decoding
// ---------------------------------------------------------------------------

fn decode_leader_hint(reader: leader_hint::Reader<'_>) -> LeaderHint {
    LeaderHint {
        leader_id: reader.get_leader_id(),
        leader_addr: reader
            .get_leader_addr()
            .map(|v| v.to_string().unwrap_or_default())
            .unwrap_or_default(),
    }
}

pub fn decode_response(frame: &[u8]) -> RaftResult<ResponseMessage> {
    let body = split_frame(frame)?;
    // Copy the body so the Cap'n Proto root is 8-byte aligned; the frame
    // prefix is four bytes, which would otherwise make the slice unaligned.
    let owned = body.to_vec();
    let mut slice = owned.as_slice();
    let message = serialize::read_message_from_flat_slice(&mut slice, reader_options())
        .map_err(|e| RaftError::protocol(format!("capnp decode failed: {e}")))?;
    let resp = message
        .get_root::<response::Reader>()
        .map_err(|e| RaftError::protocol(format!("root decode failed: {e}")))?;

    if resp.get_contract_version() != RAFT_CONTRACT_VERSION {
        return Err(RaftError::new(
            ErrorCode::InvalidRequest,
            format!(
                "unsupported contract version {}",
                resp.get_contract_version()
            ),
        ));
    }

    let code = resp
        .get_code()
        .map_err(|e| RaftError::protocol(format!("unknown error code: {e}")))?;
    if code != WireErrorCode::Ok {
        let hint = resp
            .get_leader_hint()
            .ok()
            .map(decode_leader_hint)
            .filter(LeaderHint::is_complete);
        let retry_after_ms = match resp.get_retry_after_ms() {
            0 => None,
            ms => Some(ms),
        };
        return Err(RaftError {
            code: ErrorCode::from_u16(code as u16),
            message: resp
                .get_message()
                .map(|m| m.to_string().unwrap_or_default())
                .unwrap_or_default(),
            leader_hint: hint,
            retryable: resp.get_retryable(),
            retry_after_ms,
        });
    }

    match resp
        .which()
        .map_err(|e| RaftError::protocol(format!("unknown response union: {e}")))?
    {
        raft_capnp::response::Which::AppendEntries(reader) => {
            let b = reader.map_err(|e| RaftError::protocol(e.to_string()))?;
            Ok(ResponseMessage::AppendEntries(AppendEntriesResponse {
                term: b.get_term(),
                success: b.get_success(),
                match_index: b.get_match_index(),
                conflict_index: b.get_conflict_index(),
                conflict_term: b.get_conflict_term(),
            }))
        }
        raft_capnp::response::Which::RequestVote(reader) => {
            let b = reader.map_err(|e| RaftError::protocol(e.to_string()))?;
            Ok(ResponseMessage::RequestVote(RequestVoteResponse {
                term: b.get_term(),
                vote_granted: b.get_vote_granted(),
            }))
        }
        raft_capnp::response::Which::InstallSnapshot(reader) => {
            let b = reader.map_err(|e| RaftError::protocol(e.to_string()))?;
            Ok(ResponseMessage::InstallSnapshot(InstallSnapshotAck {
                term: b.get_term(),
                success: b.get_success(),
                bytes_accepted: b.get_bytes_accepted(),
                next_offset: b.get_next_offset(),
                done: b.get_done(),
            }))
        }
    }
}

// Suppress dead-code warnings for generated modules that are only used via
// the helpers above.
#[allow(dead_code)]
fn _keep_generated_modules(
    _a: log_entry::Reader<'_>,
    _b: append_entries_response::Reader<'_>,
    _c: request_vote_response::Reader<'_>,
    _d: install_snapshot_ack::Reader<'_>,
) {
}
