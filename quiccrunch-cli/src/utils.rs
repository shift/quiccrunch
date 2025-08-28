/// Utility functions for the CLI
use anyhow::Result;
use std::path::Path;

/// Validate that a file exists and is readable
pub fn validate_file_exists(path: &str) -> Result<()> {
    if !Path::new(path).exists() {
        anyhow::bail!("File '{}' does not exist", path);
    }
    Ok(())
}

/// Validate that a directory exists or can be created
pub fn ensure_directory_exists(path: &str) -> Result<()> {
    std::fs::create_dir_all(path)
        .map_err(|e| anyhow::anyhow!("Failed to create directory '{}': {}", path, e))?;
    Ok(())
}

/// Parse an endpoint string into host and port
pub fn parse_endpoint(endpoint: &str) -> Result<(String, u16)> {
    if let Some((host, port_str)) = endpoint.rsplit_once(':') {
        let port = port_str.parse::<u16>()
            .map_err(|_| anyhow::anyhow!("Invalid port number: {}", port_str))?;
        Ok((host.to_string(), port))
    } else {
        anyhow::bail!("Endpoint must be in format 'host:port', got: {}", endpoint);
    }
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 1.0 {
        format!("{:.1}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.2}s", secs)
    } else {
        format!("{:.1}m", secs / 60.0)
    }
}

/// Format bytes in a human-readable way
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

/// Format requests per second
pub fn format_rps(rps: f64) -> String {
    if rps >= 1000.0 {
        format!("{:.1}k req/s", rps / 1000.0)
    } else {
        format!("{:.1} req/s", rps)
    }
}

/// Progress bar utility for long-running operations
pub struct ProgressBar {
    bar: indicatif::ProgressBar,
}

impl ProgressBar {
    pub fn new(total: u64, message: &str) -> Self {
        let bar = indicatif::ProgressBar::new(total);
        bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}/{len:7} {msg}")
                .expect("Invalid progress bar template")
                .progress_chars("##-")
        );
        bar.set_message(message.to_string());
        
        Self { bar }
    }
    
    pub fn inc(&self, delta: u64) {
        self.bar.inc(delta);
    }
    
    pub fn set_message(&self, message: &str) {
        self.bar.set_message(message.to_string());
    }
    
    pub fn finish_with_message(&self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }
}

/// Interactive prompts for user input
pub struct Prompts;

impl Prompts {
    /// Prompt for text input
    pub fn text(message: &str, default: Option<&str>) -> Result<String> {
        let mut prompt = inquire::Text::new(message);
        if let Some(default) = default {
            prompt = prompt.with_default(default);
        }
        Ok(prompt.prompt()?)
    }
    
    /// Prompt for confirmation (yes/no)
    pub fn confirm(message: &str, default: bool) -> Result<bool> {
        Ok(inquire::Confirm::new(message)
            .with_default(default)
            .prompt()?)
    }
    
    /// Prompt for selection from a list
    pub fn select(message: &str, options: Vec<&str>) -> Result<String> {
        Ok(inquire::Select::new(message, options).prompt()?.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_parse_endpoint() {
        assert_eq!(parse_endpoint("localhost:8080").unwrap(), ("localhost".to_string(), 8080));
        assert_eq!(parse_endpoint("127.0.0.1:3000").unwrap(), ("127.0.0.1".to_string(), 3000));
        assert!(parse_endpoint("invalid").is_err());
        assert!(parse_endpoint("localhost:abc").is_err());
    }
    
    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500.0ms");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.00s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1.5m");
    }
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(2_097_152), "2.0 MB");
    }
    
    #[test]
    fn test_format_rps() {
        assert_eq!(format_rps(123.4), "123.4 req/s");
        assert_eq!(format_rps(1234.5), "1.2k req/s");
    }
}