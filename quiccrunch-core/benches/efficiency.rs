#[derive(Debug, Clone)]
pub struct EfficiencyMetrics {
    // Throughput metrics
    pub messages_per_second: f64,
    pub bandwidth_gbps: f64,
    pub latency_us: f64,

    // Power metrics
    pub power_profile: PowerProfile,
    pub energy_per_message_uj: f64,
    pub energy_per_gb: f64,

    // Derived efficiency metrics
    pub messages_per_joule: f64,
    pub bits_per_joule: f64,
    pub performance_per_watt: f64, // Messages/sec/watt

    // System utilization
    pub cpu_utilization: f64, // 0.0 to 1.0
    pub memory_bandwidth_gbps: f64,

    // Combined metrics
    pub efficiency_score: f64, // Weighted composite score
}

impl EfficiencyMetrics {
    pub fn new(
        throughput_stats: &ThroughputStats,
        power_stats: &PowerProfile,
        system_stats: &SystemStats,
    ) -> Self {
        let messages_per_second =
            throughput_stats.total_messages as f64 / throughput_stats.duration.as_secs_f64();

        let bandwidth_gbps = (throughput_stats.total_bytes as f64 * 8.0)
            / throughput_stats.duration.as_secs_f64()
            / 1_000_000_000.0;

        let total_energy_j = power_stats.total_energy_j;
        let total_active_power = power_stats.avg_package_power + power_stats.avg_dram_power;

        let energy_per_message_uj =
            (total_energy_j * 1_000_000.0) / throughput_stats.total_messages as f64;

        let energy_per_gb =
            total_energy_j / (bandwidth_gbps * throughput_stats.duration.as_secs_f64());

        let messages_per_joule = throughput_stats.total_messages as f64 / total_energy_j;
        let bits_per_joule = (throughput_stats.total_bytes as f64 * 8.0) / total_energy_j;

        let performance_per_watt = messages_per_second / total_active_power;

        // Calculate efficiency score (weighted combination of key metrics)
        let efficiency_score = {
            let throughput_score = bandwidth_gbps / energy_per_gb;
            let latency_score =
                1.0 / (throughput_stats.avg_latency.as_micros() as f64 * energy_per_message_uj);
            let utilization_score = performance_per_watt * (1.0 - system_stats.cpu_utilization);

            // Weight the scores (can be adjusted based on priorities)
            throughput_score * 0.4 + latency_score * 0.4 + utilization_score * 0.2
        };

        Self {
            messages_per_second,
            bandwidth_gbps,
            latency_us: throughput_stats.avg_latency.as_micros() as f64,
            power_profile: power_stats.clone(),
            energy_per_message_uj,
            energy_per_gb,
            messages_per_joule,
            bits_per_joule,
            performance_per_watt,
            cpu_utilization: system_stats.cpu_utilization,
            memory_bandwidth_gbps: system_stats.memory_bandwidth_gbps,
            efficiency_score,
        }
    }

    pub fn print_summary(&self) {
        println!("\nEfficiency Metrics Summary:");
        println!("---------------------------");
        println!("Throughput:");
        println!("  Messages/sec: {:.2}", self.messages_per_second);
        println!("  Bandwidth: {:.2} Gbps", self.bandwidth_gbps);
        println!("  Latency: {:.2} μs", self.latency_us);

        println!("\nPower & Energy:");
        println!(
            "  Package Power: {:.2}W",
            self.power_profile.avg_package_power
        );
        println!("  DRAM Power: {:.2}W", self.power_profile.avg_dram_power);
        println!("  Energy/Message: {:.2} μJ", self.energy_per_message_uj);
        println!("  Energy/GB: {:.2} J", self.energy_per_gb);

        println!("\nEfficiency:");
        println!("  Messages/Joule: {:.2}", self.messages_per_joule);
        println!("  Bits/Joule: {:.2}", self.bits_per_joule);
        println!(
            "  Performance/Watt: {:.2} msgs/sec/W",
            self.performance_per_watt
        );

        println!("\nSystem Utilization:");
        println!("  CPU: {:.1}%", self.cpu_utilization * 100.0);
        println!("  Memory Bandwidth: {:.2} GB/s", self.memory_bandwidth_gbps);

        println!("\nComposite Efficiency Score: {:.2}", self.efficiency_score);
    }
}
