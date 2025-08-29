use anyhow::{Context, Result};
use async_trait::async_trait;
use rand::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::{interval, sleep};
use tracing::{info, debug, warn};

use crate::config::{ScenarioConfig, RequestRateConfig, MessageSizeConfig, StreamingConfig};
use crate::framework::{TestContext, MetricsEvent};
use crate::generators::{RequestGenerator, MessageGenerator, ConnectionGenerator};

#[derive(Debug, Clone)]
pub struct MockConnection {
    pub id: String,
    pub backend: String,
    pub established_at: Instant,
}

impl MockConnection {
    pub fn new(id: String, backend: String) -> Self {
        Self {
            id,
            backend,
            established_at: Instant::now(),
        }
    }
}

#[async_trait]
pub trait TestScenario {
    async fn run(&self, context: TestContext) -> Result<()>;
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
}

pub struct ScenarioRunner {
    config: ScenarioConfig,
    request_generator: Arc<RequestGenerator>,
    message_generator: Arc<MessageGenerator>,
    connection_generator: Arc<ConnectionGenerator>,
}

impl ScenarioRunner {
    pub fn new(config: ScenarioConfig) -> Self {
        let request_generator = Arc::new(RequestGenerator::new(config.request_rate.clone()));
        let message_generator = Arc::new(MessageGenerator::new(config.message_size.clone()));
        let connection_generator = Arc::new(ConnectionGenerator::new(config.connections));

        Self {
            config,
            request_generator,
            message_generator,
            connection_generator,
        }
    }
}

#[async_trait]
impl TestScenario for ScenarioRunner {
    async fn run(&self, context: TestContext) -> Result<()> {
        info!("Running scenario: {}", self.config.name);
        
        let start_time = Instant::now();
        let duration = self.config.duration;
        
        // Set up connections
        let connections = self.connection_generator.generate_connections().await
            .context("Failed to generate connections")?;
        
        info!("Established {} connections", connections.len());

        // Report connection establishment
        for (i, connection) in connections.iter().enumerate() {
            context.metrics_tx.send(MetricsEvent::ConnectionEstablished {
                connection_id: format!("conn-{}", i),
                timestamp: Instant::now(),
            }).ok();
        }

        // Run the main test loop
        if let Some(streaming_config) = &self.config.streaming {
            self.run_streaming_scenario(&context, &connections, streaming_config).await?;
        } else {
            self.run_request_response_scenario(&context, &connections).await?;
        }

        // Close connections
        for (i, _connection) in connections.iter().enumerate() {
            context.metrics_tx.send(MetricsEvent::ConnectionClosed {
                connection_id: format!("conn-{}", i),
                timestamp: Instant::now(),
                reason: "test_completed".to_string(),
            }).ok();
        }

        let elapsed = start_time.elapsed();
        info!("Scenario completed in {:?}", elapsed);
        
        Ok(())
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn description(&self) -> Option<&str> {
        self.config.description.as_deref()
    }
}

impl ScenarioRunner {
    async fn run_request_response_scenario(
        &self,
        context: &TestContext,
        connections: &[MockConnection],
    ) -> Result<()> {
        let start_time = Instant::now();
        let duration = self.config.duration;
        let mut request_timer = self.request_generator.create_timer().await?;
        
        let mut request_counter = 0u64;
        
        while start_time.elapsed() < duration {
            tokio::select! {
                _ = request_timer.tick() => {
                    let connection = &connections[request_counter as usize % connections.len()];
                    let message_size = self.message_generator.generate_size();
                    
                    let request_id = format!("req-{}", request_counter);
                    request_counter += 1;
                    
                    // Start request
                    context.metrics_tx.send(MetricsEvent::RequestStarted {
                        request_id: request_id.clone(),
                        timestamp: Instant::now(),
                    }).ok();
                    
                    // Simulate request processing
                    let success = self.simulate_request(connection, message_size).await?;
                    
                    // Complete request
                    let latency = self.calculate_request_latency(message_size);
                    context.metrics_tx.send(MetricsEvent::RequestCompleted {
                        request_id,
                        timestamp: Instant::now(),
                        success,
                        latency,
                    }).ok();
                }
                _ = sleep(duration) => break,
            }
        }
        
        info!("Completed {} requests", request_counter);
        Ok(())
    }

    async fn run_streaming_scenario(
        &self,
        context: &TestContext,
        connections: &[MockConnection],
        streaming_config: &StreamingConfig,
    ) -> Result<()> {
        info!("Running streaming scenario with {} streams", streaming_config.stream_count);
        
        let streams_per_connection = streaming_config.stream_count / connections.len() as u32;
        let mut stream_handles = Vec::new();
        
        for (conn_idx, connection) in connections.iter().enumerate() {
            for stream_idx in 0..streams_per_connection {
                let stream_id = format!("stream-{}-{}", conn_idx, stream_idx);
                let context = context.clone();
                let connection = connection.clone();
                let streaming_config = streaming_config.clone();
                
                let handle = tokio::spawn(async move {
                    Self::run_single_stream(stream_id, context, connection, streaming_config).await
                });
                
                stream_handles.push(handle);
            }
        }
        
        // Wait for all streams to complete
        for handle in stream_handles {
            if let Err(e) = handle.await {
                warn!("Stream task failed: {}", e);
            }
        }
        
        Ok(())
    }

    async fn run_single_stream(
        stream_id: String,
        context: TestContext,
        connection: MockConnection,
        streaming_config: StreamingConfig,
    ) -> Result<()> {
        let start_time = Instant::now();
        
        // Report stream creation
        context.metrics_tx.send(MetricsEvent::StreamCreated {
            stream_id: stream_id.clone(),
            timestamp: start_time,
        }).ok();
        
        let mut bytes_transferred = 0u64;
        let mut chunk_interval = interval(Duration::from_millis(10)); // 100Hz
        
        while start_time.elapsed() < streaming_config.stream_duration {
            chunk_interval.tick().await;
            
            // Simulate sending/receiving a chunk
            let chunk_size = streaming_config.chunk_size;
            let success = Self::simulate_stream_chunk(&connection, chunk_size).await?;
            
            if success {
                bytes_transferred += chunk_size as u64;
            }
            
            if streaming_config.bidirectional {
                // Simulate bidirectional traffic
                let success = Self::simulate_stream_chunk(&connection, chunk_size).await?;
                if success {
                    bytes_transferred += chunk_size as u64;
                }
            }
        }
        
        // Report stream closure
        context.metrics_tx.send(MetricsEvent::StreamClosed {
            stream_id,
            timestamp: Instant::now(),
            bytes_transferred,
        }).ok();
        
        debug!("Stream completed, transferred {} bytes", bytes_transferred);
        Ok(())
    }

    async fn simulate_request(&self, connection: &MockConnection, message_size: usize) -> Result<bool> {
        // Simulate network delay based on message size
        let base_delay = Duration::from_micros(100);
        let size_delay = Duration::from_nanos(message_size as u64 / 10); // ~100 MB/s
        let total_delay = base_delay + size_delay;
        
        sleep(total_delay).await;
        
        // Simulate some failure rate
        let success_rate = 0.99; // 99% success rate
        Ok(thread_rng().gen::<f64>() < success_rate)
    }

    fn calculate_request_latency(&self, message_size: usize) -> Duration {
        // Simulate realistic latency based on message size
        let base_latency = Duration::from_micros(500);
        let size_latency = Duration::from_nanos(message_size as u64 / 5); // ~200 MB/s
        
        // Add some random jitter
        let jitter = Duration::from_micros(thread_rng().gen_range(0..=100));
        
        base_latency + size_latency + jitter
    }

    async fn simulate_stream_chunk(connection: &MockConnection, chunk_size: usize) -> Result<bool> {
        // Simulate streaming chunk processing
        let delay = Duration::from_nanos(chunk_size as u64 / 20); // ~400 MB/s
        sleep(delay).await;
        
        // Very high success rate for streaming chunks
        Ok(thread_rng().gen::<f64>() < 0.999)
    }
}



// Built-in scenario implementations
pub struct SimpleLoadScenario;
pub struct RealisticWorkloadScenario;
pub struct StreamingRpcScenario;
pub struct BurstTrafficScenario;
pub struct StressTestScenario;

#[async_trait]
impl TestScenario for SimpleLoadScenario {
    async fn run(&self, context: TestContext) -> Result<()> {
        let config = ScenarioConfig {
            name: "Simple Load Test".to_string(),
            description: Some("Basic functionality validation under load".to_string()),
            duration: Duration::from_secs(60),
            connections: 100,
            request_rate: RequestRateConfig::Constant { rate: 1000 },
            message_size: MessageSizeConfig::Fixed { size: 1024 },
            backend: "quinn".to_string(),
            streaming: None,
            network_conditions: None,
        };
        
        let runner = ScenarioRunner::new(config);
        runner.run(context).await
    }

    fn name(&self) -> &str {
        "simple"
    }

    fn description(&self) -> Option<&str> {
        Some("Basic functionality validation under load")
    }
}

#[async_trait]
impl TestScenario for RealisticWorkloadScenario {
    async fn run(&self, context: TestContext) -> Result<()> {
        let config = ScenarioConfig {
            name: "Realistic Workload".to_string(),
            description: Some("Production-like traffic patterns".to_string()),
            duration: Duration::from_secs(300),
            connections: 500,
            request_rate: RequestRateConfig::Variable {
                pattern: "sine".to_string(),
                base_rate: 2000,
                variation: 0.3,
            },
            message_size: MessageSizeConfig::Distribution {
                pattern: "realistic".to_string(),
            },
            backend: "quinn".to_string(),
            streaming: None,
            network_conditions: None,
        };
        
        let runner = ScenarioRunner::new(config);
        runner.run(context).await
    }

    fn name(&self) -> &str {
        "realistic"
    }

    fn description(&self) -> Option<&str> {
        Some("Production-like traffic patterns")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TestConfig;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_simple_scenario() {
        let scenario = SimpleLoadScenario;
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        let context = TestContext {
            scenario_name: scenario.name().to_string(),
            start_time: Instant::now(),
            config: TestConfig::default_config(),
            metrics_tx: tx,
        };
        
        // Run a very short test
        let config = ScenarioConfig {
            duration: Duration::from_millis(100), // Very short for test
            ..TestConfig::default_config().scenarios["simple"].clone()
        };
        
        let runner = ScenarioRunner::new(config);
        let result = runner.run(context).await;
        
        assert!(result.is_ok());
        
        // Check that some events were generated
        let mut event_count = 0;
        while rx.try_recv().is_ok() {
            event_count += 1;
        }
        
        assert!(event_count > 0);
    }

    #[tokio::test]
    async fn test_scenario_runner_creation() {
        let config = TestConfig::default_config().scenarios["simple"].clone();
        let runner = ScenarioRunner::new(config);
        
        assert_eq!(runner.name(), "Simple Load Test");
    }
}