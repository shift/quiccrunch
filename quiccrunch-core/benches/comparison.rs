use std::time::{Duration, Instant};
use tonic::{transport::Server, Request, Response};
use tokio::sync::mpsc;
use bytes::Bytes;

// Test implementations
pub struct TestService {
    data_tx: mpsc::Sender<Bytes>,
}

#[tonic::async_trait]
impl test_proto::test_service_server::TestService for TestService {
    async fn send_message(
        &self,
        request: Request<test_proto::Message>,
    ) -> Result<Response<test_proto::Response>, tonic::Status> {
        let data = Bytes::from(request.into_inner().data);
        self.data_tx.send(data).await.map_err(|_| {
            tonic::Status::internal("Failed to process message")
        })?;
        
        Ok(Response::new(test_proto::Response {
            status: "ok".to_string(),
        }))
    }
}

pub async fn run_grpc_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, _rx) = mpsc::channel(1024);
    let svc = TestService { data_tx: tx };
    
    Server::builder()
        .add_service(test_proto::test_service_server::TestServiceServer::new(svc))
        .serve(addr.parse()?)
        .await?;
        
    Ok(())
}

// Test matrix configuration
const IMPLEMENTATIONS: &[(&str, fn() -> Box<dyn Transport>)] = &[
    ("QuicCrunch Standard", || Box::new(QuicTransport::new(Default::default()).unwrap())),
    ("QuicCrunch io_uring", || Box::new(IoUringOptimizedClient::new(1024, 65536, 256, true).unwrap())),
    ("gRPC", || Box::new(GrpcTestClient::new("http://[::1]:50051").unwrap())),
    ("raw QUIC", || Box::new(QuinnTestClient::new().unwrap())),
];

const TEST_SCENARIOS: &[(&str, TestConfig)] = &[
    // Small messages
    ("Tiny RPC (128B)", TestConfig {
        message_size: 128,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    ("Small RPC (512B)", TestConfig {
        message_size: 512,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    ("Medium RPC (1KB)", TestConfig {
        message_size: 1024,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    ("Large RPC (4KB)", TestConfig {
        message_size: 4096,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    ("XLarge RPC (16KB)", TestConfig {
        message_size: 16384,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    ("XXLarge RPC (64KB)", TestConfig {
        message_size: 65536,
        duration: Duration::from_secs(30),
        concurrent_streams: 1,
    }),
    // Streaming scenarios
    ("Small Stream (512B)", TestConfig {
        message_size: 512,
        duration: Duration::from_secs(30),
        concurrent_streams: 100,
    }),
    ("Medium Stream (4KB)", TestConfig {
        message_size: 4096,
        duration: Duration::from_secs(30),
        concurrent_streams: 100,
    }),
    ("Large Stream (16KB)", TestConfig {
        message_size: 16384,
        duration: Duration::from_secs(30),
        concurrent_streams: 100,
    }),
    // Bulk transfer tests
    ("Small Bulk (1MB)", TestConfig {
        message_size: 1048576,
        duration: Duration::from_secs(30),
        concurrent_streams: 10,
    }),
    ("Large Bulk (16MB)", TestConfig {
        message_size: 16777216,
        duration: Duration::from_secs(30),
        concurrent_streams: 10,
    }),
];

const CONCURRENCY_LEVELS: &[usize] = &[1, 4, 16, 64, 256, 1024];
const MESSAGE_PATTERNS: &[MessagePattern] = &[
    MessagePattern::Unary,
    MessagePattern::Streaming,
    MessagePattern::Bidirectional,
];

#[derive(Debug, Clone)]
struct TestConfig {
    message_size: usize,
    duration: Duration,
    concurrent_streams: usize,
}

#[derive(Debug, Clone)]
enum MessagePattern {
    Unary,
    Streaming,
    Bidirectional,
}

// Performance test harness
pub async fn run_performance_matrix() -> Result<TestReport, Box<dyn std::error::Error>> {
    let mut report = TestReport::new();
    
    for &(impl_name, create_transport) in IMPLEMENTATIONS {
        for &(scenario_name, config) in TEST_SCENARIOS {
            for &concurrency in CONCURRENCY_LEVELS {
                for pattern in MESSAGE_PATTERNS {
                    let transport = create_transport();
                    let mut test = PerformanceTest::new(
                        transport,
                        impl_name.to_string(),
                        scenario_name.to_string(),
                        config.clone(),
                        concurrency,
                        pattern.clone(),
                    );
                    
                    let results = test.run().await?;
                    report.add_results(results);
                }
            }
        }
    }
    
    report.generate()
}

struct PerformanceTest {
    transport: Box<dyn Transport>,
    impl_name: String,
    scenario_name: String,
    config: TestConfig,
    concurrency: usize,
    pattern: MessagePattern,
    metrics: PerformanceMetrics,
}

impl PerformanceTest {
    fn new(
        transport: Box<dyn Transport>,
        impl_name: String,
        scenario_name: String,
        config: TestConfig,
        concurrency: usize,
        pattern: MessagePattern,
    ) -> Self {
        Self {
            transport,
            impl_name,
            scenario_name,
            config,
            concurrency,
            pattern,
            metrics: PerformanceMetrics::default(),
        }
    }
    
    async fn run(&mut self) -> Result<TestResults, Box<dyn std::error::Error>> {
        let data = vec![0u8; self.config.message_size];
        let mut handles = Vec::new();
        let start = Instant::now();
        
        for _ in 0..self.concurrency {
            let transport = self.transport.clone();
            let data = data.clone();
            let config = self.config.clone();
            let pattern = self.pattern.clone();
            
            handles.push(tokio::spawn(async move {
                let mut stream_metrics = StreamMetrics::default();
                let end_time = Instant::now() + config.duration;
                
                while Instant::now() < end_time {
                    match pattern {
                        MessagePattern::Unary => {
                            let start = Instant::now();
                            transport.send_message(&data).await?;
                            stream_metrics.record_latency(start.elapsed(), config.message_size);
                        },
                        MessagePattern::Streaming => {
                            for _ in 0..100 {
                                let start = Instant::now();
                                transport.send_message(&data).await?;
                                stream_metrics.record_latency(start.elapsed(), config.message_size);
                            }
                        },
                        MessagePattern::Bidirectional => {
                            let (tx, mut rx) = transport.open_bi().await?;
                            loop {
                                let start = Instant::now();
                                tx.send_message(&data).await?;
                                rx.receive_message().await?;
                                stream_metrics.record_latency(start.elapsed(), config.message_size * 2); // Count both directions
                            }
                        }
                    }
                }
                
                Ok::<StreamMetrics, Box<dyn std::error::Error>>(stream_metrics)
            }));
        }
        
        // Collect results
        let mut stream_results = Vec::new();
        for handle in handles {
            stream_results.push(handle.await??);
        }
        
        Ok(TestResults {
            impl_name: self.impl_name.clone(),
            scenario_name: self.scenario_name.clone(),
            config: self.config.clone(),
            concurrency: self.concurrency,
            pattern: self.pattern.clone(),
            duration: start.elapsed(),
            stream_metrics: stream_results,
        })
    }
}

#[derive(Default)]
struct StreamMetrics {
    message_count: usize,
    latencies: Vec<Duration>,
    errors: usize,
    bytes_transferred: usize,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
    min_latency: Option<Duration>,
    max_latency: Option<Duration>,
    total_latency: Duration,
}

impl StreamMetrics {
    fn record_latency(&mut self, latency: Duration, message_size: usize) {
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self.end_time = Some(Instant::now());
        
        self.message_count += 1;
        self.bytes_transferred += message_size;
        self.latencies.push(latency);
        self.total_latency += latency;

        self.min_latency = Some(match self.min_latency {
            Some(min) => min.min(latency),
            None => latency,
        });
        
        self.max_latency = Some(match self.max_latency {
            Some(max) => max.max(latency),
            None => latency,
        });
    }

    fn calculate_stats(&self) -> StreamStats {
        let duration = self.end_time.unwrap()
            .duration_since(self.start_time.unwrap());
        
        let throughput = self.bytes_transferred as f64 / duration.as_secs_f64();
        let msg_rate = self.message_count as f64 / duration.as_secs_f64();
        
        let mut sorted_latencies = self.latencies.clone();
        sorted_latencies.sort_unstable();
        
        let p50 = sorted_latencies[self.latencies.len() / 2];
        let p95 = sorted_latencies[(self.latencies.len() * 95) / 100];
        let p99 = sorted_latencies[(self.latencies.len() * 99) / 100];
        
        StreamStats {
            message_count: self.message_count,
            bytes_transferred: self.bytes_transferred,
            duration,
            throughput,
            message_rate: msg_rate,
            min_latency: self.min_latency.unwrap(),
            max_latency: self.max_latency.unwrap(),
            avg_latency: self.total_latency / self.message_count as u32,
            p50_latency: p50,
            p95_latency: p95,
            p99_latency: p99,
            error_count: self.errors,
        }
    }
}

#[derive(Debug)]
struct StreamStats {
    message_count: usize,
    bytes_transferred: usize,
    duration: Duration,
    throughput: f64,
    message_rate: f64,
    min_latency: Duration,
    max_latency: Duration,
    avg_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    error_count: usize,
}

struct TestResults {
    impl_name: String,
    scenario_name: String,
    config: TestConfig,
    concurrency: usize,
    pattern: MessagePattern,
    duration: Duration,
    stream_metrics: Vec<StreamMetrics>,
}

struct TestReport {
    results: Vec<TestResults>,
}