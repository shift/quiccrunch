use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;
use std::process::Command;
use sysinfo::{System, SystemExt, ProcessExt, CpuExt};
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error, debug};
use which::which;

pub struct SystemUtils;

impl SystemUtils {
    pub fn check_system_requirements() -> Result<SystemInfo> {
        let mut system = System::new_all();
        system.refresh_all();

        let info = SystemInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_cores: num_cpus::get(),
            total_memory_mb: system.total_memory() / (1024 * 1024),
            available_memory_mb: system.available_memory() / (1024 * 1024),
            cpu_usage_percent: system.global_cpu_info().cpu_usage(),
            load_average: system.load_average(),
            disk_space_gb: Self::get_disk_space_gb()?,
            network_interfaces: Self::get_network_interfaces(),
        };

        info!("System info: {:?}", info);
        Ok(info)
    }

    fn get_disk_space_gb() -> Result<f64> {
        let current_dir = std::env::current_dir()?;
        let metadata = std::fs::metadata(&current_dir)?;
        
        // This is a simplified approach - in a real implementation you'd use a proper disk space library
        Ok(100.0) // Placeholder
    }

    fn get_network_interfaces() -> Vec<String> {
        // This is a simplified approach - in a real implementation you'd use proper network interface detection
        vec!["lo".to_string(), "eth0".to_string()]
    }

    pub async fn wait_for_port(port: u16, timeout_secs: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            if Self::is_port_open(port).await {
                info!("Port {} is now open", port);
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;
        }

        anyhow::bail!("Port {} did not become available within {} seconds", port, timeout_secs);
    }

    async fn is_port_open(port: u16) -> bool {
        match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn kill_processes_by_name(name: &str) -> Result<()> {
        let mut system = System::new_all();
        system.refresh_all();

        let mut killed = 0;
        for (pid, process) in system.processes() {
            if let Some(process_name) = process.name() {
                if process_name.contains(name) {
                    debug!("Killing process {} (PID: {})", process_name, pid);
                    if let Err(e) = process.kill() {
                        warn!("Failed to kill process {} (PID: {}): {}", process_name, pid, e);
                    } else {
                        killed += 1;
                    }
                }
            }
        }

        info!("Killed {} processes matching '{}'", killed, name);
        Ok(())
    }

    pub fn check_command_exists(command: &str) -> bool {
        which(command).is_ok()
    }

    pub fn get_available_port() -> Result<u16> {
        use std::net::{TcpListener, Ipv4Addr, SocketAddrV4};
        
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }

    pub async fn measure_network_latency(host: &str, count: u32) -> Result<NetworkLatencyResult> {
        let output = Command::new("ping")
            .arg("-c")
            .arg(count.to_string())
            .arg(host)
            .output()?;

        if !output.status.success() {
            anyhow::bail!("Ping command failed: {}", String::from_utf8_lossy(&output.stderr));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_ping_output(&stdout, count)
    }

    fn parse_ping_output(output: &str, count: u32) -> Result<NetworkLatencyResult> {
        let lines: Vec<&str> = output.lines().collect();
        let mut latencies = Vec::new();

        for line in lines {
            if line.contains("time=") {
                if let Some(time_part) = line.split("time=").nth(1) {
                    if let Some(time_str) = time_part.split_whitespace().next() {
                        if let Ok(latency_ms) = time_str.parse::<f64>() {
                            latencies.push(latency_ms);
                        }
                    }
                }
            }
        }

        if latencies.is_empty() {
            anyhow::bail!("No latency measurements found in ping output");
        }

        let min = latencies.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = latencies.iter().fold(0.0, |a, &b| a.max(b));
        let mean = latencies.iter().sum::<f64>() / latencies.len() as f64;

        // Calculate percentiles
        let mut sorted_latencies = latencies.clone();
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p95_index = (sorted_latencies.len() as f64 * 0.95) as usize;
        let p99_index = (sorted_latencies.len() as f64 * 0.99) as usize;
        
        let p95 = sorted_latencies.get(p95_index).unwrap_or(&max);
        let p99 = sorted_latencies.get(p99_index).unwrap_or(&max);

        Ok(NetworkLatencyResult {
            host: "localhost".to_string(), // Would be passed as parameter
            packet_count: count,
            packets_received: latencies.len() as u32,
            packet_loss_percent: ((count as f64 - latencies.len() as f64) / count as f64) * 100.0,
            min_ms: min,
            max_ms: max,
            mean_ms: mean,
            p95_ms: *p95,
            p99_ms: *p99,
        })
    }

    pub fn generate_test_payload(size: usize, pattern: &PayloadPattern) -> Vec<u8> {
        match pattern {
            PayloadPattern::Random => {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                (0..size).map(|_| rng.gen()).collect()
            }
            PayloadPattern::Sequential => {
                (0..size).map(|i| (i % 256) as u8).collect()
            }
            PayloadPattern::Zeroes => vec![0; size],
            PayloadPattern::Repeating(pattern_str) => {
                let pattern_bytes = pattern_str.as_bytes();
                let mut payload = Vec::with_capacity(size);
                for i in 0..size {
                    payload.push(pattern_bytes[i % pattern_bytes.len()]);
                }
                payload
            }
        }
    }

    pub fn calculate_checksum(data: &[u8]) -> u32 {
        data.iter().fold(0u32, |acc, &byte| {
            acc.wrapping_mul(31).wrapping_add(byte as u32)
        })
    }

    pub async fn cleanup_temp_files(pattern: &str) -> Result<()> {
        let temp_dir = std::env::temp_dir();
        let mut entries = tokio::fs::read_dir(&temp_dir).await?;
        
        let mut cleaned = 0;
        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            if let Some(name_str) = file_name.to_str() {
                if name_str.contains(pattern) {
                    if let Err(e) = tokio::fs::remove_file(entry.path()).await {
                        warn!("Failed to remove temp file {:?}: {}", entry.path(), e);
                    } else {
                        cleaned += 1;
                    }
                }
            }
        }

        info!("Cleaned up {} temporary files matching '{}'", cleaned, pattern);
        Ok(())
    }

    pub fn validate_test_environment() -> Result<EnvironmentValidation> {
        let mut validation = EnvironmentValidation::default();

        // Check required commands
        let required_commands = ["cargo", "rustc", "ping"];
        for cmd in &required_commands {
            if !Self::check_command_exists(cmd) {
                validation.missing_commands.push(cmd.to_string());
            }
        }

        // Check system resources
        let system_info = Self::check_system_requirements()?;
        
        if system_info.total_memory_mb < 1024 {
            validation.warnings.push("Low memory detected (< 1GB)".to_string());
        }

        if system_info.cpu_cores < 2 {
            validation.warnings.push("Low CPU core count (< 2)".to_string());
        }

        // Check disk space
        if system_info.disk_space_gb < 1.0 {
            validation.warnings.push("Low disk space (< 1GB)".to_string());
        }

        validation.is_valid = validation.missing_commands.is_empty();
        Ok(validation)
    }
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub os: String,
    pub arch: String,
    pub cpu_cores: usize,
    pub total_memory_mb: u64,
    pub available_memory_mb: u64,
    pub cpu_usage_percent: f64,
    pub load_average: f64,
    pub disk_space_gb: f64,
    pub network_interfaces: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NetworkLatencyResult {
    pub host: String,
    pub packet_count: u32,
    pub packets_received: u32,
    pub packet_loss_percent: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

#[derive(Debug, Clone)]
pub enum PayloadPattern {
    Random,
    Sequential,
    Zeroes,
    Repeating(String),
}

#[derive(Debug, Clone, Default)]
pub struct EnvironmentValidation {
    pub is_valid: bool,
    pub missing_commands: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct ProcessManager {
    processes: std::collections::HashMap<String, std::process::Child>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: std::collections::HashMap::new(),
        }
    }

    pub fn start_process(&mut self, name: &str, command: &str, args: &[&str]) -> Result<()> {
        let mut cmd = Command::new(command);
        cmd.args(args);
        
        let child = cmd.spawn()?;
        self.processes.insert(name.to_string(), child);
        
        info!("Started process '{}' with command: {} {}", name, command, args.join(" "));
        Ok(())
    }

    pub fn stop_process(&mut self, name: &str) -> Result<()> {
        if let Some(mut child) = self.processes.remove(name) {
            match child.kill() {
                Ok(_) => info!("Stopped process '{}'", name),
                Err(e) => warn!("Failed to stop process '{}': {}", name, e),
            }
        } else {
            warn!("Process '{}' not found", name);
        }
        Ok(())
    }

    pub fn stop_all(&mut self) -> Result<()> {
        let process_names: Vec<String> = self.processes.keys().cloned().collect();
        for name in process_names {
            self.stop_process(&name)?;
        }
        Ok(())
    }

    pub fn is_running(&self, name: &str) -> bool {
        self.processes.contains_key(name)
    }

    pub fn wait_for_process(&mut self, name: &str, timeout_secs: u64) -> Result<std::process::ExitStatus> {
        if let Some(child) = self.processes.get_mut(name) {
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(timeout_secs);
            
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        self.processes.remove(name);
                        return Ok(status);
                    }
                    Ok(None) => {
                        if start.elapsed() > timeout {
                            anyhow::bail!("Process '{}' did not finish within {} seconds", name, timeout_secs);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        anyhow::bail!("Error waiting for process '{}': {}", name, e);
                    }
                }
            }
        } else {
            anyhow::bail!("Process '{}' not found", name);
        }
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        let _ = self.stop_all();
    }
}

pub struct NetworkUtils;

impl NetworkUtils {
    pub async fn test_connectivity(host: &str, port: u16, timeout_secs: u64) -> Result<bool> {
        let timeout = Duration::from_secs(timeout_secs);
        
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(format!("{}:{}", host, port))).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(_)) => Ok(false),
            Err(_) => Ok(false), // Timeout
        }
    }

    pub fn get_local_ip() -> Result<String> {
        use std::net::{UdpSocket, Ipv4Addr};
        
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("8.8.8.8:80")?;
        let local_addr = socket.local_addr()?;
        
        Ok(local_addr.ip().to_string())
    }

    pub async fn measure_bandwidth(host: &str, port: u16, duration_secs: u64, payload_size: usize) -> Result<BandwidthResult> {
        let start = std::time::Instant::now();
        let duration = Duration::from_secs(duration_secs);
        let payload = SystemUtils::generate_test_payload(payload_size, &PayloadPattern::Random);
        
        let mut total_bytes = 0u64;
        let mut messages = 0u64;
        
        // Connect to server
        let mut stream = tokio::net::TcpStream::connect(format!("{}:{}", host, port)).await?;
        
        while start.elapsed() < duration {
            let bytes_sent = tokio::io::AsyncWriteExt::write(&mut stream, &payload).await?;
            total_bytes += bytes_sent as u64;
            messages += 1;
            
            // Small delay to prevent overwhelming the network
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        
        let actual_duration = start.elapsed();
        let throughput_bps = total_bytes as f64 / actual_duration.as_secs_f64();
        let throughput_mbps = throughput_bps / (1024.0 * 1024.0);
        
        Ok(BandwidthResult {
            host: host.to_string(),
            port,
            duration_secs: actual_duration.as_secs_f64(),
            total_bytes,
            messages,
            throughput_bps,
            throughput_mbps,
            payload_size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BandwidthResult {
    pub host: String,
    pub port: u16,
    pub duration_secs: f64,
    pub total_bytes: u64,
    pub messages: u64,
    pub throughput_bps: f64,
    pub throughput_mbps: f64,
    pub payload_size: usize,
}