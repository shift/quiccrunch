use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientConfig {
    pub transport: QuicTransportSettings,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            transport: QuicTransportSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub transport: QuicTransportSettings,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            transport: QuicTransportSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum CongestionControl {
    BbrV3,
    Bbr,
    Cubic,
    NewReno,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Backend {
    Tquic,
    Quinn,
    IoUringAdvanced,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuicTransportSettings {
    // Liveness
    pub keep_alive_interval_ms: Option<u64>,
    pub max_idle_timeout_ms: Option<u64>,

    // Flow control
    pub initial_max_data: u64,
    pub initial_max_stream_data_bidi_remote: u64,

    // Performance / network
    pub congestion_control: CongestionControl,
    pub enable_multipath: bool,
    pub enable_hystart: bool,

    // Backend
    pub backend: Backend,
}

impl Default for QuicTransportSettings {
    fn default() -> Self {
        const MB: u64 = 1024 * 1024;
        Self {
            keep_alive_interval_ms: Some(50),
            max_idle_timeout_ms: Some(30_000),
            initial_max_data: 64 * MB,
            initial_max_stream_data_bidi_remote: 16 * MB,
            congestion_control: CongestionControl::BbrV3,
            enable_multipath: true,
            enable_hystart: true,
            backend: Backend::Tquic,
        }
    }
}

/// Standardized configuration for performance testing
/// Ensures identical settings across backends for fair comparison
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StandardizedBenchmarkConfig {
    /// Core transport settings
    pub transport: QuicTransportSettings,
    
    /// Additional backend-specific standardization
    pub max_concurrent_bidi_streams: u32,
    pub max_concurrent_uni_streams: u32,
    
    /// Test case optimization
    pub test_case_variant: TestCaseVariant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TestCaseVariant {
    /// Optimized for Test Case 1: RPC Ping-Pong (Latency)
    Latency,
    /// Optimized for Test Case 2: RPC Flood (Throughput)
    Throughput, 
    /// Optimized for Test Case 3: Bulk Data Streaming (Bandwidth)
    Streaming,
}

impl StandardizedBenchmarkConfig {
    /// Create standardized config for backend comparison
    /// Uses identical settings for both Quinn and tQUIC
    pub fn new_for_backend(backend: Backend, test_case: TestCaseVariant) -> Self {
        const MB: u64 = 1024 * 1024;
        
        let base_transport = QuicTransportSettings {
            // Specification requirements: 30s idle timeout, 5s keep-alive
            keep_alive_interval_ms: Some(5_000),
            max_idle_timeout_ms: Some(30_000),
            
            // Generous flow control windows to avoid bottlenecks  
            initial_max_data: match test_case {
                TestCaseVariant::Latency => 8 * MB,
                TestCaseVariant::Throughput => 64 * MB, 
                TestCaseVariant::Streaming => 256 * MB,
            },
            initial_max_stream_data_bidi_remote: match test_case {
                TestCaseVariant::Latency => 2 * MB,
                TestCaseVariant::Throughput => 16 * MB,
                TestCaseVariant::Streaming => 64 * MB,
            },
            
            // Use consistent congestion control for fair comparison
            // NewReno is available in both backends
            congestion_control: CongestionControl::NewReno,
            
            // Disable features that might differ between backends
            enable_multipath: false,
            enable_hystart: false,
            
            backend,
        };
        
        let (max_bidi, max_uni) = match test_case {
            TestCaseVariant::Latency => (100, 100),      // Few streams, low latency
            TestCaseVariant::Throughput => (10000, 1000), // Many streams, high concurrency
            TestCaseVariant::Streaming => (10, 10),      // Few streams, high bandwidth each
        };
        
        Self {
            transport: base_transport,
            max_concurrent_bidi_streams: max_bidi,
            max_concurrent_uni_streams: max_uni,
            test_case_variant: test_case,
        }
    }
    
    /// Validate configuration for benchmark compliance
    pub fn validate(&self) -> Result<(), String> {
        // Verify timeout settings match specification
        if self.transport.max_idle_timeout_ms != Some(30_000) {
            return Err("max_idle_timeout must be 30 seconds for specification compliance".to_string());
        }
        
        if self.transport.keep_alive_interval_ms != Some(5_000) {
            return Err("keep_alive_interval must be 5 seconds for specification compliance".to_string());
        }
        
        // Verify congestion control consistency
        if !matches!(self.transport.congestion_control, CongestionControl::NewReno) {
            return Err("congestion_control must be NewReno for consistent comparison".to_string());
        }
        
        // Verify features are disabled for consistency
        if self.transport.enable_multipath {
            return Err("enable_multipath must be false for consistent comparison".to_string());
        }
        
        if self.transport.enable_hystart {
            return Err("enable_hystart must be false for consistent comparison".to_string());
        }
        
        // Verify flow control windows are reasonable
        if self.transport.initial_max_data < 1024 * 1024 {
            return Err("initial_max_data should be at least 1MB".to_string());
        }
        
        if self.transport.initial_max_stream_data_bidi_remote < 512 * 1024 {
            return Err("initial_max_stream_data_bidi_remote should be at least 512KB".to_string());
        }
        
        Ok(())
    }
    
    /// Get configuration summary for reporting
    pub fn summary(&self) -> String {
        format!(
            "Standardized Benchmark Configuration:\n\
             - Backend: {:?}\n\
             - Test Case: {:?}\n\
             - Idle Timeout: {}s\n\
             - Keep Alive: {}s\n\
             - Connection Window: {}MB\n\
             - Stream Window: {}MB\n\
             - Max Concurrent Streams: {} bidi, {} uni\n\
             - Congestion Control: {:?}\n\
             - Multipath: {}, HyStart: {}",
            self.transport.backend,
            self.test_case_variant,
            self.transport.max_idle_timeout_ms.unwrap_or(0) / 1000,
            self.transport.keep_alive_interval_ms.unwrap_or(0) / 1000,
            self.transport.initial_max_data / (1024 * 1024),
            self.transport.initial_max_stream_data_bidi_remote / (1024 * 1024),
            self.max_concurrent_bidi_streams,
            self.max_concurrent_uni_streams,
            self.transport.congestion_control,
            self.transport.enable_multipath,
            self.transport.enable_hystart
        )
    }
}

/// Pre-configured standardized settings for the three test cases
pub struct BenchmarkConfigs;

impl BenchmarkConfigs {
    /// Test Case 1: RPC Ping-Pong (Latency) - Quinn Backend
    pub fn latency_quinn() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Quinn, TestCaseVariant::Latency)
    }
    
    /// Test Case 1: RPC Ping-Pong (Latency) - tQUIC Backend  
    pub fn latency_tquic() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Tquic, TestCaseVariant::Latency)
    }
    
    /// Test Case 2: RPC Flood (Throughput) - Quinn Backend
    pub fn throughput_quinn() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Quinn, TestCaseVariant::Throughput)
    }
    
    /// Test Case 2: RPC Flood (Throughput) - tQUIC Backend
    pub fn throughput_tquic() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Tquic, TestCaseVariant::Throughput)
    }
    
    /// Test Case 3: Bulk Data Streaming (Bandwidth) - Quinn Backend
    pub fn streaming_quinn() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Quinn, TestCaseVariant::Streaming)
    }
    
    /// Test Case 3: Bulk Data Streaming (Bandwidth) - tQUIC Backend
    pub fn streaming_tquic() -> StandardizedBenchmarkConfig {
        StandardizedBenchmarkConfig::new_for_backend(Backend::Tquic, TestCaseVariant::Streaming)
    }
    
    /// Verify all configurations are valid and consistent
    pub fn validate_all() -> Result<(), String> {
        let configs = [
            Self::latency_quinn(),
            Self::latency_tquic(),
            Self::throughput_quinn(),
            Self::throughput_tquic(),
            Self::streaming_quinn(),
            Self::streaming_tquic(),
        ];
        
        for config in &configs {
            config.validate()?;
        }
        
        // Verify cross-backend consistency
        Self::verify_consistency(&Self::latency_quinn(), &Self::latency_tquic())?;
        Self::verify_consistency(&Self::throughput_quinn(), &Self::throughput_tquic())?;
        Self::verify_consistency(&Self::streaming_quinn(), &Self::streaming_tquic())?;
        
        Ok(())
    }
    
    fn verify_consistency(config1: &StandardizedBenchmarkConfig, config2: &StandardizedBenchmarkConfig) -> Result<(), String> {
        // These settings must be identical for fair comparison
        if config1.transport.max_idle_timeout_ms != config2.transport.max_idle_timeout_ms {
            return Err("max_idle_timeout_ms must be identical across backends".to_string());
        }
        
        if config1.transport.keep_alive_interval_ms != config2.transport.keep_alive_interval_ms {
            return Err("keep_alive_interval_ms must be identical across backends".to_string());
        }
        
        if config1.transport.congestion_control != config2.transport.congestion_control {
            return Err("congestion_control must be identical across backends".to_string());
        }
        
        if config1.transport.initial_max_data != config2.transport.initial_max_data {
            return Err("initial_max_data must be identical across backends".to_string());
        }
        
        if config1.transport.initial_max_stream_data_bidi_remote != config2.transport.initial_max_stream_data_bidi_remote {
            return Err("initial_max_stream_data_bidi_remote must be identical across backends".to_string());
        }
        
        if config1.max_concurrent_bidi_streams != config2.max_concurrent_bidi_streams {
            return Err("max_concurrent_bidi_streams must be identical across backends".to_string());
        }
        
        if config1.max_concurrent_uni_streams != config2.max_concurrent_uni_streams {
            return Err("max_concurrent_uni_streams must be identical across backends".to_string());
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_standardized_config_validation() {
        let config = StandardizedBenchmarkConfig::new_for_backend(Backend::Quinn, TestCaseVariant::Latency);
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_all_benchmark_configs_valid() {
        assert!(BenchmarkConfigs::validate_all().is_ok());
    }
    
    #[test]
    fn test_backend_consistency() {
        let quinn_latency = BenchmarkConfigs::latency_quinn();
        let tquic_latency = BenchmarkConfigs::latency_tquic();
        
        // These should be identical for fair comparison
        assert_eq!(quinn_latency.transport.max_idle_timeout_ms, tquic_latency.transport.max_idle_timeout_ms);
        assert_eq!(quinn_latency.transport.keep_alive_interval_ms, tquic_latency.transport.keep_alive_interval_ms);
        assert_eq!(quinn_latency.transport.congestion_control, tquic_latency.transport.congestion_control);
        assert_eq!(quinn_latency.max_concurrent_bidi_streams, tquic_latency.max_concurrent_bidi_streams);
    }
    
    #[test]
    fn test_test_case_optimization() {
        let latency = BenchmarkConfigs::latency_quinn();
        let throughput = BenchmarkConfigs::throughput_quinn();
        let streaming = BenchmarkConfigs::streaming_quinn();
        
        // Latency should have smallest windows
        assert!(latency.transport.initial_max_data < throughput.transport.initial_max_data);
        assert!(throughput.transport.initial_max_data < streaming.transport.initial_max_data);
        
        // Throughput should have most concurrent streams
        assert!(latency.max_concurrent_bidi_streams < throughput.max_concurrent_bidi_streams);
        assert!(streaming.max_concurrent_bidi_streams < throughput.max_concurrent_bidi_streams);
    }
}
