use crate::config::{BenchmarkConfig, ImplementationConfig, TestScenario, ImplementationType};
use crate::metrics::{BenchmarkResult, TestMetrics, SystemMetrics, ErrorMetrics};
use crate::storage::ResultStorage;
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use sysinfo::{System, SystemExt, ProcessExt, CpuExt};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{sleep, Duration, Instant};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TestEnvironment {
    pub test_id: String,
    pub start_time: DateTime<Utc>,
    pub config: BenchmarkConfig,
    pub working_directory: PathBuf,
}

#[async_trait]
pub trait TestOrchestrator: Send + Sync {
    async fn run_benchmark(&self, config: BenchmarkConfig) -> Result<Vec<BenchmarkResult>>;
    async fn run_single_test(&self, env: TestEnvironment, impl_config: ImplementationConfig, scenario: TestScenario) -> Result<BenchmarkResult>;
}

pub struct DefaultTestOrchestrator {
    storage: Arc<dyn ResultStorage>,
    max_parallel_tests: usize,
}

impl DefaultTestOrchestrator {
    pub fn new(storage: Arc<dyn ResultStorage>, max_parallel_tests: usize) -> Self {
        Self {
            storage,
            max_parallel_tests,
        }
    }

    async fn start_server(&self, impl_config: &ImplementationConfig, env: &TestEnvironment) -> Result<ServerHandle> {
        info!("Starting server for implementation: {}", impl_config.name);
        
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
           .arg(&impl_config.server_command);

        if let Some(working_dir) = &impl_config.working_directory {
            let full_path = env.working_directory.join(working_dir);
            cmd.current_dir(&full_path);
        }

        for (key, value) in &impl_config.environment {
            cmd.env(key, value);
        }

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to start server for {}", impl_config.name))?;

        let server_handle = ServerHandle {
            process: Arc::new(RwLock::new(Some(child))),
            name: impl_config.name.clone(),
            start_time: Instant::now(),
        };

        // Wait for server to be ready
        if let Some(health_url) = &impl_config.health_check_url {
            self.wait_for_health_check(health_url, impl_config.health_check_timeout_secs).await?;
        } else {
            // Fallback: wait a fixed time
            sleep(Duration::from_secs(5)).await;
        }

        info!("Server started successfully for implementation: {}", impl_config.name);
        Ok(server_handle)
    }

    async fn wait_for_health_check(&self, url: &str, timeout_secs: u64) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            if let Ok(response) = reqwest::get(url).await {
                if response.status().is_success() {
                    return Ok(());
                }
            }
            sleep(Duration::from_millis(500)).await;
        }

        anyhow::bail!("Health check failed after {} seconds", timeout_secs);
    }

    async fn stop_server(&self, server_handle: ServerHandle, impl_config: &ImplementationConfig) -> Result<()> {
        info!("Stopping server for implementation: {}", impl_config.name);
        
        let timeout = Duration::from_secs(impl_config.shutdown_timeout_secs);
        
        if let Some(mut process) = server_handle.process.write().await.take() {
            let start = Instant::now();
            
            // Try graceful shutdown first
            if let Err(e) = process.kill() {
                warn!("Failed to gracefully stop server {}: {}", impl_config.name, e);
            }

            // Wait for process to exit or timeout
            while start.elapsed() < timeout {
                match process.try_wait() {
                    Ok(Some(_)) => {
                        info!("Server {} stopped gracefully", impl_config.name);
                        return Ok(());
                    }
                    Ok(None) => {
                        sleep(Duration::from_millis(100)).await;
                    }
                    Err(e) => {
                        warn!("Error checking server {} status: {}", impl_config.name, e);
                        break;
                    }
                }
            }

            // Force kill if still running
            if let Err(e) = process.kill() {
                warn!("Failed to force kill server {}: {}", impl_config.name, e);
            }
        }

        info!("Server {} stopped", impl_config.name);
        Ok(())
    }

    async fn run_client_test(&self, impl_config: &ImplementationConfig, scenario: &TestScenario, env: &TestEnvironment) -> Result<TestMetrics> {
        info!("Running client test for implementation: {}", impl_config.name);

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
           .arg(&impl_config.client_command);

        if let Some(working_dir) = &impl_config.working_directory {
            let full_path = env.working_directory.join(working_dir);
            cmd.current_dir(&full_path);
        }

        // Add test parameters as environment variables
        cmd.env("BENCHMARK_MESSAGE_SIZE", &scenario.message_sizes[0].to_string())
           .env("BENCHMARK_CONCURRENCY", &scenario.concurrency_levels[0].to_string())
           .env("BENCHMARK_DURATION", &scenario.duration_secs.to_string())
           .env("BENCHMARK_WARMUP", &scenario.warmup_duration_secs.to_string())
           .env("BENCHMARK_CONNECTIONS", &scenario.connections_per_client.to_string());

        if let Some(rate_limit) = scenario.rate_limit {
            cmd.env("BENCHMARK_RATE_LIMIT", &rate_limit.to_string());
        }

        if let Some(messages_per_conn) = scenario.messages_per_connection {
            cmd.env("BENCHMARK_MESSAGES_PER_CONNECTION", &messages_per_conn.to_string());
        }

        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to start client for {}", impl_config.name))?;

        let start_time = Instant::now();
        
        // Monitor system resources during test
        let system_monitor = SystemMonitor::new(env.config.global_settings.system_monitoring_interval_ms);
        system_monitor.start().await;

        let result = child.wait().await;
        let test_duration = start_time.elapsed();

        let system_metrics = system_monitor.stop().await;

        match result {
            Ok(exit_status) => {
                if exit_status.success() {
                    info!("Client test completed successfully for {}", impl_config.name);
                    // Parse client output to extract metrics
                    // This is a placeholder - actual implementation would parse client output
                    Ok(TestMetrics {
                        total_messages: 1000, // Placeholder
                        total_bytes: 1024000, // Placeholder
                        duration: test_duration,
                        latency_min: Duration::from_micros(100),
                        latency_max: Duration::from_millis(10),
                        latency_mean: Duration::from_micros(500),
                        latency_p50: Duration::from_micros(400),
                        latency_p95: Duration::from_millis(2),
                        latency_p99: Duration::from_millis(5),
                        latency_p99_9: Duration::from_millis(8),
                        throughput_mps: 1000.0 / test_duration.as_secs_f64(),
                        throughput_bps: 1024000.0 / test_duration.as_secs_f64(),
                        connection_time: Duration::from_millis(50),
                        system_metrics,
                        errors: ErrorMetrics::default(),
                    })
                } else {
                    anyhow::bail!("Client test failed with exit status: {:?}", exit_status);
                }
            }
            Err(e) => {
                anyhow::bail!("Failed to wait for client process: {}", e);
            }
        }
    }

    async fn cleanup_between_tests(&self, env: &TestEnvironment) -> Result<()> {
        if env.config.global_settings.cleanup_between_tests {
            info!("Performing cleanup between tests");
            // Kill any remaining processes
            // Clean up temporary files
            // Reset network conditions if any were applied
            sleep(Duration::from_secs(2)).await;
        }
        Ok(())
    }
}

#[async_trait]
impl TestOrchestrator for DefaultTestOrchestrator {
    async fn run_benchmark(&self, config: BenchmarkConfig) -> Result<Vec<BenchmarkResult>> {
        info!("Starting benchmark with {} implementations and {} scenarios", 
              config.implementations.len(), config.test_scenarios.len());

        config.validate()?;
        
        let test_env = TestEnvironment {
            test_id: Uuid::new_v4().to_string(),
            start_time: Utc::now(),
            config: config.clone(),
            working_directory: std::env::current_dir()?,
        };

        let semaphore = Arc::new(Semaphore::new(self.max_parallel_tests));
        let mut results = Vec::new();

        for impl_config in &config.implementations {
            for scenario in &config.test_scenarios {
                let permit = semaphore.acquire().await?;
                let env = test_env.clone();
                let impl_config = impl_config.clone();
                let scenario = scenario.clone();

                let task = tokio::spawn(async move {
                    let _permit = permit;
                    self.run_single_test(env, impl_config, scenario).await
                });

                // In a real implementation, we'd collect results from tasks
                // For now, run sequentially
                drop(task);
                
                let result = self.run_single_test(test_env.clone(), impl_config.clone(), scenario.clone()).await?;
                results.push(result);

                self.cleanup_between_tests(&test_env).await?;
            }
        }

        info!("Benchmark completed with {} results", results.len());
        Ok(results)
    }

    async fn run_single_test(&self, env: TestEnvironment, impl_config: ImplementationConfig, scenario: TestScenario) -> Result<BenchmarkResult> {
        info!("Running test: {} - {} - {}", impl_config.name, scenario.name, env.test_id);

        let server_handle = self.start_server(&impl_config, &env).await?;
        
        let test_result = match self.run_client_test(&impl_config, &scenario, &env).await {
            Ok(metrics) => {
                info!("Test completed successfully for {} - {}", impl_config.name, scenario.name);
                BenchmarkResult {
                    test_id: env.test_id.clone(),
                    implementation_name: impl_config.name.clone(),
                    scenario_name: scenario.name.clone(),
                    start_time: env.start_time,
                    end_time: Utc::now(),
                    metrics,
                    success: true,
                    error_message: None,
                }
            }
            Err(e) => {
                error!("Test failed for {} - {}: {}", impl_config.name, scenario.name, e);
                BenchmarkResult {
                    test_id: env.test_id.clone(),
                    implementation_name: impl_config.name.clone(),
                    scenario_name: scenario.name.clone(),
                    start_time: env.start_time,
                    end_time: Utc::now(),
                    metrics: TestMetrics::default(),
                    success: false,
                    error_message: Some(e.to_string()),
                }
            }
        };

        self.stop_server(server_handle, &impl_config).await?;
        
        // Store result
        self.storage.store_result(&test_result).await?;

        Ok(test_result)
    }
}

#[derive(Debug)]
pub struct ServerHandle {
    process: Arc<RwLock<Option<Child>>>,
    name: String,
    start_time: Instant,
}

pub struct SystemMonitor {
    interval_ms: u64,
    system: Arc<RwLock<System>>,
    running: Arc<RwLock<bool>>,
}

impl SystemMonitor {
    pub fn new(interval_ms: u64) -> Self {
        Self {
            interval_ms,
            system: Arc::new(RwLock::new(System::new_all())),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self) {
        *self.running.write().await = true;
        let system = self.system.clone();
        let running = self.running.clone();
        let interval = Duration::from_millis(self.interval_ms);

        tokio::spawn(async move {
            while *running.read().await {
                system.write().await.refresh_all();
                sleep(interval).await;
            }
        });
    }

    pub async fn stop(&self) -> SystemMetrics {
        *self.running.write().await = false;
        let system = self.system.read().await;
        
        SystemMetrics {
            cpu_usage_percent: system.global_cpu_info().cpu_usage(),
            memory_used_mb: system.used_memory() / (1024 * 1024),
            memory_total_mb: system.total_memory() / (1024 * 1024),
            network_rx_bytes: 0, // Would need additional monitoring
            network_tx_bytes: 0,
            disk_usage_percent: 0.0,
            load_average: system.load_average(),
            process_count: system.processes().len(),
        }
    }
}