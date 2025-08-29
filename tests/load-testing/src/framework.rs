use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn, error, debug};

use crate::config::TestConfig;
use crate::metrics::{MetricsCollector, TestMetrics};
use crate::scenarios::{TestScenario, ScenarioRunner};
use crate::reporting::TestReporter;

pub struct LoadTestFramework {
    config: TestConfig,
    metrics_collector: Arc<MetricsCollector>,
    reporter: Arc<TestReporter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub scenario_name: String,
    #[serde(skip, default = "Instant::now")]
    pub start_time: Instant,
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
    pub metrics: TestMetrics,
    pub success: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestContext {
    pub scenario_name: String,
    pub start_time: Instant,
    pub config: TestConfig,
    pub metrics_tx: mpsc::UnboundedSender<MetricsEvent>,
}

#[derive(Debug, Clone)]
pub enum MetricsEvent {
    RequestStarted { request_id: String, timestamp: Instant },
    RequestCompleted { request_id: String, timestamp: Instant, success: bool, latency: Duration },
    ConnectionEstablished { connection_id: String, timestamp: Instant },
    ConnectionClosed { connection_id: String, timestamp: Instant, reason: String },
    ErrorOccurred { error_type: String, message: String, timestamp: Instant },
    StreamCreated { stream_id: String, timestamp: Instant },
    StreamClosed { stream_id: String, timestamp: Instant, bytes_transferred: u64 },
    CustomMetric { name: String, value: f64, timestamp: Instant },
}

impl LoadTestFramework {
    pub fn new(config: TestConfig) -> Self {
        let metrics_collector = Arc::new(MetricsCollector::new(
            config.performance.metrics_interval
        ));
        let reporter = Arc::new(TestReporter::new(config.reporting.clone()));

        Self {
            config,
            metrics_collector,
            reporter,
        }
    }

    pub async fn run_scenario(&self, scenario_name: &str) -> Result<TestResult> {
        info!("Starting load test scenario: {}", scenario_name);
        
        let scenario_config = self.config.get_scenario(scenario_name)?;
        let start_time = Instant::now();

        // Set up metrics collection
        let (metrics_tx, metrics_rx) = mpsc::unbounded_channel();
        let metrics_collector = Arc::clone(&self.metrics_collector);
        
        // Start metrics collection task
        let metrics_handle = tokio::spawn(async move {
            metrics_collector.collect_metrics(metrics_rx).await
        });

        // Create test context
        let context = TestContext {
            scenario_name: scenario_name.to_string(),
            start_time,
            config: self.config.clone(),
            metrics_tx: metrics_tx.clone(),
        };

        // Run scenario
        let scenario_result = self.run_scenario_with_context(scenario_config, context).await;
        
        // Stop metrics collection
        drop(metrics_tx);
        let metrics = metrics_handle.await
            .context("Failed to collect metrics")?
            .context("Metrics collection failed")?;

        let duration = start_time.elapsed();
        
        let result = match scenario_result {
            Ok(_) => TestResult {
                scenario_name: scenario_name.to_string(),
                start_time,
                duration,
                metrics,
                success: true,
                error_message: None,
            },
            Err(e) => {
                error!("Scenario failed: {}", e);
                TestResult {
                    scenario_name: scenario_name.to_string(),
                    start_time,
                    duration,
                    metrics,
                    success: false,
                    error_message: Some(e.to_string()),
                }
            }
        };

        // Generate report
        self.reporter.generate_report(&result).await
            .context("Failed to generate report")?;

        info!("Completed load test scenario: {} (success: {})", scenario_name, result.success);
        Ok(result)
    }

    async fn run_scenario_with_context(
        &self,
        scenario_config: &crate::config::ScenarioConfig,
        context: TestContext,
    ) -> Result<()> {
        // Warmup phase
        if self.config.performance.warmup_duration > Duration::ZERO {
            info!("Starting warmup phase ({:?})", self.config.performance.warmup_duration);
            self.run_warmup(&context).await
                .context("Warmup phase failed")?;
        }

        // Main test phase
        info!("Starting main test phase ({:?})", scenario_config.duration);
        let scenario = ScenarioRunner::new(scenario_config.clone());
        scenario.run(context.clone()).await
            .context("Main test phase failed")?;

        // Cooldown phase
        if self.config.performance.cooldown_duration > Duration::ZERO {
            info!("Starting cooldown phase ({:?})", self.config.performance.cooldown_duration);
            tokio::time::sleep(self.config.performance.cooldown_duration).await;
        }

        Ok(())
    }

    async fn run_warmup(&self, context: &TestContext) -> Result<()> {
        // Simple warmup: run a small subset of the main test
        let warmup_scenario = crate::config::ScenarioConfig {
            name: format!("{}-warmup", context.scenario_name),
            description: Some("Warmup phase".to_string()),
            duration: self.config.performance.warmup_duration,
            connections: 10, // Reduced connections for warmup
            request_rate: crate::config::RequestRateConfig::Constant { rate: 100 },
            message_size: crate::config::MessageSizeConfig::Fixed { size: 1024 },
            backend: "quinn".to_string(),
            streaming: None,
            network_conditions: None,
        };

        let scenario = ScenarioRunner::new(warmup_scenario);
        scenario.run(context.clone()).await
            .context("Warmup failed")?;

        Ok(())
    }

    pub async fn run_all_scenarios(&self) -> Result<Vec<TestResult>> {
        let mut results = Vec::new();

        for scenario_name in self.config.scenarios.keys() {
            match self.run_scenario(scenario_name).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Failed to run scenario {}: {}", scenario_name, e);
                    // Continue with other scenarios
                }
            }
        }

        Ok(results)
    }

    pub async fn run_regression_test(&self, baseline_name: &str, threshold: f64) -> Result<bool> {
        info!("Running regression test against baseline: {}", baseline_name);
        
        // Run current test
        let current_results = self.run_scenario("simple").await?;
        
        // Load baseline
        let baseline_metrics = self.load_baseline(baseline_name).await
            .context("Failed to load baseline")?;
        
        // Compare results
        let regression_detected = self.detect_regression(&current_results.metrics, &baseline_metrics, threshold)
            .context("Failed to detect regression")?;
        
        if regression_detected {
            warn!("Performance regression detected!");
        } else {
            info!("No performance regression detected");
        }
        
        Ok(!regression_detected)
    }

    async fn load_baseline(&self, baseline_name: &str) -> Result<TestMetrics> {
        match &self.config.regression.storage {
            crate::config::RegressionStorage::File { path } => {
                let baseline_path = format!("{}/{}.json", path, baseline_name);
                let content = tokio::fs::read_to_string(baseline_path).await
                    .context("Failed to read baseline file")?;
                
                serde_json::from_str(&content)
                    .context("Failed to parse baseline metrics")
            }
            crate::config::RegressionStorage::Database { url: _ } => {
                // TODO: Implement database storage
                anyhow::bail!("Database baseline storage not yet implemented")
            }
        }
    }

    fn detect_regression(&self, current: &TestMetrics, baseline: &TestMetrics, threshold: f64) -> Result<bool> {
        // Simple regression detection based on latency percentiles
        let current_p99 = current.latency_p99.as_secs_f64();
        let baseline_p99 = baseline.latency_p99.as_secs_f64();
        
        let change_ratio = (current_p99 - baseline_p99) / baseline_p99;
        
        debug!("Regression check: current_p99={:.3}ms, baseline_p99={:.3}ms, change={:.1}%", 
               current_p99 * 1000.0, baseline_p99 * 1000.0, change_ratio * 100.0);
        
        Ok(change_ratio > threshold)
    }

    pub async fn save_baseline(&self, baseline_name: &str, metrics: &TestMetrics) -> Result<()> {
        match &self.config.regression.storage {
            crate::config::RegressionStorage::File { path } => {
                tokio::fs::create_dir_all(path).await
                    .context("Failed to create baseline directory")?;
                
                let baseline_path = format!("{}/{}.json", path, baseline_name);
                let content = serde_json::to_string_pretty(metrics)
                    .context("Failed to serialize baseline metrics")?;
                
                tokio::fs::write(baseline_path, content).await
                    .context("Failed to write baseline file")?;
                
                info!("Saved baseline: {}", baseline_name);
                Ok(())
            }
            crate::config::RegressionStorage::Database { url: _ } => {
                // TODO: Implement database storage
                anyhow::bail!("Database baseline storage not yet implemented")
            }
        }
    }
}

#[async_trait]
pub trait TestRunner {
    async fn setup(&mut self) -> Result<()>;
    async fn run(&mut self, context: TestContext) -> Result<()>;
    async fn teardown(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestConfig;

    #[tokio::test]
    async fn test_framework_creation() {
        let config = TestConfig::default_config();
        let framework = LoadTestFramework::new(config);
        
        // Framework should be created successfully
        assert!(!framework.config.scenarios.is_empty());
    }

    #[tokio::test]
    async fn test_regression_detection() {
        let config = TestConfig::default_config();
        let framework = LoadTestFramework::new(config);
        
        let baseline = TestMetrics {
            latency_p99: Duration::from_millis(100),
            ..Default::default()
        };
        
        let current = TestMetrics {
            latency_p99: Duration::from_millis(120), // 20% increase
            ..Default::default()
        };
        
        let regression = framework.detect_regression(&current, &baseline, 0.1).unwrap();
        assert!(regression); // Should detect 20% increase with 10% threshold
        
        let no_regression = framework.detect_regression(&current, &baseline, 0.3).unwrap();
        assert!(!no_regression); // Should not detect with 30% threshold
    }
}