#[cfg(test)]
mod tests {
    use quiccrunch_core::schema::frame_capnp;
    use proptest::prelude::*;
    use capnp::message::{Builder, HeapAllocator};

    proptest! {
        #[test]
        fn test_frame_header_roundtrip(
            length in 0u32..10_000_000,
        ) {
            let mut message = Builder::<HeapAllocator>::new_default();
            {
                let mut frame = message.init_root::<frame_capnp::frame::Builder>();
                frame.set_length(length);
            }

            let bytes = {
                let mut array = Vec::new();
                capnp::serialize::write_message(&mut array, &message).unwrap();
                array
            };

            let mut cursor = std::io::Cursor::new(&bytes[..]);
            let message_reader = capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::default()).unwrap();
            let decoded = message_reader.get_root::<frame_capnp::frame::Reader>().unwrap();
            
            prop_assert_eq!(length, decoded.get_length());
        }

        #[test]
        #[cfg(feature = "telemetry")]
        fn test_frame_header_trace_context(
            length in 0u32..10_000_000,
            trace_id_bytes in prop::array::uniform16(any::<u8>()),
            span_id in any::<u64>(),
        ) {
            let trace_id = opentelemetry::trace::TraceId::from_bytes(trace_id_bytes);
            let trace_flags = opentelemetry::trace::TraceFlags::SAMPLED;

            let mut message = Builder::<HeapAllocator>::new_default();
            {
                let mut frame = message.init_root::<frame_capnp::frame::Builder>();
                frame.set_length(length);

                let mut ctx = frame.init_trace_context();
                ctx.set_trace_id(&trace_id.to_bytes());
                ctx.set_span_id(span_id);
                ctx.set_flags(trace_flags.to_u8());
            }

            let bytes = {
                let mut array = Vec::new();
                capnp::serialize::write_message(&mut array, &message).unwrap();
                array
            };

            let mut cursor = std::io::Cursor::new(&bytes[..]);
            let message_reader = capnp::serialize::read_message(&mut cursor, capnp::message::ReaderOptions::default()).unwrap();
            let decoded = message_reader.get_root::<frame_capnp::frame::Reader>().unwrap();
            
            prop_assert_eq!(length, decoded.get_length());

            let decoded_ctx = decoded.get_trace_context().unwrap();
            prop_assert_eq!(&decoded_ctx.get_trace_id().unwrap(), &trace_id.to_bytes());
            prop_assert_eq!(decoded_ctx.get_span_id(), span_id);
            prop_assert_eq!(decoded_ctx.get_flags(), trace_flags.to_u8());
        }
    }
}
