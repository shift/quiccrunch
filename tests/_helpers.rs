use quinn::TransportConfig;
use quiccrunch::transport::{TransportSettings, CongestionController};

pub fn translate_quinn_to_settings(cfg: &TransportConfig, mut base: TransportSettings) -> TransportSettings {
    use quinn::VarInt;
    // Map only fields used by tests; ignore others safely
    // TransportConfig has setters only; read via quinn-proto Config getters are limited.
    // We exploit Debug formatting is not stable; instead, we accept that base provides defaults
    // and only override from environment in tests directly when we construct base.
    // Therefore this helper currently returns base unchanged. Tests that require specific limits
    // should pass them through TransportSettings directly.
    let _ = cfg;
    base
}
