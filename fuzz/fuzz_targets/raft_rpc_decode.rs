#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use quiccrunch_core::raft::{
    self, AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotAck, InstallSnapshotChunk,
    LogEntry, RaftResult, RequestMessage, RequestVoteRequest, RequestVoteResponse, ResponseMessage,
    MAX_ENTRIES, MAX_ENTRY_BYTES, MAX_FRAME_BYTES, MAX_SNAPSHOT_CHUNK_BYTES,
};

#[derive(Arbitrary, Debug)]
struct FuzzRaft {
    kind: u8,
    a: u64,
    b: u64,
    data: Vec<u8>,
    entries: Vec<(u64, u64, Vec<u8>)>,
}

fn entries(input: &FuzzRaft) -> Vec<LogEntry> {
    input
        .entries
        .iter()
        .take(MAX_ENTRIES)
        .map(|(term, index, data)| LogEntry {
            term: *term,
            index: *index,
            data: data.clone(),
        })
        .collect()
}

fn build_request(input: &FuzzRaft) -> RequestMessage {
    match input.kind % 3 {
        0 => RequestMessage::AppendEntries(AppendEntriesRequest {
            partition_id: input.a,
            term: input.b,
            leader_id: input.a,
            prev_log_index: input.b,
            prev_log_term: input.a,
            entries: entries(input),
            leader_commit: input.b,
        }),
        1 => RequestMessage::RequestVote(RequestVoteRequest {
            partition_id: input.a,
            term: input.b,
            candidate_id: input.a,
            last_log_index: input.b,
            last_log_term: input.a,
        }),
        _ => RequestMessage::InstallSnapshot(InstallSnapshotChunk {
            partition_id: input.a,
            term: input.b,
            leader_id: input.a,
            last_index: input.b,
            last_term: input.a,
            offset: input.b,
            chunk: input.data.clone(),
            done: input.a % 2 == 0,
        }),
    }
}

fn build_response(input: &FuzzRaft) -> ResponseMessage {
    match input.kind % 3 {
        0 => ResponseMessage::AppendEntries(AppendEntriesResponse {
            term: input.a,
            success: input.b % 2 == 0,
            match_index: input.a,
            conflict_index: input.b,
            conflict_term: input.a,
        }),
        1 => ResponseMessage::RequestVote(RequestVoteResponse {
            term: input.a,
            vote_granted: input.b % 2 == 0,
        }),
        _ => ResponseMessage::InstallSnapshot(InstallSnapshotAck {
            term: input.a,
            success: input.b % 2 == 0,
            bytes_accepted: input.a,
            next_offset: input.b,
            done: input.a % 2 == 0,
        }),
    }
}

fn encode_request(request: &RequestMessage) -> RaftResult<Vec<u8>> {
    match request {
        RequestMessage::AppendEntries(request) => raft::encode_append_entries(request),
        RequestMessage::RequestVote(request) => raft::encode_request_vote(request),
        RequestMessage::InstallSnapshot(request) => raft::encode_snapshot_chunk(request),
    }
}

fn encode_response(response: &ResponseMessage) -> RaftResult<Vec<u8>> {
    match response {
        ResponseMessage::AppendEntries(response) => raft::encode_append_entries_response(response),
        ResponseMessage::RequestVote(response) => raft::encode_request_vote_response(response),
        ResponseMessage::InstallSnapshot(response) => raft::encode_snapshot_ack(response),
    }
}

fn check_request(request: &RequestMessage) {
    match request {
        RequestMessage::AppendEntries(request) => {
            assert!(request.entries.len() <= MAX_ENTRIES);
            for entry in &request.entries {
                assert!(entry.data.len() <= MAX_ENTRY_BYTES);
            }
        }
        RequestMessage::InstallSnapshot(request) => {
            assert!(request.chunk.len() <= MAX_SNAPSHOT_CHUNK_BYTES);
        }
        RequestMessage::RequestVote(_) => {}
    }
}

fuzz_target!(|data: &[u8]| {
    // Arbitrary bytes through the Raft request/response decoders.
    if let Ok(request) = raft::decode_request(data) {
        assert!(data.len() <= MAX_FRAME_BYTES + 4);
        check_request(&request);
        let reencoded = encode_request(&request).expect("decoded request must re-encode");
        assert!(reencoded.len() <= MAX_FRAME_BYTES + 4);
        let _ = raft::decode_request(&reencoded).expect("re-encoded request must decode");
    }
    if let Ok(response) = raft::decode_response(data) {
        assert!(data.len() <= MAX_FRAME_BYTES + 4);
        let reencoded = encode_response(&response).expect("decoded response must re-encode");
        assert!(reencoded.len() <= MAX_FRAME_BYTES + 4);
        let _ = raft::decode_response(&reencoded).expect("re-encoded response must decode");
    }

    // Structured input: build and round-trip all three RPCs in both directions.
    let mut unstructured = Unstructured::new(data);
    if let Ok(input) = FuzzRaft::arbitrary(&mut unstructured) {
        let request = build_request(&input);
        if let Ok(frame) = encode_request(&request) {
            assert!(frame.len() <= MAX_FRAME_BYTES + 4);
            let decoded = raft::decode_request(&frame).expect("encoded request must decode");
            assert_eq!(decoded, request);
        }

        let response = build_response(&input);
        if let Ok(frame) = encode_response(&response) {
            assert!(frame.len() <= MAX_FRAME_BYTES + 4);
            let decoded = raft::decode_response(&frame).expect("encoded response must decode");
            assert_eq!(decoded, response);
        }
    }
});
