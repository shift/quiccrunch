#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;
    use tracing_test::traced_test;

    #[test]
    fn test_config_defaults() {
        let config = TransportConfig::default();
        
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.max_concurrent_streams, 1000);
        assert_eq!(config.keep_alive_interval, Duration::from_secs(5));
        assert_eq!(config.initial_rtt, Duration::from_millis(100));
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert_eq!(config.stream_read_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_config_custom_values() {
        let config = TransportConfig {
            connect_timeout: Duration::from_secs(30),
            max_concurrent_streams: 2000,
            keep_alive_interval: Duration::from_secs(10),
            initial_rtt: Duration::from_millis(50),
            max_message_size: 20 * 1024 * 1024,
            stream_read_timeout: Duration::from_secs(60),
        };
        
        assert_eq!(config.connect_timeout, Duration::from_secs(30));
        assert_eq!(config.max_concurrent_streams, 2000);
        assert_eq!(config.keep_alive_interval, Duration::from_secs(10));
        assert_eq!(config.initial_rtt, Duration::from_millis(50));
        assert_eq!(config.max_message_size, 20 * 1024 * 1024);
        assert_eq!(config.stream_read_timeout, Duration::from_secs(60));
    }
}