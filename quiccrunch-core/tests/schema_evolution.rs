//! Schema-evolution compatibility tests for the messaging and Raft contracts.
//!
//! Policy implemented by the schemas (Cap'n Proto):
//! - Unknown fields are **safely ignored** by an older decoder. Cap'n Proto
//!   readers only follow the ordinals they know; a new optional field is
//!   invisible to a v1 reader.
//! - A new **required** field is a breaking change and must bump
//!   `contractVersion`. The v1 decoder rejects the new version before reading
//!   any payload, so it can never silently substitute a default.
//! - `contractVersion` is the major version. Additive minor changes do not
//!   change it; breaking changes increment it.

use capnp::message::Builder;
use capnp::serialize;

use quiccrunch_core::messaging::{
    self, Body, MessagingError, RequestPayload, CONTRACT_VERSION, MAX_ENVELOPE_BYTES,
};
use quiccrunch_core::raft::{self, ErrorCode, RequestMessage, RAFT_CONTRACT_VERSION};
use quiccrunch_core::schema::messaging_capnp::{envelope, Method};
use quiccrunch_core::schema::raft_capnp::{
    request as raft_request, response as raft_response, ErrorCode as WireErrorCode, RpcKind,
};

fn finish_messaging(builder: &Builder<capnp::message::HeapAllocator>) -> Vec<u8> {
    let mut body = serialize::write_message_to_words(builder);
    let padding = (8 - (body.len() % 8)) % 8;
    body.resize(body.len() + padding, 0);
    assert!(body.len() <= MAX_ENVELOPE_BYTES);
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

fn finish_raft(builder: &Builder<capnp::message::HeapAllocator>) -> Vec<u8> {
    let body = serialize::write_message_to_words(builder);
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(&body);
    frame
}

/// Build a v1-shaped heartbeat envelope that also carries the optional
/// `traceId` field added after v1.
fn messaging_envelope_with_trace(version: u16, trace_id: &str) -> Vec<u8> {
    let mut builder = Builder::new_default();
    {
        let mut env = builder.init_root::<envelope::Builder>();
        env.set_contract_version(version);
        env.set_message_id(7);
        env.set_request_id(9);
        env.set_timestamp_ms(11);
        env.set_method(Method::Heartbeat);
        env.set_trace_id(trace_id);
        let mut request = env.reborrow().init_request();
        let mut heartbeat = request.reborrow().init_heartbeat();
        heartbeat.set_node_id("node-a");
        heartbeat.set_epoch(42);
    }
    finish_messaging(&builder)
}

fn raft_request_with_trace(version: u16, trace_id: &str) -> Vec<u8> {
    let mut builder = Builder::new_default();
    {
        let mut request = builder.init_root::<raft_request::Builder>();
        request.set_contract_version(version);
        request.set_kind(RpcKind::RequestVote);
        request.set_trace_id(trace_id);
        let mut vote = request.reborrow().init_request_vote();
        vote.set_partition_id(3);
        vote.set_term(9);
        vote.set_candidate_id(4);
        vote.set_last_log_index(100);
        vote.set_last_log_term(8);
    }
    finish_raft(&builder)
}

fn raft_response_with_version(version: u16) -> Vec<u8> {
    let mut builder = Builder::new_default();
    {
        let mut response = builder.init_root::<raft_response::Builder>();
        response.set_contract_version(version);
        response.set_kind(RpcKind::RequestVote);
        response.set_code(WireErrorCode::Ok);
        let mut vote = response.reborrow().init_request_vote();
        vote.set_term(9);
        vote.set_vote_granted(true);
    }
    finish_raft(&builder)
}

#[test]
fn v1_messaging_decoder_ignores_new_optional_field() {
    let frame = messaging_envelope_with_trace(CONTRACT_VERSION, "trace-abc");
    let message = messaging::decode(&frame)
        .expect("v1 decoder must ignore an unknown optional field, not reject it");

    assert_eq!(message.contract_version, CONTRACT_VERSION);
    match message.body {
        Body::Request { payload, .. } => match payload {
            RequestPayload::Heartbeat(heartbeat) => {
                assert_eq!(heartbeat.node_id, "node-a");
                assert_eq!(heartbeat.epoch, 42);
            }
            other => panic!("unexpected payload: {other:?}"),
        },
        other => panic!("unexpected body: {other:?}"),
    }
}

#[test]
fn v1_messaging_decoder_rejects_v2_required_field_with_both_versions() {
    // A v2 message that adds a required field must bump contractVersion. The
    // v1 decoder rejects it before reading the payload, so the new field can
    // never be silently defaulted.
    let frame = messaging_envelope_with_trace(2, "required-in-v2");
    let error = messaging::decode(&frame).expect_err("v1 decoder must reject contract v2");
    let text = error.to_string();

    assert!(matches!(error, MessagingError::UnsupportedVersion(2)));
    assert!(
        text.contains('2'),
        "error must name the received version: {text}"
    );
    assert!(
        text.contains(&CONTRACT_VERSION.to_string()),
        "error must name the supported version: {text}"
    );
}

#[test]
fn messaging_encoder_rejects_mismatched_major_version() {
    let mut message = messaging::Message::request(
        1,
        1,
        Body::Response(quiccrunch_core::messaging::ResponsePayload::Heartbeat(
            quiccrunch_core::messaging::HeartbeatAck {
                server_time_unix_ms: 1,
                epoch: 1,
            },
        )),
    );
    message.contract_version = 99;
    let error = messaging::encode(&message).expect_err("encoder must reject contract v99");
    let text = error.to_string();
    assert!(matches!(error, MessagingError::UnsupportedVersion(99)));
    assert!(text.contains("99"));
    assert!(text.contains(&CONTRACT_VERSION.to_string()));
}

#[test]
fn v1_raft_decoder_ignores_new_optional_field() {
    let frame = raft_request_with_trace(RAFT_CONTRACT_VERSION, "trace-abc");
    let request = raft::decode_request(&frame)
        .expect("v1 decoder must ignore an unknown optional field, not reject it");

    match request {
        RequestMessage::RequestVote(vote) => {
            assert_eq!(vote.partition_id, 3);
            assert_eq!(vote.term, 9);
            assert_eq!(vote.candidate_id, 4);
            assert_eq!(vote.last_log_index, 100);
            assert_eq!(vote.last_log_term, 8);
        }
        other => panic!("unexpected request: {other:?}"),
    }
}

#[test]
fn v1_raft_decoder_rejects_v2_required_field_with_both_versions() {
    let frame = raft_request_with_trace(2, "required-in-v2");
    let error = raft::decode_request(&frame).expect_err("v1 decoder must reject contract v2");
    let text = error.to_string();

    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(
        text.contains('2'),
        "error must name the received version: {text}"
    );
    assert!(
        text.contains(&RAFT_CONTRACT_VERSION.to_string()),
        "error must name the supported version: {text}"
    );
}

#[test]
fn v1_raft_response_decoder_rejects_mismatched_major_version() {
    let frame = raft_response_with_version(2);
    let error = raft::decode_response(&frame).expect_err("v1 decoder must reject contract v2");
    let text = error.to_string();

    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert!(text.contains('2'));
    assert!(text.contains(&RAFT_CONTRACT_VERSION.to_string()));
}
