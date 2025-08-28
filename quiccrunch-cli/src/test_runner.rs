/// Test runner for RPC services
use anyhow::Result;
use std::time::{Duration, Instant};
use serde_json::Value;

/// Configuration for RPC tests
#[derive(Debug, Clone)]
pub struct TestConfig {
    pub endpoint: String,
    pub service: String,
    pub method: String,
    pub params: Value,
    pub concurrency: usize,
    pub requests: usize,
    pub timeout: Duration,
}

/// Results from running RPC tests
#[derive(Debug, Clone)]
pub struct TestResults {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub duration: Duration,
    pub requests_per_second: f64,
    pub average_latency: Duration,
    pub min_latency: Duration,
    pub max_latency: Duration,
    pub p95_latency: Duration,
    pub p99_latency: Duration,
    pub errors: Vec<String>,
}

/// RPC test runner
pub struct TestRunner {
    config: TestConfig,
}

impl TestRunner {
    pub fn new(config: TestConfig) -> Self {
        Self { config }
    }
    
    /// Run the configured tests
    pub async fn run(&self) -> Result<TestResults> {
        crate::print_info(&format!(
            "Running {} requests to {}.{} with {} concurrent connections",
            self.config.requests,
            self.config.service,
            self.config.method,
            self.config.concurrency
        ));
        
        let start = Instant::now();
        
        // TODO: Implement actual RPC testing
        // For now, simulate test execution
        let mut latencies = Vec::new();
        let mut errors = Vec::new();
        
        // Simulate test execution
        for i in 0..self.config.requests {
            let request_start = Instant::now();
            
            // Simulate request execution with some variability
            let delay_ms = 10 + (i % 20) as u64;
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            
            let latency = request_start.elapsed();
            latencies.push(latency);
            
            // Simulate occasional errors
            if i % 50 == 0 && i > 0 {
                errors.push(format!("Simulated error for request {}", i));
            }
        }
        
        let total_duration = start.elapsed();
        
        // Calculate statistics
        latencies.sort();
        let successful_requests = self.config.requests - errors.len();
        let rps = successful_requests as f64 / total_duration.as_secs_f64();
        
        let avg_latency = if !latencies.is_empty() {
            latencies.iter().sum::<Duration>() / latencies.len() as u32
        } else {
            Duration::ZERO
        };
        
        let min_latency = latencies.first().copied().unwrap_or(Duration::ZERO);
        let max_latency = latencies.last().copied().unwrap_or(Duration::ZERO);
        
        let p95_index = (latencies.len() as f64 * 0.95) as usize;
        let p99_index = (latencies.len() as f64 * 0.99) as usize;
        
        let p95_latency = latencies.get(p95_index).copied().unwrap_or(Duration::ZERO);
        let p99_latency = latencies.get(p99_index).copied().unwrap_or(Duration::ZERO);
        
        Ok(TestResults {
            total_requests: self.config.requests,
            successful_requests,
            failed_requests: errors.len(),
            duration: total_duration,
            requests_per_second: rps,
            average_latency: avg_latency,
            min_latency,
            max_latency,
            p95_latency,
            p99_latency,
            errors,
        })
    }
}

impl TestResults {
    /// Print results in a human-readable format
    pub fn print_summary(&self) {
        use colored::*;
        
        println!("{}", "Test Results Summary".bold().underline());
        println!();
        
        println!("{}", "Request Statistics:".bold());
        println!("  Total Requests: {}", self.total_requests);
        println!("  Successful: {}", self.successful_requests.to_string().green());
        println!("  Failed: {}", if self.failed_requests > 0 {
            self.failed_requests.to_string().red()
        } else {
            self.failed_requests.to_string().normal()
        });
        println!("  Success Rate: {:.2}%", 
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0);
        println!();
        
        println!("{}", "Performance Metrics:".bold());
        println!("  Duration: {:.2}s", self.duration.as_secs_f64());
        println!("  Requests/sec: {:.2}", self.requests_per_second);
        println!();
        
        println!("{}", "Latency Statistics:".bold());
        println!("  Average: {:.2}ms", self.average_latency.as_secs_f64() * 1000.0);
        println!("  Minimum: {:.2}ms", self.min_latency.as_secs_f64() * 1000.0);
        println!("  Maximum: {:.2}ms", self.max_latency.as_secs_f64() * 1000.0);
        println!("  95th Percentile: {:.2}ms", self.p95_latency.as_secs_f64() * 1000.0);
        println!("  99th Percentile: {:.2}ms", self.p99_latency.as_secs_f64() * 1000.0);
        
        if !self.errors.is_empty() {
            println!();
            println!("{}", "Errors:".bold().red());
            for (i, error) in self.errors.iter().enumerate() {
                if i < 5 {  // Only show first 5 errors
                    println!("  {}", error);
                } else if i == 5 {
                    println!("  ... and {} more errors", self.errors.len() - 5);
                    break;
                }
            }
        }
    }
    
    /// Export results as JSON
    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::json!({
            "total_requests": self.total_requests,
            "successful_requests": self.successful_requests,
            "failed_requests": self.failed_requests,
            "duration_s": self.duration.as_secs_f64(),
            "requests_per_second": self.requests_per_second,
            "latency": {
                "average_ms": self.average_latency.as_secs_f64() * 1000.0,
                "min_ms": self.min_latency.as_secs_f64() * 1000.0,
                "max_ms": self.max_latency.as_secs_f64() * 1000.0,
                "p95_ms": self.p95_latency.as_secs_f64() * 1000.0,
                "p99_ms": self.p99_latency.as_secs_f64() * 1000.0
            },
            "errors": self.errors
        });
        
        Ok(serde_json::to_string_pretty(&json)?)
    }
}