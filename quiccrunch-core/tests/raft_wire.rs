use quiccrunch_core::raft::{
    decode_request, decode_response, encode_append_entries, encode_append_entries_response,
    encode_request_vote, encode_snapshot_chunk, AppendEntriesRequest, AppendEntriesResponse,
    ErrorCode, InstallSnapshotChunk, LogEntry, RaftError, RequestMessage, ResponseMessage,
    MAX_ENTRIES, MAX_ENTRY_BYTES, MAX_FRAME_BYTES, MAX_SNAPSHOT_CHUNK_BYTES, RAFT_CONTRACT_VERSION,
};

fn entry(index: u64, data: &[u8]) -> LogEntry {
    LogEntry {
        term: 3,
        index,
        data: data.to_vec(),
    }
}

#[test]
fn append_entries_round_trip_preserves_every_field() {
    let request = AppendEntriesRequest {
        partition_id: 7,
        term: 42,
        leader_id: 1,
        prev_log_index: 9,
        prev_log_term: 4,
        entries: vec![entry(10, b"alpha"), entry(11, b"")],
        leader_commit: 8,
    };
    let frame = encode_append_entries(&request).unwrap();
    assert_eq!(
        u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize,
        frame.len() - 4
    );
    match decode_request(&frame).unwrap() {
        RequestMessage::AppendEntries(decoded) => assert_eq!(decoded, request),
        other => panic!("unexpected request: {other:?}"),
    }

    let response = AppendEntriesResponse {
        term: 42,
        success: false,
        match_index: 0,
        conflict_index: 8,
        conflict_term: 2,
    };
    let frame = encode_append_entries_response(&response).unwrap();
    match decode_response(&frame).unwrap() {
        ResponseMessage::AppendEntries(decoded) => assert_eq!(decoded, response),
        other => panic!("unexpected response: {other:?}"),
    }
}

#[test]
fn request_vote_and_snapshot_chunk_round_trip() {
    let vote = quiccrunch_core::raft::RequestVoteRequest {
        partition_id: 2,
        term: 9,
        candidate_id: 3,
        last_log_index: 100,
        last_log_term: 8,
    };
    let frame = encode_request_vote(&vote).unwrap();
    match decode_request(&frame).unwrap() {
        RequestMessage::RequestVote(decoded) => assert_eq!(decoded, vote),
        other => panic!("unexpected request: {other:?}"),
    }

    let chunk = InstallSnapshotChunk {
        partition_id: 2,
        term: 9,
        leader_id: 3,
        last_index: 100,
        last_term: 8,
        offset: 4096,
        chunk: vec![7u8; 4096],
        done: false,
    };
    let frame = encode_snapshot_chunk(&chunk).unwrap();
    match decode_request(&frame).unwrap() {
        RequestMessage::InstallSnapshot(decoded) => assert_eq!(decoded, chunk),
        other => panic!("unexpected request: {other:?}"),
    }
}

#[test]
fn oversized_frames_are_rejected_before_writing() {
    let mut request = AppendEntriesRequest {
        entries: vec![entry(1, b"ok")],
        ..Default::default()
    };
    request.entries[0].data = vec![0u8; MAX_ENTRY_BYTES + 1];
    let err = encode_append_entries(&request).unwrap_err();
    assert_eq!(err.code, ErrorCode::FrameTooLarge);

    let chunk = InstallSnapshotChunk {
        chunk: vec![0u8; MAX_SNAPSHOT_CHUNK_BYTES + 1],
        ..Default::default()
    };
    let err = encode_snapshot_chunk(&chunk).unwrap_err();
    assert_eq!(err.code, ErrorCode::FrameTooLarge);

    let mut frame = (MAX_FRAME_BYTES as u32 + 1).to_be_bytes().to_vec();
    frame.extend_from_slice(&[0u8; 8]);
    let err = decode_request(&frame).unwrap_err();
    assert_eq!(err.code, ErrorCode::FrameTooLarge);

    let too_many = AppendEntriesRequest {
        entries: (0..=MAX_ENTRIES as u64).map(|i| entry(i, b"x")).collect(),
        ..AppendEntriesRequest::default()
    };
    let err = encode_append_entries(&too_many).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
}

#[test]
fn unknown_contract_version_is_rejected() {
    use capnp::message::Builder;
    use quiccrunch_core::schema::raft_capnp::{request, RpcKind};

    let mut builder = Builder::new_default();
    let mut req = builder.init_root::<request::Builder>();
    req.set_contract_version(99);
    req.set_kind(RpcKind::RequestVote);
    let _ = req.reborrow().init_request_vote();
    let body = capnp::serialize::write_message_to_words(&builder);
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);

    let err = decode_request(&frame).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
}

#[test]
fn explicit_error_codes_and_leader_hint_round_trip() {
    let hint = quiccrunch_core::raft::LeaderHint::new(9, "127.0.0.1:9000");
    let err = RaftError::not_leader(hint.clone());
    let frame = quiccrunch_core::raft::encode_error_response(
        quiccrunch_core::schema::raft_capnp::RpcKind::AppendEntries,
        &err,
    )
    .unwrap();
    let decoded = decode_response(&frame).unwrap_err();
    assert_eq!(decoded.code, ErrorCode::NotLeader);
    assert_eq!(decoded.leader_hint, Some(hint));

    let stale = RaftError::snapshot_stale("offset moved");
    let frame = quiccrunch_core::raft::encode_error_response(
        quiccrunch_core::schema::raft_capnp::RpcKind::InstallSnapshot,
        &stale,
    )
    .unwrap();
    assert_eq!(
        decode_response(&frame).unwrap_err().code,
        ErrorCode::SnapshotStale
    );
}

#[test]
fn contract_version_constant_is_stable() {
    assert_eq!(RAFT_CONTRACT_VERSION, 1);
}
