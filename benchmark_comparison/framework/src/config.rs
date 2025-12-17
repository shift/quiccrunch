use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub test_name: String,
    pub description: Option<String>,
    pub implementations: Vec<ImplementationConfig>,
    pub test_scenarios: Vec<TestScenario>,
    pub global_settings: GlobalSettings,
    pub output_settings: OutputSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationConfig {
    pub name: String,
    pub implementation_type: ImplementationType,
    pub server_command: String,
    pub client_command: String,
    pub working_directory: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub startup_timeout_secs: u64,
    pub shutdown_timeout_secs: u64,
    pub health_check_url: Option<String>,
    pub health_check_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImplementationType {
    GRPC,
    QUICCRUNCH,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestScenario {
    pub name: String,
    pub description: Option<String>,
    pub message_sizes: Vec<usize>,
    pub concurrency_levels: Vec<usize>,
    pub duration_secs: u64,
    pub warmup_duration_secs: u64,
    pub connections_per_client: usize,
    pub messages_per_connection: Option<usize>,
    pub rate_limit: Option<u64>, // messages per second
    pub payload_pattern: PayloadPattern,
    pub network_conditions: Option<NetworkConditions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PayloadPattern {
    Random,
    Sequential,
    Zeroes,
    Repeating(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditions {
    pub bandwidth_mbps: Option<u64>,
    pub latency_ms: Option<u64>,
    pub packet_loss_percent: Option<f64>,
    pub jitter_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub parallel_tests: usize,
    pub max_retries: usize,
    pub cleanup_between_tests: bool,
    pub system_monitoring_interval_ms: u64,
    pub resource_limits: ResourceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_cpu_percent: f64,
    pub max_memory_mb: u64,
    pub max_disk_usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSettings {
    pub results_directory: PathBuf,
    pub formats: Vec<OutputFormat>,
    pub include_raw_data: bool,
    pub generate_charts: bool,
    pub chart_types: Vec<ChartType>,
    pub compression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputFormat {
    Json,
    Csv,
    Yaml,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChartType {
    LatencyComparison,
    ThroughputComparison,
    ResourceUsage,
    ErrorRates,
    PercentileDistribution,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            test_name: "default_benchmark".to_string(),
            description: None,
            implementations: vec![
                ImplementationConfig {
                    name: "gRPC".to_string(),
                    implementation_type: ImplementationType::GRPC,
                    server_command: "cargo run --bin server".to_string(),
                    client_command: "cargo run --bin client".to_string(),
                    working_directory: Some(PathBuf::from("../grpc")),
                    environment: HashMap::new(),
                    startup_timeout_secs: 30,
                    shutdown_timeout_secs: 10,
                    health_check_url: Some("http://localhost:50051".to_string()),
                    health_check_timeout_secs: 5,
                },
                ImplementationConfig {
                    name: "QUICCRUNCH".to_string(),
                    implementation_type: ImplementationType::QUICCRUNCH,
                    server_command: "cargo run --bin server".to_string(),
                    client_command: "cargo run --bin client".to_string(),
                    working_directory: Some(PathBuf::from("../quiccrunch")),
                    environment: HashMap::new(),
                    startup_timeout_secs: 30,
                    shutdown_timeout_secs: 10,
                    health_check_url: Some("http://localhost:8080".to_string()),
                    health_check_timeout_secs: 5,
                },
            ],
            test_scenarios: vec![
                TestScenario {
                    name: "light_load".to_string(),
                    description: Some("Light load test for basic functionality".to_string()),
                    message_sizes: vec![64, 256, 1024],
                    concurrency_levels: vec![1, 5, 10],
                    duration_secs: 30,
                    warmup_duration_secs: 5,
                    connections_per_client: 1,
                    messages_per_connection: Some(1000),
                    rate_limit: None,
                    payload_pattern: PayloadPattern::Random,
                    network_conditions: None,
                },
                TestScenario {
                    name: "medium_load".to_string(),
                    description: Some("Medium load test for typical usage".to_string()),
                    message_sizes: vec![1024, 4096, 16384],
                    concurrency_levels: vec![10, 50, 100],
                    duration_secs: 60,
                    warmup_duration_secs: 10,
                    connections_per_client: 5,
                    messages_per_connection: None,
                    rate_limit: Some(10000),
                    payload_pattern: PayloadPattern::Random,
                    network_conditions: None,
                },
                TestScenario {
                    name: "heavy_load".to_string(),
                    description: Some("Heavy load test for performance limits".to_string()),
                    message_sizes: vec![4096, 65536, 1048576],
                    concurrency_levels: vec![100, 500, 1000],
                    duration_secs: 120,
                    warmup_duration_secs: 20,
                    connections_per_client: 10,
                    messages_per_connection: None,
                    rate_limit: Some(50000),
                    payload_pattern: PayloadPattern::Random,
                    network_conditions: None,
                },
            ],
            global_settings: GlobalSettings {
                parallel_tests: 2,
                max_retries: 3,
                cleanup_between_tests: true,
                system_monitoring_interval_ms: 1000,
                resource_limits: ResourceLimits {
                    max_cpu_percent: 90.0,
                    max_memory_mb: 8192,
                    max_disk_usage_percent: 80.0,
                },
            },
            output_settings: OutputSettings {
                results_directory: PathBuf::from("./results"),
                formats: vec![OutputFormat::Json, OutputFormat::Csv],
                include_raw_data: true,
                generate_charts: true,
                chart_types: vec![
                    ChartType::LatencyComparison,
                    ChartType::ThroughputComparison,
                    ChartType::ResourceUsage,
                ],
                compression: false,
            },
        }
    }
}

impl BenchmarkConfig {
    pub fn load_from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: BenchmarkConfig = if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };
        Ok(config)
    }

    pub fn save_to_file(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
            serde_yaml::to_string_pretty(self)?
        } else {
            serde_json::to_string_pretty(self)?
        };
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.implementations.is_empty() {
            anyhow::bail!("At least one implementation must be configured");
        }

        if self.test_scenarios.is_empty() {
            anyhow::bail!("At least one test scenario must be configured");
        }

        for impl_config in &self.implementations {
            if impl_config.name.is_empty() {
                anyhow::bail!("Implementation name cannot be empty");
            }
            if impl_config.server_command.is_empty() {
                anyhow::bail!("Server command cannot be empty for implementation {}", impl_config.name);
            }
            if impl_config.client_command.is_empty() {
                anyhow::bail!("Client command cannot be empty for implementation {}", impl_config.name);
            }
        }

        for scenario in &self.test_scenarios {
            if scenario.name.is_empty() {
                anyhow::bail!("Scenario name cannot be empty");
            }
            if scenario.message_sizes.is_empty() {
                anyhow::bail!("Message sizes cannot be empty for scenario {}", scenario.name);
            }
            if scenario.concurrency_levels.is_empty() {
                anyhow::bail!("Concurrency levels cannot be empty for scenario {}", scenario.name);
            }
        }

        Ok(())
    }
}