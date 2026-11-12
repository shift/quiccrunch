#![no_main]

use libfuzzer_sys::fuzz_target;
use quiccrunch_core::frame_header::FrameHeader;
use quiccrunch_core::messaging;

fuzz_target!(|data: &[u8]| {
    // Hand-written frame header: length prefix plus optional trace context.
    // A decoded header must re-encode and decode to the same length.
    if let Some(header) = FrameHeader::decode(data) {
        let encoded = header.encode();
        let decoded = FrameHeader::decode(&encoded).expect("encoded header must decode");
        assert_eq!(decoded.length, header.length);
    }

    // Messaging frame: u32 big-endian length + Cap'n Proto envelope.
    // The decoder must either return a clean error or a message that fits the
    // documented 4 MiB envelope bound and can be re-encoded.
    if let Ok(message) = messaging::decode(data) {
        assert!(
            data.len() <= messaging::MAX_ENVELOPE_BYTES + 4,
            "decoder accepted a frame larger than the documented bound"
        );
        let reencoded = messaging::encode(&message).expect("decoded message must re-encode");
        assert!(
            reencoded.len() <= messaging::MAX_ENVELOPE_BYTES + 4,
            "re-encoded frame exceeds the documented bound"
        );
        let _ = messaging::decode(&reencoded).expect("re-encoded message must decode");
    }
});
