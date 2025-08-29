# Production Use Cases and Examples

This directory contains real-world examples demonstrating QuicCrunch in production scenarios.

## Table of Contents

### 🏢 Enterprise Use Cases
- **[Financial Trading Platform](trading-platform.md)** - Low-latency trading system
- **[Video Streaming Service](video-streaming.md)** - Real-time video delivery
- **[IoT Data Collection](iot-data.md)** - High-throughput sensor data ingestion
- **[Chat Application](chat-app.md)** - Real-time messaging platform
- **[File Synchronization](file-sync.md)** - Distributed file sync service

### 🔧 Technical Patterns
- **[Microservices Architecture](microservices.md)** - Service-to-service communication
- **[Event Streaming](event-streaming.md)** - Real-time event processing
- **[Data Pipeline](data-pipeline.md)** - Stream processing pipeline
- **[API Gateway](api-gateway.md)** - Request routing and aggregation
- **[Load Balancing](load-balancing.md)** - High-availability deployment

### 📊 Performance Examples
- **[High Throughput](high-throughput.md)** - Millions of requests per second
- **[Low Latency](low-latency.md)** - Sub-millisecond response times
- **[Large Scale](large-scale.md)** - Thousands of concurrent connections
- **[Mobile Clients](mobile-clients.md)** - Connection migration and 0-RTT

## Production Deployment Patterns

### 1. Multi-Service Architecture

```rust
// Central service registry pattern
use quiccrunch_core::registry::ServiceRegistry;
use std::sync::Arc;

#[rpc_service(name = "UserService")]
trait UserService {
    async fn get_user(&self, id: u64) -> RpcResult<User>;
    async fn create_user(&self, request: CreateUserRequest) -> RpcResult<User>;
}

#[rpc_service(name = "OrderService")]
trait OrderService {
    async fn create_order(&self, request: CreateOrderRequest) -> RpcResult<Order>;
    async fn get_order_history(&self, user_id: u64) -> RpcResult<RpcStreamSource<Order>>;
}

#[rpc_service(name = "PaymentService")]
trait PaymentService {
    async fn process_payment(&self, request: PaymentRequest) -> RpcResult<PaymentResult>;
    async fn get_payment_status(&self, payment_id: String) -> RpcResult<PaymentStatus>;
}

// Service composition for complex workflows
struct E2EOrderProcessor {
    user_service: Arc<UserServiceClient>,
    order_service: Arc<OrderServiceClient>,
    payment_service: Arc<PaymentServiceClient>,
}

impl E2EOrderProcessor {
    async fn process_complete_order(&self, request: CompleteOrderRequest) -> Result<OrderResult, ProcessingError> {
        // 1. Validate user
        let user = self.user_service.get_user(request.user_id).await
            .map_err(|e| ProcessingError::UserValidation(e))?;
            
        // 2. Create order
        let order = self.order_service.create_order(CreateOrderRequest {
            user_id: user.id,
            items: request.items,
            total: request.total,
        }).await.map_err(|e| ProcessingError::OrderCreation(e))?;
        
        // 3. Process payment
        let payment_result = self.payment_service.process_payment(PaymentRequest {
            order_id: order.id,
            amount: order.total,
            payment_method: request.payment_method,
        }).await.map_err(|e| ProcessingError::PaymentProcessing(e))?;
        
        Ok(OrderResult {
            order,
            payment_result,
            user,
        })
    }
}
```

### 2. Event-Driven Architecture

```rust
// Event streaming for decoupled architecture
#[rpc_service(name = "EventBus")]
trait EventBus {
    async fn publish_event(&self, event: Event) -> RpcResult<()>;
    async fn subscribe_to_events(&self, filters: EventFilters) -> RpcResult<RpcStreamSource<Event>>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub id: String,
    pub event_type: String,
    pub source: String,
    pub timestamp: u64,
    pub data: serde_json::Value,
}

// Producer service
struct OrderEventProducer {
    event_bus: Arc<EventBusClient>,
}

impl OrderEventProducer {
    async fn publish_order_created(&self, order: &Order) -> Result<(), EventError> {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: "order.created".to_string(),
            source: "order-service".to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: serde_json::to_value(order)?,
        };
        
        self.event_bus.publish_event(event).await
            .map_err(EventError::PublishFailed)?;
            
        Ok(())
    }
}

// Consumer service
struct InventoryEventConsumer {
    event_bus: Arc<EventBusClient>,
    inventory_service: Arc<InventoryService>,
}

impl InventoryEventConsumer {
    async fn start_consuming(&self) -> Result<(), EventError> {
        let filters = EventFilters {
            event_types: vec!["order.created".to_string()],
            sources: vec!["order-service".to_string()],
        };
        
        let mut event_stream = self.event_bus.subscribe_to_events(filters).await?;
        
        while let Some(event) = event_stream.recv().await? {
            if let Err(e) = self.handle_order_event(&event).await {
                log::error!("Failed to handle order event {}: {:?}", event.id, e);
                // Implement retry logic or dead letter queue
            }
        }
        
        Ok(())
    }
    
    async fn handle_order_event(&self, event: &Event) -> Result<(), EventError> {
        if event.event_type == "order.created" {
            let order: Order = serde_json::from_value(event.data.clone())?;
            
            // Reserve inventory for order items
            for item in &order.items {
                self.inventory_service.reserve_item(item.product_id, item.quantity).await?;
            }
        }
        
        Ok(())
    }
}
```

## Performance Optimization Examples

### High-Throughput Data Ingestion

```rust
// Optimized for maximum throughput
use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};

#[rpc_service(name = "DataIngestionService")]
trait DataIngestionService {
    async fn ingest_batch(&self, data: Vec<DataPoint>) -> RpcResult<IngestResult>;
    async fn ingest_stream(&self, data: RpcStreamSink<DataPoint>) -> RpcResult<RpcStreamSource<IngestResult>>;
}

struct HighThroughputIngestionService {
    batch_processor: Arc<BatchProcessor>,
    metrics: Arc<IngestMetrics>,
}

impl DataIngestionService for HighThroughputIngestionService {
    async fn ingest_stream(&self, mut data_stream: RpcStreamSink<DataPoint>) -> RpcResult<RpcStreamSource<IngestResult>> {
        let (result_sink, result_source) = RpcStreamSource::new();
        let batch_processor = self.batch_processor.clone();
        let metrics = self.metrics.clone();
        
        tokio::spawn(async move {
            let mut batch = Vec::with_capacity(1000);
            let mut batch_timeout = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                tokio::select! {
                    // Receive data points
                    data_point = data_stream.recv() => {
                        match data_point {
                            Ok(Some(point)) => {
                                batch.push(point);
                                
                                // Process when batch is full
                                if batch.len() >= 1000 {
                                    let current_batch = std::mem::take(&mut batch);
                                    let result = batch_processor.process_batch(current_batch).await;
                                    
                                    metrics.record_batch(result.processed_count);
                                    
                                    if result_sink.send(result).await.is_err() {
                                        break; // Client disconnected
                                    }
                                }
                            }
                            Ok(None) => {
                                // Stream ended, process remaining batch
                                if !batch.is_empty() {
                                    let result = batch_processor.process_batch(batch).await;
                                    let _ = result_sink.send(result).await;
                                }
                                break;
                            }
                            Err(e) => {
                                let _ = result_sink.send_error(e).await;
                                break;
                            }
                        }
                    }
                    
                    // Timeout-based processing
                    _ = batch_timeout.tick() => {
                        if !batch.is_empty() {
                            let current_batch = std::mem::take(&mut batch);
                            let result = batch_processor.process_batch(current_batch).await;
                            
                            metrics.record_batch(result.processed_count);
                            
                            if result_sink.send(result).await.is_err() {
                                break; // Client disconnected
                            }
                        }
                    }
                }
            }
        });
        
        Ok(result_source)
    }
}

struct BatchProcessor {
    database: Arc<dyn Database>,
    workers: usize,
}

impl BatchProcessor {
    async fn process_batch(&self, batch: Vec<DataPoint>) -> IngestResult {
        let chunk_size = (batch.len() + self.workers - 1) / self.workers;
        let chunks: Vec<_> = batch.chunks(chunk_size).collect();
        
        let mut handles = Vec::new();
        
        for chunk in chunks {
            let chunk = chunk.to_vec();
            let database = self.database.clone();
            
            handles.push(tokio::spawn(async move {
                database.insert_batch(chunk).await
            }));
        }
        
        let mut total_processed = 0;
        let mut errors = Vec::new();
        
        for handle in handles {
            match handle.await {
                Ok(Ok(count)) => total_processed += count,
                Ok(Err(e)) => errors.push(e.to_string()),
                Err(e) => errors.push(format!("Task error: {}", e)),
            }
        }
        
        IngestResult {
            processed_count: total_processed,
            error_count: errors.len() as u64,
            errors,
        }
    }
}
```

### Low-Latency Trading System

```rust
// Optimized for minimal latency
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[rpc_service(name = "TradingEngine")]
trait TradingEngine {
    #[rpc(timeout = "1ms")] // Ultra-low timeout
    async fn place_order(&self, order: TradeOrder) -> RpcResult<TradeResult>;
    
    #[rpc(streaming = true)]
    async fn market_data_stream(&self) -> RpcResult<RpcStreamSource<MarketData>>;
    
    #[rpc(timeout = "500us")]
    async fn get_position(&self, account: String, symbol: String) -> RpcResult<Position>;
}

struct HighFrequencyTradingEngine {
    order_book: Arc<LockFreeOrderBook>,
    position_cache: Arc<PositionCache>,
    market_data_publisher: Arc<MarketDataPublisher>,
    latency_metrics: Arc<LatencyMetrics>,
}

impl TradingEngine for HighFrequencyTradingEngine {
    async fn place_order(&self, order: TradeOrder) -> RpcResult<TradeResult> {
        let start_time = Instant::now();
        
        // Fast-path validation (no async calls)
        if !self.validate_order_fast(&order) {
            return Err(RpcError::InvalidRequest {
                message: "Order validation failed".to_string()
            });
        }
        
        // Lock-free order matching
        let trade_result = self.order_book.match_order(order).await?;
        
        // Update positions atomically
        self.position_cache.update_positions(&trade_result).await?;
        
        // Publish market data update
        self.market_data_publisher.publish_trade(&trade_result).await?;
        
        // Record latency metrics
        let latency = start_time.elapsed();
        self.latency_metrics.record_order_latency(latency);
        
        Ok(trade_result)
    }
    
    async fn market_data_stream(&self) -> RpcResult<RpcStreamSource<MarketData>> {
        // Pre-allocated stream for zero allocation in hot path
        let (sink, source) = RpcStreamSource::with_capacity(10000);
        
        self.market_data_publisher.subscribe(sink).await?;
        
        Ok(source)
    }
    
    async fn get_position(&self, account: String, symbol: String) -> RpcResult<Position> {
        // Cache lookup with atomic operations
        self.position_cache.get_position(&account, &symbol).await
            .ok_or_else(|| RpcError::NotFound {
                resource: format!("Position for {}/{}", account, symbol)
            })
    }
}

// Lock-free data structures for minimal contention
struct LockFreeOrderBook {
    buy_orders: crossbeam::queue::SegQueue<Order>,
    sell_orders: crossbeam::queue::SegQueue<Order>,
    last_price: AtomicU64,
}

impl LockFreeOrderBook {
    async fn match_order(&self, order: TradeOrder) -> RpcResult<TradeResult> {
        let start_nanos = Instant::now();
        
        match order.side {
            OrderSide::Buy => {
                // Try to match with sell orders
                while let Some(sell_order) = self.sell_orders.pop() {
                    if sell_order.price <= order.price {
                        let trade = self.execute_trade(&order, &sell_order);
                        return Ok(TradeResult {
                            trade_id: generate_trade_id(),
                            executed_price: sell_order.price,
                            executed_quantity: trade.quantity,
                            execution_time: start_nanos.elapsed().as_nanos() as u64,
                        });
                    } else {
                        // Put back non-matching order
                        self.sell_orders.push(sell_order);
                        break;
                    }
                }
                
                // No match, add to buy queue
                self.buy_orders.push(Order::from(order));
                Ok(TradeResult::no_execution())
            }
            OrderSide::Sell => {
                // Similar logic for sell orders
                // ... implementation details
                todo!()
            }
        }
    }
}

struct LatencyMetrics {
    order_latency_histogram: prometheus::Histogram,
    order_count: AtomicU64,
}

impl LatencyMetrics {
    fn record_order_latency(&self, latency: Duration) {
        self.order_latency_histogram.observe(latency.as_secs_f64());
        self.order_count.fetch_add(1, Ordering::Relaxed);
        
        // Log if latency exceeds threshold
        if latency > Duration::from_micros(500) {
            log::warn!("High latency order: {:?}", latency);
        }
    }
}
```

## Production Monitoring

### Comprehensive Metrics Collection

```rust
use prometheus::{Counter, Histogram, IntGauge, Registry};
use std::sync::Arc;

pub struct ProductionMetrics {
    // Request metrics
    pub requests_total: Counter,
    pub request_duration: Histogram,
    pub active_requests: IntGauge,
    
    // Connection metrics
    pub connections_total: Counter,
    pub active_connections: IntGauge,
    pub connection_duration: Histogram,
    
    // Error metrics
    pub errors_total: Counter,
    pub error_rate: Counter,
    
    // Business metrics
    pub business_operations: Counter,
    pub business_latency: Histogram,
}

impl ProductionMetrics {
    pub fn new(registry: &Registry) -> Result<Self, Box<dyn std::error::Error>> {
        let requests_total = Counter::with_opts(
            prometheus::Opts::new("requests_total", "Total requests processed")
                .namespace("quiccrunch")
                .subsystem("rpc")
        )?;
        
        let request_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new("request_duration_seconds", "Request duration")
                .namespace("quiccrunch")
                .subsystem("rpc")
                .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0])
        )?;
        
        // Register all metrics
        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;
        
        Ok(Self {
            requests_total,
            request_duration,
            // ... other metrics
        })
    }
    
    pub fn record_request(&self, method: &str, duration: Duration, status: &str) {
        self.requests_total
            .with_label_values(&[method, status])
            .inc();
            
        self.request_duration
            .with_label_values(&[method])
            .observe(duration.as_secs_f64());
    }
}

// Middleware for automatic metrics collection
pub struct MetricsMiddleware {
    metrics: Arc<ProductionMetrics>,
}

impl MetricsMiddleware {
    pub async fn wrap_request<F, T>(
        &self,
        method: &str,
        request: F,
    ) -> Result<T, RpcError>
    where
        F: Future<Output = Result<T, RpcError>>,
    {
        let start_time = Instant::now();
        self.metrics.active_requests.inc();
        
        let result = request.await;
        
        let duration = start_time.elapsed();
        let status = match &result {
            Ok(_) => "success",
            Err(_) => "error",
        };
        
        self.metrics.record_request(method, duration, status);
        self.metrics.active_requests.dec();
        
        result
    }
}
```

### Health Checks and Readiness

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: HealthState,
    pub timestamp: u64,
    pub version: String,
    pub uptime_seconds: u64,
    pub checks: Vec<HealthCheck>,
}

#[derive(Serialize, Deserialize)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthState,
    pub message: Option<String>,
    pub duration_ms: u64,
}

pub struct HealthMonitor {
    checks: Vec<Box<dyn HealthChecker + Send + Sync>>,
    start_time: Instant,
}

#[async_trait]
pub trait HealthChecker {
    async fn check(&self) -> HealthCheck;
    fn name(&self) -> &str;
}

// Database health check
pub struct DatabaseHealthChecker {
    database: Arc<dyn Database>,
}

#[async_trait]
impl HealthChecker for DatabaseHealthChecker {
    async fn check(&self) -> HealthCheck {
        let start = Instant::now();
        let name = self.name().to_string();
        
        match self.database.ping().await {
            Ok(_) => HealthCheck {
                name,
                status: HealthState::Healthy,
                message: None,
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => HealthCheck {
                name,
                status: HealthState::Unhealthy,
                message: Some(e.to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }
    
    fn name(&self) -> &str {
        "database"
    }
}

// External service health check
pub struct ExternalServiceHealthChecker {
    service_name: String,
    client: Arc<dyn ExternalServiceClient>,
}

#[async_trait]
impl HealthChecker for ExternalServiceHealthChecker {
    async fn check(&self) -> HealthCheck {
        let start = Instant::now();
        
        match self.client.health_check().await {
            Ok(_) => HealthCheck {
                name: self.service_name.clone(),
                status: HealthState::Healthy,
                message: None,
                duration_ms: start.elapsed().as_millis() as u64,
            },
            Err(e) => HealthCheck {
                name: self.service_name.clone(),
                status: HealthState::Unhealthy,
                message: Some(format!("External service error: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
            },
        }
    }
    
    fn name(&self) -> &str {
        &self.service_name
    }
}

impl HealthMonitor {
    pub async fn check_health(&self) -> HealthStatus {
        let mut checks = Vec::new();
        let mut overall_status = HealthState::Healthy;
        
        // Run all health checks in parallel
        let check_futures: Vec<_> = self.checks.iter()
            .map(|checker| checker.check())
            .collect();
            
        let check_results = futures::future::join_all(check_futures).await;
        
        for check in check_results {
            match check.status {
                HealthState::Unhealthy => overall_status = HealthState::Unhealthy,
                HealthState::Degraded if matches!(overall_status, HealthState::Healthy) => {
                    overall_status = HealthState::Degraded;
                }
                _ => {}
            }
            checks.push(check);
        }
        
        HealthStatus {
            status: overall_status,
            timestamp: chrono::Utc::now().timestamp() as u64,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            checks,
        }
    }
}
```

---

These production examples demonstrate how QuicCrunch can be used in real-world scenarios with proper error handling, monitoring, and optimization. Each example includes best practices for production deployment and operational considerations.