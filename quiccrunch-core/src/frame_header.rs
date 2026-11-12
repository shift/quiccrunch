#[cfg(feature = "telemetry")]
use opentelemetry::{
    propagation::{Extractor, Injector, TextMapPropagator},
    trace::{SpanContext, TraceContextExt, TraceFlags, TraceId, TraceState},
    Context,
};

/// Frame header for messages
#[derive(Debug, Clone)]
pub struct FrameHeader {
    /// Length of the payload
    pub length: u32,
    /// Optional trace context (only when telemetry feature is enabled)
    #[cfg(feature = "telemetry")]
    pub trace_context: Option<TraceContext>,
}

#[cfg(feature = "telemetry")]
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: u64,
    pub trace_flags: TraceFlags,
    pub trace_state: TraceState,
}

impl FrameHeader {
    pub fn new(length: u32) -> Self {
        Self {
            length,
            #[cfg(feature = "telemetry")]
            trace_context: None,
        }
    }

    #[cfg(feature = "telemetry")]
    pub fn with_current_context(length: u32) -> Self {
        let current = Context::current();
        let span = current.span();
        let span_context = span.span_context();

        let trace_context = if span_context.is_valid() {
            Some(TraceContext {
                trace_id: span_context.trace_id(),
                span_id: span_context.span_id(),
                trace_flags: span_context.trace_flags(),
                trace_state: span_context.trace_state().clone(),
            })
        } else {
            None
        };

        Self {
            length,
            trace_context,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        #[allow(unused_mut)]
        let mut buf = self.length.to_le_bytes().to_vec();

        #[cfg(feature = "telemetry")]
        if let Some(tc) = &self.trace_context {
            buf.extend_from_slice(&tc.trace_id.to_bytes());
            buf.extend_from_slice(&tc.span_id.to_le_bytes());
            buf.push(tc.trace_flags.to_u8());

            let state_bytes = tc.trace_state.to_string().into_bytes();
            buf.extend_from_slice(&(state_bytes.len() as u16).to_le_bytes());
            buf.extend_from_slice(&state_bytes);
        } else {
            // Empty trace context: 16-byte trace id, 8-byte span id, flags,
            // and a zero-length trace-state string.
            buf.extend_from_slice(&[0u8; 27]);
        }

        buf
    }

    pub fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() < 4 {
            return None;
        }

        let length = u32::from_le_bytes(buf[0..4].try_into().ok()?);

        #[cfg(feature = "telemetry")]
        {
            // 4-byte length + 16-byte trace id + 8-byte span id + 1-byte
            // flags + 2-byte state length = 31 bytes minimum.
            if buf.len() < 31 {
                return None;
            }

            let trace_id = TraceId::from_bytes(buf[4..20].try_into().ok()?);
            let span_id = u64::from_le_bytes(buf[20..28].try_into().ok()?);
            let trace_flags = TraceFlags::from_u8(buf[28]);

            let state_len = u16::from_le_bytes(buf[29..31].try_into().ok()?) as usize;
            let state_end = 31usize.checked_add(state_len)?;
            let state_bytes = buf.get(31..state_end)?;
            let trace_state =
                std::str::FromStr::from_str(std::str::from_utf8(state_bytes).ok()?).ok()?;

            let trace_context = if !trace_id.is_zero() {
                Some(TraceContext {
                    trace_id,
                    span_id,
                    trace_flags,
                    trace_state,
                })
            } else {
                None
            };

            Some(Self {
                length,
                trace_context,
            })
        }

        #[cfg(not(feature = "telemetry"))]
        Some(Self { length })
    }
}
