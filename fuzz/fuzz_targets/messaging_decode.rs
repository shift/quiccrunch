#![no_main]

use std::collections::HashMap;

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use quiccrunch_core::messaging::{
    self, Body, Event, EventIngest, Heartbeat, HeartbeatAck, LeaseAcquire, LeaseAcquired,
    LeaseRelease, LeaseReleased, Message, PeerIdentity, PolicyBundle, PolicyGet, PolicyWatch,
    PolicyWatchEventDto, RequestPayload, ResponsePayload, CONTRACT_VERSION, MAX_BUNDLE_BYTES,
    MAX_ENVELOPE_BYTES, MAX_EVENTS_PER_INGEST, MAX_METADATA_ENTRIES,
};

#[derive(Arbitrary, Debug)]
struct FuzzMessage {
    kind: u8,
    a: String,
    b: String,
    n: u64,
    data: Vec<u8>,
    entries: Vec<(u64, u64, Vec<u8>)>,
}

fn build(input: FuzzMessage) -> Message {
    let mut metadata = HashMap::new();
    metadata.insert(input.a.clone(), input.b.clone());
    let identity = PeerIdentity {
        spiffe_id: input.a.clone(),
        node_id: input.b.clone(),
        roles: vec![input.a.clone()],
    };
    let events: Vec<Event> = input
        .entries
        .iter()
        .take(MAX_EVENTS_PER_INGEST)
        .map(|(id, ts, payload)| Event {
            event_id: format!("event-{id}"),
            kind: input.a.clone(),
            occurred_at_unix_ms: *ts,
            payload: payload.clone(),
        })
        .collect();

    let body = match input.kind % 14 {
        0 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::LeaseAcquire(LeaseAcquire {
                lease_id: input.a,
                resource: input.b,
                ttl_ms: input.n as u32,
            }),
        },
        1 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::LeaseRelease(LeaseRelease {
                lease_id: input.a,
                fencing_token: input.n,
            }),
        },
        2 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::PolicyGet(PolicyGet {
                policy_id: input.a,
                known_version: input.n,
            }),
        },
        3 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::PolicyWatch(PolicyWatch {
                policy_id: input.a,
                since_version: input.n,
            }),
        },
        4 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::EventIngest(EventIngest { events }),
        },
        5 => Body::Request {
            identity,
            metadata,
            payload: RequestPayload::Heartbeat(Heartbeat {
                node_id: input.a,
                epoch: input.n,
            }),
        },
        6 => Body::Response(ResponsePayload::LeaseAcquire(LeaseAcquired {
            granted: input.n % 2 == 0,
            fencing_token: input.n,
            expires_at_unix_ms: input.n,
            holder: input.a,
        })),
        7 => Body::Response(ResponsePayload::LeaseRelease(LeaseReleased {
            released: input.n % 2 == 0,
        })),
        8 => Body::Response(ResponsePayload::PolicyGet(PolicyBundle {
            policy_id: input.a,
            version: input.n,
            bundle: input.data,
            checksum: input.b,
        })),
        9 => Body::Response(ResponsePayload::EventIngest(
            quiccrunch_core::messaging::EventIngested {
                accepted: input.n as u32,
                duplicate: input.n as u32,
            },
        )),
        10 => Body::Response(ResponsePayload::Heartbeat(HeartbeatAck {
            server_time_unix_ms: input.n,
            epoch: input.n,
        })),
        11 => Body::WatchEvent(PolicyWatchEventDto {
            policy_id: input.a,
            version: input.n,
            bundle: input.data,
            checksum: input.b,
        }),
        12 => Body::StreamEnd(input.a),
        _ => Body::Error {
            code: input.n as u16,
            message: input.a,
            retry_after_ms: input.n as u32,
            earliest_available: input.n,
            server_latest: input.n,
            retryable: input.n % 2 == 0,
        },
    };

    Message {
        contract_version: CONTRACT_VERSION,
        message_id: input.n,
        request_id: input.n,
        timestamp_ms: input.n,
        body,
    }
}

fn check_message(message: &Message) {
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

fuzz_target!(|data: &[u8]| {
    // Arbitrary bytes: every message type is reachable through the envelope
    // union, so this exercises the shared decode path and all payload decoders.
    if let Ok(message) = messaging::decode(data) {
        assert!(data.len() <= MAX_ENVELOPE_BYTES + 4);
        check_message(&message);
        let reencoded = messaging::encode(&message).expect("decoded message must re-encode");
        assert!(reencoded.len() <= MAX_ENVELOPE_BYTES + 4);
        let _ = messaging::decode(&reencoded).expect("re-encoded message must decode");
    }

    // Structured input: build and round-trip every message type.
    let mut unstructured = Unstructured::new(data);
    if let Ok(input) = FuzzMessage::arbitrary(&mut unstructured) {
        let message = build(input);
        if let Ok(frame) = messaging::encode(&message) {
            assert!(frame.len() <= MAX_ENVELOPE_BYTES + 4);
            let decoded = messaging::decode(&frame).expect("encoded message must decode");
            assert_eq!(decoded, message);
        }
    }
});
