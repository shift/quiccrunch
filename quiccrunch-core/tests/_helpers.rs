use quiccrunch_core::config::QuicTransportSettings as TransportSettings;

pub fn translate_quinn_to_settings<T>(_cfg: &T, base: TransportSettings) -> TransportSettings {
    // Backend-neutral placeholder: return base unchanged.
    base
}
