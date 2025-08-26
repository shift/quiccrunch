use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};
use std::thread;

/// Power consumption metrics collector using RAPL with baseline calibration
pub struct PowerMetrics {
    rapl_paths: Vec<String>,
    baseline: PowerBaseline,
    last_readings: Vec<u64>,
    start_time: Instant,
}

/// Baseline power measurements
#[derive(Debug, Clone)]
pub struct PowerBaseline {
    pub idle_package_power: f64,
    pub idle_dram_power: f64,
    pub measurement_duration: Duration,
    pub variation_coefficient: f64,  // Measure of baseline stability
}

impl PowerMetrics {
    pub fn new() -> std::io::Result<Self> {
        let rapl_paths = Self::discover_rapl_paths()?;
        let baseline = Self::establish_baseline(&rapl_paths)?;
        
        let mut last_readings = Vec::new();
        for path in &rapl_paths {
            let mut file = File::open(path)?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            last_readings.push(content.trim().parse::<u64>().unwrap_or(0));
        }

        Ok(Self {
            rapl_paths,
            baseline,
            last_readings,
            start_time: Instant::now(),
        })
    }

    /// Discover available RAPL domains
    fn discover_rapl_paths() -> std::io::Result<Vec<String>> {
        let mut paths = Vec::new();
        
        // Check if RAPL is available
        if !std::path::Path::new("/sys/class/powercap/intel-rapl").exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "RAPL interface not available. Ensure running on Intel CPU with RAPL support."
            ));
        }

        // Find package (CPU) domains
        if let Ok(entries) = std::fs::read_dir("/sys/class/powercap/intel-rapl") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.to_string_lossy().contains("package") {
                    if let Some(energy_path) = path.join("energy_uj").to_str() {
                        // Verify we can read from this path
                        File::open(energy_path)?;
                        paths.push(energy_path.to_string());
                    }
                }
            }
        }

        // Find DRAM domains
        if let Ok(entries) = std::fs::read_dir("/sys/class/powercap/intel-rapl") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.to_string_lossy().contains("dram") {
                    if let Some(energy_path) = path.join("energy_uj").to_str() {
                        // Verify we can read from this path
                        File::open(energy_path)?;
                        paths.push(energy_path.to_string());
                    }
                }
            }
        }

        if paths.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No readable RAPL domains found"
            ));
        }

        Ok(paths)
    }

    /// Establish power baseline by measuring idle system
    fn establish_baseline(rapl_paths: &[String]) -> std::io::Result<PowerBaseline> {
        println!("Establishing power baseline...");
        println!("Please ensure system is idle (no heavy processes running)");
        thread::sleep(Duration::from_secs(5));  // Wait for system to settle

        const SAMPLES: usize = 10;
        const SAMPLE_DURATION: Duration = Duration::from_secs(2);
        
        let mut package_samples = Vec::new();
        let mut dram_samples = Vec::new();

        // Take multiple samples to ensure stable baseline
        for i in 0..SAMPLES {
            println!("Taking baseline sample {}/{}", i+1, SAMPLES);
            
            let mut initial_readings = Vec::new();
            for path in rapl_paths {
                let mut file = File::open(path)?;
                let mut content = String::new();
                file.read_to_string(&mut content)?;
                initial_readings.push(content.trim().parse::<u64>().unwrap_or(0));
            }

            thread::sleep(SAMPLE_DURATION);

            let mut final_readings = Vec::new();
            for path in rapl_paths {
                let mut file = File::open(path)?;
                let mut content = String::new();
                file.read_to_string(&mut content)?;
                final_readings.push(content.trim().parse::<u64>().unwrap_or(0));
            }

            // Calculate power for each domain
            for i in 0..rapl_paths.len() {
                let energy_uj = if final_readings[i] >= initial_readings[i] {
                    final_readings[i] - initial_readings[i]
                } else {
                    // Handle counter wraparound
                    u64::MAX - initial_readings[i] + final_readings[i]
                };

                let power_w = energy_uj as f64 / 1_000_000.0 / SAMPLE_DURATION.as_secs_f64();
                
                if i == 0 {  // Package domain
                    package_samples.push(power_w);
                } else {     // DRAM domain
                    dram_samples.push(power_w);
                }
            }

            // Check for system stability
            if i > 0 {
                let pkg_diff = (package_samples[i] - package_samples[i-1]).abs();
                if pkg_diff > 1.0 {  // More than 1W difference between samples
                    println!("Warning: System power fluctuating ({}W difference). Results may be inaccurate.", pkg_diff);
                }
            }
        }

        // Calculate averages and variation
        let avg_package = package_samples.iter().sum::<f64>() / SAMPLES as f64;
        let avg_dram = dram_samples.iter().sum::<f64>() / SAMPLES as f64;

        // Calculate coefficient of variation to measure baseline stability
        let pkg_variance = package_samples.iter()
            .map(|x| (x - avg_package).powi(2))
            .sum::<f64>() / SAMPLES as f64;
        let variation_coefficient = (pkg_variance.sqrt() / avg_package) * 100.0;

        let baseline = PowerBaseline {
            idle_package_power: avg_package,
            idle_dram_power: avg_dram,
            measurement_duration: SAMPLE_DURATION * SAMPLES as u32,
            variation_coefficient,
        };

        println!("Baseline established:");
        println!("  Package idle power: {:.2}W", baseline.idle_package_power);
        println!("  DRAM idle power: {:.2}W", baseline.idle_dram_power);
        println!("  Variation coefficient: {:.1}%", baseline.variation_coefficient);
        
        if baseline.variation_coefficient > 10.0 {
            println!("\nWARNING: High power variation detected (>10%). Consider:");
            println!("- Closing unnecessary applications");
            println!("- Disabling power management features");
            println!("- Ensuring consistent CPU frequency");
        }

        Ok(baseline)
    }

    /// Measure current power consumption relative to baseline
    pub fn measure(&mut self) -> std::io::Result<PowerSnapshot> {
        let mut current_readings = Vec::new();
        let elapsed = self.start_time.elapsed();

        for path in &self.rapl_paths {
            let mut file = File::open(path)?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            current_readings.push(content.trim().parse::<u64>().unwrap_or(0));
        }

        let mut power_data = Vec::new();
        for (i, (&last, &current)) in self.last_readings.iter().zip(&current_readings).enumerate() {
            let energy_uj = if current >= last {
                current - last
            } else {
                u64::MAX - last + current
            };
            
            // Convert to watts and subtract baseline
            let total_power_w = energy_uj as f64 / 1_000_000.0 / elapsed.as_secs_f64();
            let baseline_power = if i == 0 {
                self.baseline.idle_package_power
            } else {
                self.baseline.idle_dram_power
            };
            
            let active_power_w = (total_power_w - baseline_power).max(0.0);
            
            power_data.push(PowerDomain {
                domain: if i == 0 { "package" } else { "dram" }.to_string(),
                total_power_w: total_power_w,
                active_power_w: active_power_w,
                energy_uj,
                baseline_power_w: baseline_power,
            });
        }

        self.last_readings = current_readings;
        self.start_time = Instant::now();

        Ok(PowerSnapshot {
            domains: power_data,
            timestamp: Instant::now(),
            baseline: self.baseline.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct PowerDomain {
    pub domain: String,
    pub total_power_w: f64,      // Total power including baseline
    pub active_power_w: f64,     // Power above baseline
    pub energy_uj: u64,
    pub baseline_power_w: f64,   // Baseline power for this domain
}

#[derive(Debug, Clone)]
pub struct PowerSnapshot {
    pub domains: Vec<PowerDomain>,
    pub timestamp: Instant,
    pub baseline: PowerBaseline,
}

#[derive(Debug, Clone)]
pub struct PowerProfile {
    pub avg_package_power: f64,      // Average active CPU power
    pub avg_dram_power: f64,         // Average active DRAM power
    pub total_energy_j: f64,         // Total energy above baseline
    pub energy_per_message: f64,     // Active energy per message
    pub energy_per_byte: f64,        // Active energy per byte
    pub baseline: PowerBaseline,     // Reference baseline measurements
}

impl PowerProfile {
    pub fn new(snapshots: &[PowerSnapshot], total_messages: usize, total_bytes: usize) -> Self {
        let mut total_package_power = 0.0;
        let mut total_dram_power = 0.0;
        let mut total_energy_uj = 0;

        for snapshot in snapshots {
            for domain in &snapshot.domains {
                match domain.domain.as_str() {
                    "package" => total_package_power += domain.active_power_w,
                    "dram" => total_dram_power += domain.active_power_w,
                    _ => {}
                }
                total_energy_uj += domain.energy_uj;
            }
        }

        let num_snapshots = snapshots.len().max(1) as f64;
        let total_energy_j = total_energy_uj as f64 / 1_000_000.0;

        Self {
            avg_package_power: total_package_power / num_snapshots,
            avg_dram_power: total_dram_power / num_snapshots,
            total_energy_j,
            energy_per_message: total_energy_j / total_messages as f64,
            energy_per_byte: total_energy_j / total_bytes as f64,
            baseline: snapshots[0].baseline.clone(),
        }
    }

    pub fn get_efficiency_metrics(&self) -> PowerEfficiencyMetrics {
        PowerEfficiencyMetrics {
            total_active_power: self.avg_package_power + self.avg_dram_power,
            total_baseline_power: self.baseline.idle_package_power + self.baseline.idle_dram_power,
            power_increase_ratio: (self.avg_package_power + self.avg_dram_power) /
                                (self.baseline.idle_package_power + self.baseline.idle_dram_power),
            baseline_stability: self.baseline.variation_coefficient,
        }
    }
}

#[derive(Debug)]
pub struct PowerEfficiencyMetrics {
    pub total_active_power: f64,     // Total power above baseline
    pub total_baseline_power: f64,   // Total baseline power
    pub power_increase_ratio: f64,   // Ratio of active to baseline power
    pub baseline_stability: f64,     // Coefficient of variation in baseline
}