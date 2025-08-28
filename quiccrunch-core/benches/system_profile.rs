use serde::{Serialize, Deserialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemProfile {
    // CPU Information
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub motherboard: MotherboardInfo,
    pub os_info: OsInfo,
    
    // Runtime metrics
    pub temperatures: TempMetrics,
    pub cooling: CoolingMetrics,
    pub power_limits: PowerLimits,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    pub model_name: String,
    pub microarchitecture: String,
    pub physical_cores: u32,
    pub logical_cores: u32,
    pub base_frequency_mhz: u32,
    pub max_frequency_mhz: u32,
    pub cache_info: CacheInfo,
    pub flags: Vec<String>,
    pub stepping: String,
    pub bugs: Vec<String>,          // Known CPU bugs/vulnerabilities
    pub microcode_version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheInfo {
    pub l1d_size_kb: u32,
    pub l1i_size_kb: u32,
    pub l2_size_kb: u32,
    pub l3_size_kb: u32,
    pub line_size: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryInfo {
    pub total_memory_mb: u64,
    pub memory_type: String,        // DDR4, DDR5, etc.
    pub channels: u32,
    pub speed_mhz: u32,
    pub timings: String,            // e.g., "16-18-18-36"
    pub ecc: bool,
    pub max_bandwidth: Option<f64>, // GB/s
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MotherboardInfo {
    pub manufacturer: String,
    pub model: String,
    pub bios_version: String,
    pub bios_date: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsInfo {
    pub name: String,
    pub version: String,
    pub kernel: String,
    pub scheduler: String,
    pub power_governor: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TempMetrics {
    pub cpu_package: Vec<SensorReading>,
    pub cpu_cores: Vec<Vec<SensorReading>>,
    pub memory: Vec<SensorReading>,
    pub motherboard: Vec<SensorReading>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoolingMetrics {
    pub cpu_fan_rpm: Vec<SensorReading>,
    pub case_fans_rpm: Vec<Vec<SensorReading>>,
    pub pump_rpm: Option<Vec<SensorReading>>,  // For liquid cooling
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PowerLimits {
    pub package_tdp: f64,           // Watts
    pub current_pl1: f64,           // Short term power limit
    pub current_pl2: f64,           // Long term power limit
    pub tau: f64,                   // Power limit time window
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SensorReading {
    pub timestamp: f64,
    pub value: f64,
}

impl SystemProfile {
    pub fn new() -> std::io::Result<Self> {
        let cpu = Self::gather_cpu_info()?;
        let memory = Self::gather_memory_info()?;
        let motherboard = Self::gather_motherboard_info()?;
        let os_info = Self::gather_os_info()?;
        
        Ok(Self {
            cpu,
            memory,
            motherboard,
            os_info,
            temperatures: TempMetrics::default(),
            cooling: CoolingMetrics::default(),
            power_limits: Self::gather_power_limits()?,
        })
    }

    fn gather_cpu_info() -> std::io::Result<CpuInfo> {
        let mut cpu_info = String::new();
        File::open("/proc/cpuinfo")?.read_to_string(&mut cpu_info)?;

        let mut model_name = String::new();
        let mut physical_cores = 0;
        let mut logical_cores = 0;
        let mut flags = Vec::new();
        let mut bugs = Vec::new();
        let mut microcode = String::new();

        for line in cpu_info.lines() {
            if line.starts_with("model name") {
                model_name = line.split(":").nth(1).unwrap_or("").trim().to_string();
            } else if line.starts_with("cpu cores") {
                physical_cores = line.split(":").nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            } else if line.starts_with("siblings") {
                logical_cores = line.split(":").nth(1).unwrap_or("0").trim().parse().unwrap_or(0);
            } else if line.starts_with("flags") {
                flags = line.split(":").nth(1).unwrap_or("")
                    .trim()
                    .split_whitespace()
                    .map(String::from)
                    .collect();
            } else if line.starts_with("bugs") {
                bugs = line.split(":").nth(1).unwrap_or("")
                    .trim()
                    .split_whitespace()
                    .map(String::from)
                    .collect();
            } else if line.starts_with("microcode") {
                microcode = line.split(":").nth(1).unwrap_or("").trim().to_string();
            }
        }

        // Get CPU frequencies
        let base_freq = Self::read_cpu_freq("/sys/devices/system/cpu/cpu0/cpufreq/base_frequency")?;
        let max_freq = Self::read_cpu_freq("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")?;

        // Get cache information
        let cache_info = Self::gather_cache_info()?;

        // Determine microarchitecture
        let microarchitecture = Self::determine_microarchitecture(&model_name, &flags);

        Ok(CpuInfo {
            model_name,
            microarchitecture,
            physical_cores,
            logical_cores,
            base_frequency_mhz: base_freq,
            max_frequency_mhz: max_freq,
            cache_info,
            flags,
            stepping: Self::read_cpu_stepping()?,
            bugs,
            microcode_version: microcode,
        })
    }

    fn gather_memory_info() -> std::io::Result<MemoryInfo> {
        // Use dmidecode for detailed memory information
        let output = Command::new("dmidecode")
            .arg("--type")
            .arg("memory")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        
        let mut total_memory_mb = 0;
        let mut memory_type = String::new();
        let mut speed_mhz = 0;
        let mut channels = 0;
        let mut timings = String::new();
        let mut ecc = false;

        // Parse dmidecode output
        for line in output_str.lines() {
            if line.contains("Size:") {
                if let Some(size_str) = line.split(':').nth(1) {
                    if size_str.contains("MB") {
                        if let Some(size) = size_str.trim().split(' ').next() {
                            total_memory_mb += size.parse::<u64>().unwrap_or(0);
                        }
                    }
                }
            } else if line.contains("Type:") && memory_type.is_empty() {
                memory_type = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.contains("Speed:") && speed_mhz == 0 {
                if let Some(speed_str) = line.split(':').nth(1) {
                    if speed_str.contains("MHz") {
                        if let Some(speed) = speed_str.trim().split(' ').next() {
                            speed_mhz = speed.parse().unwrap_or(0);
                        }
                    }
                }
            } else if line.contains("Error Correction Type:") {
                ecc = line.contains("ECC");
            }
        }

        // Try to get memory bandwidth from CPU specs
        let max_bandwidth = Self::estimate_memory_bandwidth(&memory_type, speed_mhz, channels);

        Ok(MemoryInfo {
            total_memory_mb,
            memory_type,
            channels,
            speed_mhz,
            timings,
            ecc,
            max_bandwidth,
        })
    }

    fn gather_motherboard_info() -> std::io::Result<MotherboardInfo> {
        let output = Command::new("dmidecode")
            .arg("--type")
            .arg("baseboard")
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        
        let mut manufacturer = String::new();
        let mut model = String::new();
        let mut bios_version = String::new();
        let mut bios_date = String::new();

        for line in output_str.lines() {
            if line.contains("Manufacturer:") {
                manufacturer = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.contains("Product Name:") {
                model = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        // Get BIOS information
        let bios_output = Command::new("dmidecode")
            .arg("--type")
            .arg("bios")
            .output()?;

        let bios_str = String::from_utf8_lossy(&bios_output.stdout);
        
        for line in bios_str.lines() {
            if line.contains("Version:") {
                bios_version = line.split(':').nth(1).unwrap_or("").trim().to_string();
            } else if line.contains("Release Date:") {
                bios_date = line.split(':').nth(1).unwrap_or("").trim().to_string();
            }
        }

        Ok(MotherboardInfo {
            manufacturer,
            model,
            bios_version,
            bios_date,
        })
    }

    fn gather_os_info() -> std::io::Result<OsInfo> {
        let mut os_release = String::new();
        File::open("/etc/os-release")?.read_to_string(&mut os_release)?;

        let mut name = String::new();
        let mut version = String::new();

        for line in os_release.lines() {
            if line.starts_with("NAME=") {
                name = line.split('=').nth(1).unwrap_or("").trim_matches('"').to_string();
            } else if line.starts_with("VERSION=") {
                version = line.split('=').nth(1).unwrap_or("").trim_matches('"').to_string();
            }
        }

        // Get kernel version
        let kernel = Command::new("uname")
            .arg("-r")
            .output()?
            .stdout;
        let kernel = String::from_utf8_lossy(&kernel).trim().to_string();

        // Get scheduler
        let scheduler = std::fs::read_to_string("/sys/block/sda/queue/scheduler")?
            .trim()
            .to_string();

        // Get CPU governor
        let governor = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")?
            .trim()
            .to_string();

        Ok(OsInfo {
            name,
            version,
            kernel,
            scheduler,
            power_governor: governor,
        })
    }

    pub fn start_monitoring(&mut self) -> std::io::Result<()> {
        // Start monitoring threads for temperatures and fans
        self.monitor_temperatures()?;
        self.monitor_cooling()?;
        Ok(())
    }

    pub fn take_snapshot(&mut self) -> std::io::Result<()> {
        self.update_temperatures()?;
        self.update_cooling()?;
        self.update_power_limits()?;
        Ok(())
    }

    fn monitor_temperatures(&mut self) -> std::io::Result<()> {
        // Read temperatures from various sources
        self.read_cpu_temperatures()?;
        self.read_memory_temperatures()?;
        self.read_motherboard_temperatures()?;
        Ok(())
    }

    fn monitor_cooling(&mut self) -> std::io::Result<()> {
        // Read fan speeds and pump speeds if available
        self.read_fan_speeds()?;
        self.read_pump_speeds()?;
        Ok(())
    }

    pub fn generate_system_report(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

// Implementation of monitoring functions...
impl SystemProfile {
    fn read_cpu_temperatures(&mut self) -> std::io::Result<()> {
        // Read from /sys/class/thermal and hwmon
        let mut package_temp = Vec::new();
        let mut core_temps = Vec::new();

        // Try k10temp first (AMD)
        if let Ok(temp) = std::fs::read_to_string("/sys/class/hwmon/hwmon0/temp1_input") {
            if let Ok(t) = temp.trim().parse::<f64>() {
                package_temp.push(SensorReading {
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs_f64(),
                    value: t / 1000.0,
                });
            }
        }

        // Try coretemp (Intel)
        for entry in std::fs::read_dir("/sys/class/thermal")? {
            let entry = entry?;
            let path = entry.path();
            
            if path.to_string_lossy().contains("thermal_zone") {
                if let Ok(temp) = std::fs::read_to_string(path.join("temp")) {
                    if let Ok(t) = temp.trim().parse::<f64>() {
                        core_temps.push(vec![SensorReading {
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs_f64(),
                            value: t / 1000.0,
                        }]);
                    }
                }
            }
        }

        self.temperatures.cpu_package = package_temp;
        self.temperatures.cpu_cores = core_temps;
        Ok(())
    }

    fn read_fan_speeds(&mut self) -> std::io::Result<()> {
        let mut cpu_fan = Vec::new();
        let mut case_fans = Vec::new();

        // Read from hwmon devices
        for entry in std::fs::read_dir("/sys/class/hwmon")? {
            let entry = entry?;
            let path = entry.path();
            
            if let Ok(name) = std::fs::read_to_string(path.join("name")) {
                let name = name.trim();
                
                // Check for different fan controllers
                match name {
                    "it8620" | "nct6775" | "nct6776" => {
                        // Common fan controllers
                        for i in 1..6 {
                            if let Ok(speed) = std::fs::read_to_string(
                                path.join(format!("fan{}_input", i))
                            ) {
                                if let Ok(s) = speed.trim().parse::<f64>() {
                                    let reading = SensorReading {
                                        timestamp: std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs_f64(),
                                        value: s,
                                    };
                                    
                                    if i == 1 {
                                        cpu_fan.push(reading);
                                    } else {
                                        while case_fans.len() < i - 1 {
                                            case_fans.push(Vec::new());
                                        }
                                        case_fans[i-2].push(reading);
                                    }
                                }
                            }
                        }
                    },
                    _ => continue,
                }
            }
        }

        self.cooling.cpu_fan_rpm = cpu_fan;
        self.cooling.case_fans_rpm = case_fans;
        Ok(())
    }

    fn read_pump_speeds(&mut self) -> std::io::Result<()> {
        let mut pump_speeds = Vec::new();

        // Try to read from common AIO cooler devices
        let common_paths = [
            "/sys/class/hwmon/hwmon*/device/pump1_input",
            "/sys/class/hwmon/hwmon*/device/fan3_input", // Some AIOs use fan3
        ];

        for glob_path in common_paths.iter() {
            if let Ok(entries) = glob::glob(glob_path) {
                for entry in entries.flatten() {
                    if let Ok(speed) = std::fs::read_to_string(&entry) {
                        if let Ok(s) = speed.trim().parse::<f64>() {
                            pump_speeds.push(SensorReading {
                                timestamp: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs_f64(),
                                value: s,
                            });
                        }
                    }
                }
            }
        }

        if !pump_speeds.is_empty() {
            self.cooling.pump_rpm = Some(pump_speeds);
        }
        Ok(())
    }

    fn gather_power_limits() -> std::io::Result<PowerLimits> {
        let mut pl1 = 0.0;
        let mut pl2 = 0.0;
        let mut tau = 0.0;

        // Try reading from intel-rapl
        if let Ok(entries) = std::fs::read_dir("/sys/class/powercap/intel-rapl") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.to_string_lossy().contains("0:0") {
                    if let Ok(constraint_0) = std::fs::read_to_string(
                        path.join("constraint_0_power_limit_uw")
                    ) {
                        pl1 = constraint_0.trim().parse::<f64>().unwrap_or(0.0) / 1_000_000.0;
                    }
                    if let Ok(constraint_1) = std::fs::read_to_string(
                        path.join("constraint_1_power_limit_uw")
                    ) {
                        pl2 = constraint_1.trim().parse::<f64>().unwrap_or(0.0) / 1_000_000.0;
                    }
                    if let Ok(time_window) = std::fs::read_to_string(
                        path.join("constraint_0_time_window_us")
                    ) {
                        tau = time_window.trim().parse::<f64>().unwrap_or(0.0) / 1_000_000.0;
                    }
                }
            }
        }

        // Try reading TDP from cpuinfo or sysfs
        let tdp = if let Ok(max_power) = std::fs::read_to_string(
            "/sys/class/powercap/intel-rapl/intel-rapl:0/constraint_0_max_power_uw"
        ) {
            max_power.trim().parse::<f64>().unwrap_or(0.0) / 1_000_000.0
        } else {
            0.0 // Could try other methods to determine TDP
        };

        Ok(PowerLimits {
            package_tdp: tdp,
            current_pl1: pl1,
            current_pl2: pl2,
            tau,
        })
    }
}