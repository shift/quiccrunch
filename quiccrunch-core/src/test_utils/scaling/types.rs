use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    pub frequency_mhz: Option<u32>,
    pub governor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub bandwidth_gbps: f64,
    pub latency_us: f64,
    pub operations_per_sec: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerMetrics {
    pub avg_package_power: f64,
    pub avg_dram_power: f64,
    pub peak_package_power: f64,
    pub peak_dram_power: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureMetrics {
    pub cpu_package_temp: f64,
    pub max_core_temp: f64,
    pub memory_temp: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfficiencyMetrics {
    pub efficiency_score: f64,
    pub performance_per_watt: f64,
    pub energy_per_operation: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingTestResult {
    pub config: ScalingConfig,
    pub performance: PerformanceMetrics,
    pub power: PowerMetrics,
    pub temperatures: TemperatureMetrics,
    pub efficiency: EfficiencyMetrics,
}