//! Stable-CI property tests for the wire decoders.
//!
//! These mirror the nightly `cargo-fuzz` targets in `fuzz/` so every pull
//! request gets decoder coverage without a nightly toolchain. The invariant is
//! the same: no panic, no out-of-bounds read, no unbounded allocation, and
//! either a clean `Err` or a valid message that respects the documented bounds.

use std::collections::HashMap;

use proptest::prelude::*;
use quiccrunch_core::frame_header::FrameHeader;
use quiccrunch_core::messaging::{
    self, Body, Event, EventIngest, EventIngested, Heartbeat, HeartbeatAck, LeaseAcquire,
    LeaseAcquired, LeaseRelease, LeaseReleased, Message, PeerIdentity, PolicyBundle, PolicyGet,
    PolicyWatch, PolicyWatchEventDto, RequestPayload, ResponsePayload, CONTRACT_VERSION,
    MAX_BUNDLE_BYTES, MAX_ENVELOPE_BYTES, MAX_EVENTS_PER_INGEST, MAX_METADATA_ENTRIES,
};
use quiccrunch_core::raft::{
    self, AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotAck, InstallSnapshotChunk,
    LogEntry, RequestMessage, RequestVoteRequest, RequestVoteResponse, ResponseMessage,
    MAX_ENTRIES, MAX_ENTRY_BYTES, MAX_FRAME_BYTES, MAX_SNAPSHOT_CHUNK_BYTES,
};

fn text() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9_./:-]{0,32}").unwrap()
}

fn blob() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..256)
}

fn metadata() -> impl Strategy<Value = HashMap<String, String>> {
    prop::collection::hash_map(text(), text(), 0..4)
}

fn identity() -> impl Strategy<Value = PeerIdentity> {
    (text(), text(), prop::collection::vec(text(), 0..4)).prop_map(|(spiffe_id, node_id, roles)| {
        PeerIdentity {
            spiffe_id,
            node_id,
            roles,
        }
    })
}

fn event() -> impl Strategy<Value = Event> {
    (text(), text(), any::<u64>(), blob()).prop_map(
        |(event_id, kind, occurred_at_unix_ms, payload)| Event {
            event_id,
            kind,
            occurred_at_unix_ms,
            payload,
        },
    )
}

fn request_payload() -> impl Strategy<Value = RequestPayload> {
    prop_oneof![
        (text(), text(), any::<u32>()).prop_map(|(lease_id, resource, ttl_ms)| {
            RequestPayload::LeaseAcquire(LeaseAcquire {
                lease_id,
                resource,
                ttl_ms,
            })
        }),
        (text(), any::<u64>()).prop_map(|(lease_id, fencing_token)| {
            RequestPayload::LeaseRelease(LeaseRelease {
                lease_id,
                fencing_token,
            })
        }),
        (text(), any::<u64>()).prop_map(|(policy_id, known_version)| {
            RequestPayload::PolicyGet(PolicyGet {
                policy_id,
                known_version,
            })
        }),
        (text(), any::<u64>()).prop_map(|(policy_id, since_version)| {
            RequestPayload::PolicyWatch(PolicyWatch {
                policy_id,
                since_version,
            })
        }),
        prop::collection::vec(event(), 0..8)
            .prop_map(|events| RequestPayload::EventIngest(EventIngest { events })),
        (text(), any::<u64>()).prop_map(|(node_id, epoch)| {
            RequestPayload::Heartbeat(Heartbeat { node_id, epoch })
        }),
    ]
}

fn response_payload() -> impl Strategy<Value = ResponsePayload> {
    prop_oneof![
        (any::<bool>(), any::<u64>(), any::<u64>(), text()).prop_map(
            |(granted, fencing_token, expires_at_unix_ms, holder)| {
                ResponsePayload::LeaseAcquire(LeaseAcquired {
                    granted,
                    fencing_token,
                    expires_at_unix_ms,
                    holder,
                })
            }
        ),
        any::<bool>()
            .prop_map(|released| { ResponsePayload::LeaseRelease(LeaseReleased { released }) }),
        (text(), any::<u64>(), blob(), text()).prop_map(
            |(policy_id, version, bundle, checksum)| {
                ResponsePayload::PolicyGet(PolicyBundle {
                    policy_id,
                    version,
                    bundle,
                    checksum,
                })
            }
        ),
        (any::<u32>(), any::<u32>()).prop_map(|(accepted, duplicate)| {
            ResponsePayload::EventIngest(EventIngested {
                accepted,
                duplicate,
            })
        }),
        (any::<u64>(), any::<u64>()).prop_map(|(server_time_unix_ms, epoch)| {
            ResponsePayload::Heartbeat(HeartbeatAck {
                server_time_unix_ms,
                epoch,
            })
        }),
    ]
}

fn body() -> impl Strategy<Value = Body> {
    prop_oneof![
        (identity(), metadata(), request_payload()).prop_map(|(identity, metadata, payload)| {
            Body::Request {
                identity,
                metadata,
                payload,
            }
        }),
        response_payload().prop_map(Body::Response),
        (text(), any::<u64>(), blob(), text()).prop_map(
            |(policy_id, version, bundle, checksum)| {
                Body::WatchEvent(PolicyWatchEventDto {
                    policy_id,
                    version,
                    bundle,
                    checksum,
                })
            }
        ),
        text().prop_map(Body::StreamEnd),
        (
            any::<u16>(),
            text(),
            any::<u32>(),
            any::<u64>(),
            any::<u64>(),
            any::<bool>(),
        )
            .prop_map(
                |(code, message, retry_after_ms, earliest_available, server_latest, retryable)| {
                    Body::Error {
                        code,
                        message,
                        retry_after_ms,
                        earliest_available,
                        server_latest,
                        retryable,
                    }
                }
            ),
    ]
}

fn message() -> impl Strategy<Value = Message> {
    (any::<u64>(), any::<u64>(), any::<u64>(), body()).prop_map(
        |(message_id, request_id, timestamp_ms, body)| Message {
            contract_version: CONTRACT_VERSION,
            message_id,
            request_id,
            timestamp_ms,
            body,
        },
    )
}

fn check_messaging_bounds(message: &Message) {
    if let Body::Request {
        metadata, payload, ..
    } = &message.body
    {
        assert!(metadata.len() <= MAX_METADATA_ENTRIES);
        if let RequestPayload::EventIngest(ingest) = payload {
            assert!(ingest.events.len() <= MAX_EVENTS_PER_INGEST);
        }
    }
    match &message.body {
        Body::Response(ResponsePayload::PolicyGet(bundle)) => {
            assert!(bundle.bundle.len() <= MAX_BUNDLE_BYTES);
        }
        Body::WatchEvent(event) => assert!(event.bundle.len() <= MAX_BUNDLE_BYTES),
        _ => {}
    }
}

fn raft_entry() -> impl Strategy<Value = LogEntry> {
    (any::<u64>(), any::<u64>(), blob()).prop_map(|(term, index, data)| LogEntry {
        term,
        index,
        data,
    })
}

fn raft_request() -> impl Strategy<Value = RequestMessage> {
    prop_oneof![
        (
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            prop::collection::vec(raft_entry(), 0..8),
            any::<u64>(),
        )
            .prop_map(
                |(
                    partition_id,
                    term,
                    leader_id,
                    prev_log_index,
                    prev_log_term,
                    entries,
                    leader_commit,
                )| {
                    RequestMessage::AppendEntries(AppendEntriesRequest {
                        partition_id,
                        term,
                        leader_id,
                        prev_log_index,
                        prev_log_term,
                        entries,
                        leader_commit,
                    })
                }
            ),
        (
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(
                |(partition_id, term, candidate_id, last_log_index, last_log_term)| {
                    RequestMessage::RequestVote(RequestVoteRequest {
                        partition_id,
                        term,
                        candidate_id,
                        last_log_index,
                        last_log_term,
                    })
                }
            ),
        (
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            blob(),
            any::<bool>(),
        )
            .prop_map(
                |(partition_id, term, leader_id, last_index, last_term, offset, chunk, done)| {
                    RequestMessage::InstallSnapshot(InstallSnapshotChunk {
                        partition_id,
                        term,
                        leader_id,
                        last_index,
                        last_term,
                        offset,
                        chunk,
                        done,
                    })
                }
            ),
    ]
}

fn raft_response() -> impl Strategy<Value = ResponseMessage> {
    prop_oneof![
        (
            any::<u64>(),
            any::<bool>(),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
        )
            .prop_map(
                |(term, success, match_index, conflict_index, conflict_term)| {
                    ResponseMessage::AppendEntries(AppendEntriesResponse {
                        term,
                        success,
                        match_index,
                        conflict_index,
                        conflict_term,
                    })
                }
            ),
        (any::<u64>(), any::<bool>()).prop_map(|(term, vote_granted)| {
            ResponseMessage::RequestVote(RequestVoteResponse { term, vote_granted })
        }),
        (
            any::<u64>(),
            any::<bool>(),
            any::<u64>(),
            any::<u64>(),
            any::<bool>(),
        )
            .prop_map(|(term, success, bytes_accepted, next_offset, done)| {
                ResponseMessage::InstallSnapshot(InstallSnapshotAck {
                    term,
                    success,
                    bytes_accepted,
                    next_offset,
                    done,
                })
            }),
    ]
}

fn encode_raft_request(request: &RequestMessage) -> raft::RaftResult<Vec<u8>> {
    match request {
        RequestMessage::AppendEntries(request) => raft::encode_append_entries(request),
        RequestMessage::RequestVote(request) => raft::encode_request_vote(request),
        RequestMessage::InstallSnapshot(request) => raft::encode_snapshot_chunk(request),
    }
}

fn encode_raft_response(response: &ResponseMessage) -> raft::RaftResult<Vec<u8>> {
    match response {
        ResponseMessage::AppendEntries(response) => raft::encode_append_entries_response(response),
        ResponseMessage::RequestVote(response) => raft::encode_request_vote_response(response),
        ResponseMessage::InstallSnapshot(response) => raft::encode_snapshot_ack(response),
    }
}

fn check_raft_request_bounds(request: &RequestMessage) {
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

proptest! {
    #[test]
    fn frame_header_round_trips(length in any::<u32>()) {
        let header = FrameHeader::new(length);
        let encoded = header.encode();
        let decoded = FrameHeader::decode(&encoded).expect("encoded header must decode");
        prop_assert_eq!(decoded.length, length);
    }

    #[test]
    fn frame_header_decode_arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..128)) {
        if let Some(header) = FrameHeader::decode(&data) {
            let encoded = header.encode();
            let decoded = FrameHeader::decode(&encoded).expect("encoded header must decode");
            prop_assert_eq!(decoded.length, header.length);
        }
    }

    #[test]
    fn messaging_round_trips_every_message_type(message in message()) {
        let frame = messaging::encode(&message).expect("valid message must encode");
        prop_assert!(frame.len() <= MAX_ENVELOPE_BYTES + 4);
        let decoded = messaging::decode(&frame).expect("encoded message must decode");
        prop_assert_eq!(decoded, message);
    }

    #[test]
    fn messaging_decode_arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..8192)) {
        if let Ok(message) = messaging::decode(&data) {
            prop_assert!(data.len() <= MAX_ENVELOPE_BYTES + 4);
            check_messaging_bounds(&message);
            let reencoded = messaging::encode(&message).expect("decoded message must re-encode");
            prop_assert!(reencoded.len() <= MAX_ENVELOPE_BYTES + 4);
            prop_assert!(messaging::decode(&reencoded).is_ok());
        }
    }

    #[test]
    fn messaging_rejects_oversized_and_truncated_frames(
        prefix in any::<u32>(),
        body_len in 0usize..64,
    ) {
        let mut frame = prefix.to_be_bytes().to_vec();
        frame.resize(4 + body_len, 0);
        let result = messaging::decode(&frame);
        if prefix as usize > MAX_ENVELOPE_BYTES {
            let too_large = matches!(
                result,
                Err(messaging::MessagingError::FrameTooLarge { .. })
            );
            prop_assert!(too_large);
        } else if frame.len() != 4 + prefix as usize {
            prop_assert!(result.is_err());
        }
    }

    #[test]
    fn raft_round_trips_every_rpc(request in raft_request(), response in raft_response()) {
        let frame = encode_raft_request(&request).expect("valid request must encode");
        prop_assert!(frame.len() <= MAX_FRAME_BYTES + 4);
        let decoded = raft::decode_request(&frame).expect("encoded request must decode");
        prop_assert_eq!(decoded, request);

        let frame = encode_raft_response(&response).expect("valid response must encode");
        prop_assert!(frame.len() <= MAX_FRAME_BYTES + 4);
        let decoded = raft::decode_response(&frame).expect("encoded response must decode");
        prop_assert_eq!(decoded, response);
    }

    #[test]
    fn raft_decode_arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..8192)) {
        if let Ok(request) = raft::decode_request(&data) {
            prop_assert!(data.len() <= MAX_FRAME_BYTES + 4);
            check_raft_request_bounds(&request);
            let reencoded = encode_raft_request(&request).expect("decoded request must re-encode");
            prop_assert!(reencoded.len() <= MAX_FRAME_BYTES + 4);
            prop_assert!(raft::decode_request(&reencoded).is_ok());
        }
        if let Ok(response) = raft::decode_response(&data) {
            prop_assert!(data.len() <= MAX_FRAME_BYTES + 4);
            let reencoded = encode_raft_response(&response).expect("decoded response must re-encode");
            prop_assert!(reencoded.len() <= MAX_FRAME_BYTES + 4);
            prop_assert!(raft::decode_response(&reencoded).is_ok());
        }
    }

    #[test]
    fn raft_rejects_oversized_and_truncated_frames(
        prefix in any::<u32>(),
        body_len in 0usize..64,
    ) {
        let mut frame = prefix.to_be_bytes().to_vec();
        frame.resize(4 + body_len, 0);
        let result = raft::decode_request(&frame);
        if prefix as usize > MAX_FRAME_BYTES {
            let too_large = matches!(
                result,
                Err(ref error) if error.code == raft::ErrorCode::FrameTooLarge
            );
            prop_assert!(too_large);
        } else if frame.len() != 4 + prefix as usize {
            prop_assert!(result.is_err());
        }
    }
}
