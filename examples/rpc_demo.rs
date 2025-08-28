//! Complete RPC Demo using QuicCrunch
//! 
//! This example demonstrates the full RPC framework including service discovery,
//! load balancing, and multiple services.

use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use quiccrunch::{
    QuicTransport, TlsConfig,
    rpc::{RpcServer, RpcClient, RpcService, RpcContext, RpcResult, RpcError},
    rpc::discovery::{ServiceRegistry, DiscoveryClient, LoadBalancingStrategy},
};
use bytes::Bytes;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
struct CalculationRequest {
    a: f64,
    b: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CalculationResult {
    result: f64,
}

/// Calculator service
struct CalculatorService;

#[async_trait]
impl RpcService for CalculatorService {
    fn name(&self) -> &str {
        "CalculatorService"
    }
    
    async fn call_method(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        match context.method.as_str() {
            "add" => {
                let req: CalculationRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let result = CalculationResult { result: req.a + req.b };
                let response = serde_json::to_vec(&result)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            "multiply" => {
                let req: CalculationRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let result = CalculationResult { result: req.a * req.b };
                let response = serde_json::to_vec(&result)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            "divide" => {
                let req: CalculationRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                if req.b == 0.0 {
                    return Err(RpcError::InvalidParameters("Division by zero".to_string()));
                }
                
                let result = CalculationResult { result: req.a / req.b };
                let response = serde_json::to_vec(&result)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            _ => Err(RpcError::MethodNotFound(context.method.clone())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["add".to_string(), "multiply".to_string(), "divide".to_string()]
    }
}

/// Weather service (mock implementation)
struct WeatherService;

#[async_trait]
impl RpcService for WeatherService {
    fn name(&self) -> &str {
        "WeatherService"
    }
    
    async fn call_method(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        match context.method.as_str() {
            "getTemperature" => {
                let request: serde_json::Value = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let city = request["city"].as_str()
                    .ok_or_else(|| RpcError::InvalidParameters("Missing city".to_string()))?;
                
                // Mock temperature based on city name hash
                let temp = (city.len() * 7 + 15) as f64;
                let result = serde_json::json!({
                    "city": city,
                    "temperature": temp,
                    "unit": "Celsius"
                });
                
                Ok(Bytes::from(result.to_string()))
            }
            "getForecast" => {
                let request: serde_json::Value = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let city = request["city"].as_str()
                    .ok_or_else(|| RpcError::InvalidParameters("Missing city".to_string()))?;
                
                let days = request["days"].as_u64().unwrap_or(3);
                
                let mut forecast = Vec::new();
                for i in 0..days {
                    forecast.push(serde_json::json!({
                        "day": i + 1,
                        "temperature": (city.len() * 7 + 15 + i as usize) as f64,
                        "condition": if i % 2 == 0 { "Sunny" } else { "Cloudy" }
                    }));
                }
                
                let result = serde_json::json!({
                    "city": city,
                    "forecast": forecast
                });
                
                Ok(Bytes::from(result.to_string()))
            }
            _ => Err(RpcError::MethodNotFound(context.method.clone())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["getTemperature".to_string(), "getForecast".to_string()]
    }
}

async fn start_rpc_server(addr: &str, registry: Arc<ServiceRegistry>) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting RPC server on {}", addr);
    
    // Create TLS configuration
    let tls_config = TlsConfig::default();
    
    // Create transport and start listening
    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen(addr).await?;
    
    // Create RPC server and register services
    let rpc_server = RpcServer::new();
    
    let calculator_service = Arc::new(CalculatorService);
    let weather_service = Arc::new(WeatherService);
    
    rpc_server.register_service(calculator_service.clone()).await;
    rpc_server.register_service(weather_service.clone()).await;
    
    // Register services with discovery
    let mut calc_info = calculator_service.service_info();
    calc_info.endpoints.push(quiccrunch::rpc::EndpointInfo {
        address: addr.split(':').next().unwrap_or("127.0.0.1").to_string(),
        port: addr.split(':').nth(1).unwrap_or("50051").parse().unwrap_or(50051),
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    });
    
    let mut weather_info = weather_service.service_info();
    weather_info.endpoints.push(quiccrunch::rpc::EndpointInfo {
        address: addr.split(':').next().unwrap_or("127.0.0.1").to_string(),
        port: addr.split(':').nth(1).unwrap_or("50051").parse().unwrap_or(50051),
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    });
    
    registry.register_service(calc_info, quiccrunch::rpc::EndpointInfo {
        address: addr.split(':').next().unwrap_or("127.0.0.1").to_string(),
        port: addr.split(':').nth(1).unwrap_or("50051").parse().unwrap_or(50051),
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    }).await?;
    
    registry.register_service(weather_info, quiccrunch::rpc::EndpointInfo {
        address: addr.split(':').next().unwrap_or("127.0.0.1").to_string(),
        port: addr.split(':').nth(1).unwrap_or("50051").parse().unwrap_or(50051),
        protocol: "quic".to_string(),
        weight: 100,
        health: quiccrunch::schema::rpc_capnp::HealthStatus::Healthy,
    }).await?;
    
    println!("✅ Services registered with discovery:");
    println!("   - CalculatorService: add, multiply, divide");
    println!("   - WeatherService: getTemperature, getForecast");
    
    // Send periodic heartbeats
    let registry_heartbeat = registry.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            let _ = registry_heartbeat.heartbeat("CalculatorService").await;
            let _ = registry_heartbeat.heartbeat("WeatherService").await;
        }
    });
    
    // Handle incoming connections
    while let Ok(connection) = incoming.accept().await {
        let rpc_server = rpc_server.clone();
        tokio::spawn(async move {
            if let Err(e) = rpc_server.handle_connection(connection).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
    
    Ok(())
}

async fn run_client_demo(registry: Arc<ServiceRegistry>) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Starting client demo with service discovery...");
    
    // Wait a bit for server to register services
    sleep(Duration::from_millis(500)).await;
    
    // Create discovery client
    let discovery = DiscoveryClient::new(registry, LoadBalancingStrategy::RoundRobin);
    
    // Discover services
    let services = discovery.list_services().await;
    println!("📋 Discovered {} services:", services.len());
    for service in &services {
        println!("   - {}: {:?}", service.name, service.methods);
    }
    
    // Test calculator service
    if let Some(endpoint) = discovery.find_service_endpoint("CalculatorService").await {
        println!("🧮 Testing CalculatorService at {}:{}", endpoint.address, endpoint.port);
        
        let server_addr = format!("{}:{}", endpoint.address, endpoint.port);
        let tls_config = TlsConfig::default();
        let transport = QuicTransport::new(tls_config)?;
        let connection = transport.connect(&server_addr).await?;
        let client = RpcClient::new(connection);
        
        // Test addition
        let add_request = CalculationRequest { a: 10.0, b: 5.0 };
        let params = Bytes::from(serde_json::to_string(&add_request)?);
        let result = client.call("CalculatorService", "add", params).await?;
        let add_result: CalculationResult = serde_json::from_slice(&result)?;
        println!("   ✅ 10.0 + 5.0 = {}", add_result.result);
        
        // Test multiplication
        let mul_request = CalculationRequest { a: 7.0, b: 8.0 };
        let params = Bytes::from(serde_json::to_string(&mul_request)?);
        let result = client.call("CalculatorService", "multiply", params).await?;
        let mul_result: CalculationResult = serde_json::from_slice(&result)?;
        println!("   ✅ 7.0 * 8.0 = {}", mul_result.result);
        
        // Test division
        let div_request = CalculationRequest { a: 15.0, b: 3.0 };
        let params = Bytes::from(serde_json::to_string(&div_request)?);
        let result = client.call("CalculatorService", "divide", params).await?;
        let div_result: CalculationResult = serde_json::from_slice(&result)?;
        println!("   ✅ 15.0 / 3.0 = {}", div_result.result);
    }
    
    // Test weather service
    if let Some(endpoint) = discovery.find_service_endpoint("WeatherService").await {
        println!("🌤️  Testing WeatherService at {}:{}", endpoint.address, endpoint.port);
        
        let server_addr = format!("{}:{}", endpoint.address, endpoint.port);
        let tls_config = TlsConfig::default();
        let transport = QuicTransport::new(tls_config)?;
        let connection = transport.connect(&server_addr).await?;
        let client = RpcClient::new(connection);
        
        // Test temperature
        let temp_request = serde_json::json!({ "city": "New York" });
        let params = Bytes::from(temp_request.to_string());
        let result = client.call("WeatherService", "getTemperature", params).await?;
        let temp_result: serde_json::Value = serde_json::from_slice(&result)?;
        println!("   ✅ Temperature in {}: {}°C", 
                temp_result["city"], temp_result["temperature"]);
        
        // Test forecast
        let forecast_request = serde_json::json!({ "city": "London", "days": 5 });
        let params = Bytes::from(forecast_request.to_string());
        let result = client.call("WeatherService", "getForecast", params).await?;
        let forecast_result: serde_json::Value = serde_json::from_slice(&result)?;
        println!("   ✅ 5-day forecast for {}:", forecast_result["city"]);
        if let Some(forecast) = forecast_result["forecast"].as_array() {
            for day in forecast {
                println!("      Day {}: {}°C, {}", 
                        day["day"], day["temperature"], day["condition"]);
            }
        }
    }
    
    println!("🎉 RPC demo completed successfully!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("🚀 QuicCrunch RPC Framework Demo");
    println!("================================");
    
    // Create service registry
    let registry = Arc::new(ServiceRegistry::new());
    registry.start().await;
    
    let server_addr = "127.0.0.1:50051";
    
    // Start server in background
    let server_registry = registry.clone();
    tokio::spawn(async move {
        if let Err(e) = start_rpc_server(server_addr, server_registry).await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Wait for server to start
    sleep(Duration::from_millis(1000)).await;
    
    // Run client demo
    run_client_demo(registry).await?;
    
    println!("\n📊 Performance characteristics:");
    println!("   - Zero-copy Cap'n Proto serialization");
    println!("   - QUIC multiplexed streams");
    println!("   - Built-in service discovery");
    println!("   - Load balancing support");
    println!("   - Connection pooling");
    
    Ok(())
}