use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub scenarios: HashMap<String, ScenarioConfig>,
    pub performance: PerformanceConfig,
    pub regression: RegressionConfig,
    pub reporting: ReportingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub name: String,
    pub description: Option<String>,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub connections: u32,
    pub request_rate: RequestRateConfig,
    pub message_size: MessageSizeConfig,
    pub backend: String,
    pub streaming: Option<StreamingConfig>,
    pub network_conditions: Option<NetworkConditionsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RequestRateConfig {
    Constant { rate: u32 },
    Variable { pattern: String, base_rate: u32, variation: f64 },
    Burst { base_rate: u32, burst_rate: u32, burst_duration: Duration },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageSizeConfig {
    Fixed { size: usize },
    Range { min: usize, max: usize },
    Distribution { pattern: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    pub stream_count: u32,
    #[serde(with = "humantime_serde")]
    pub stream_duration: Duration,
    pub chunk_size: usize,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditionsConfig {
    pub latency: Option<Duration>,
    pub packet_loss: Option<f64>,
    pub bandwidth_mbps: Option<u32>,
    pub jitter: Option<Duration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub thresholds: PerformanceThresholds,
    pub warmup_duration: Duration,
    pub cooldown_duration: Duration,
    pub metrics_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    #[serde(with = "humantime_serde")]
    pub max_latency_p99: Duration,
    pub min_throughput: u32,
    pub max_error_rate: f64,
    pub max_memory_mb: Option<u32>,
    pub max_cpu_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionConfig {
    pub storage: RegressionStorage,
    pub retention: String,
    pub confidence_level: f64,
    pub significance_threshold: f64,
    pub minimum_samples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RegressionStorage {
    File { path: String },
    Database { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportingConfig {
    pub output_dir: String,
    pub formats: Vec<String>,
    pub include_graphs: bool,
    pub include_raw_data: bool,
    pub dashboard_port: Option<u16>,
}

impl TestConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .context("Failed to read configuration file")?;
        
        if path.as_ref().extension().and_then(|s| s.to_str()) == Some("json") {
            serde_json::from_str(&content)
                .context("Failed to parse JSON configuration")
        } else {
            toml::from_str(&content)
                .context("Failed to parse TOML configuration")
        }
    }

    pub fn default_config() -> Self {
        Self {
            scenarios: default_scenarios(),
            performance: PerformanceConfig {
                thresholds: PerformanceThresholds {
                    max_latency_p99: Duration::from_millis(100),
                    min_throughput: 10000,
                    max_error_rate: 0.001,
                    max_memory_mb: Some(1024),
                    max_cpu_percent: Some(80.0),
                },
                warmup_duration: Duration::from_secs(30),
                cooldown_duration: Duration::from_secs(10),
                metrics_interval: Duration::from_millis(100),
            },
            regression: RegressionConfig {
                storage: RegressionStorage::File {
                    path: "baselines/".to_string(),
                },
                retention: "30d".to_string(),
                confidence_level: 0.95,
                significance_threshold: 0.05,
                minimum_samples: 100,
            },
            reporting: ReportingConfig {
                output_dir: "reports/".to_string(),
                formats: vec!["json".to_string(), "html".to_string()],
                include_graphs: true,
                include_raw_data: false,
                dashboard_port: Some(8080),
            },
        }
    }

    pub fn get_scenario(&self, name: &str) -> Result<&ScenarioConfig> {
        self.scenarios.get(name)
            .with_context(|| format!("Scenario '{}' not found", name))
    }
}

fn default_scenarios() -> HashMap<String, ScenarioConfig> {
    let mut scenarios = HashMap::new();

    scenarios.insert("simple".to_string(), ScenarioConfig {
        name: "Simple Load Test".to_string(),
        description: Some("Basic functionality validation under load".to_string()),
        duration: Duration::from_secs(60),
        connections: 100,
        request_rate: RequestRateConfig::Constant { rate: 1000 },
        message_size: MessageSizeConfig::Fixed { size: 1024 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    });

    scenarios.insert("realistic".to_string(), ScenarioConfig {
        name: "Realistic Workload".to_string(),
        description: Some("Production-like traffic patterns".to_string()),
        duration: Duration::from_secs(300),
        connections: 500,
        request_rate: RequestRateConfig::Variable {
            pattern: "sine".to_string(),
            base_rate: 2000,
            variation: 0.3,
        },
        message_size: MessageSizeConfig::Distribution {
            pattern: "realistic".to_string(),
        },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    });

    scenarios.insert("streaming".to_string(), ScenarioConfig {
        name: "Streaming RPC Test".to_string(),
        description: Some("Validate streaming RPC performance".to_string()),
        duration: Duration::from_secs(180),
        connections: 200,
        request_rate: RequestRateConfig::Constant { rate: 100 },
        message_size: MessageSizeConfig::Range { min: 4096, max: 65536 },
        backend: "quinn".to_string(),
        streaming: Some(StreamingConfig {
            stream_count: 50,
            stream_duration: Duration::from_secs(30),
            chunk_size: 8192,
            bidirectional: true,
        }),
        network_conditions: None,
    });

    scenarios.insert("burst".to_string(), ScenarioConfig {
        name: "Burst Traffic Test".to_string(),
        description: Some("Handle sudden traffic spikes".to_string()),
        duration: Duration::from_secs(120),
        connections: 1000,
        request_rate: RequestRateConfig::Burst {
            base_rate: 500,
            burst_rate: 5000,
            burst_duration: Duration::from_secs(10),
        },
        message_size: MessageSizeConfig::Fixed { size: 512 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: None,
    });

    scenarios.insert("stress".to_string(), ScenarioConfig {
        name: "Resource Exhaustion Test".to_string(),
        description: Some("System behavior under resource constraints".to_string()),
        duration: Duration::from_secs(600),
        connections: 2000,
        request_rate: RequestRateConfig::Variable {
            pattern: "ramp".to_string(),
            base_rate: 1000,
            variation: 2.0,
        },
        message_size: MessageSizeConfig::Range { min: 1024, max: 32768 },
        backend: "quinn".to_string(),
        streaming: None,
        network_conditions: Some(NetworkConditionsConfig {
            latency: Some(Duration::from_millis(20)),
            packet_loss: Some(0.001),
            bandwidth_mbps: None,
            jitter: Some(Duration::from_millis(5)),
        }),
    });

    scenarios
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TestConfig::default_config();
        assert!(!config.scenarios.is_empty());
        assert!(config.scenarios.contains_key("simple"));
        assert!(config.scenarios.contains_key("realistic"));
    }

    #[test]
    fn test_config_serialization() {
        let config = TestConfig::default_config();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: TestConfig = serde_json::from_str(&json).unwrap();
        
        assert_eq!(config.scenarios.len(), deserialized.scenarios.len());
    }
}