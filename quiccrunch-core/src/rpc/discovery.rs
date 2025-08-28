//! Service Discovery for QuicCrunch RPC
//! 
//! Provides service registration, discovery, and health monitoring capabilities.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, broadcast};
use tokio::time::interval;
use crate::rpc::{ServiceInfo, EndpointInfo, RpcResult, RpcError};
use crate::schema::rpc_capnp::*;

/// Service registry that manages service discovery
pub struct ServiceRegistry {
    services: Arc<RwLock<HashMap<String, RegisteredService>>>,
    health_check_interval: Duration,
    service_ttl: Duration,
    event_sender: broadcast::Sender<RegistryEvent>,
}

#[derive(Debug, Clone)]
struct RegisteredService {
    info: ServiceInfo,
    last_heartbeat: SystemTime,
    health_status: HealthStatus,
}

#[derive(Debug, Clone)]
pub enum RegistryEvent {
    ServiceRegistered(ServiceInfo),
    ServiceUnregistered(String),
    ServiceHealthChanged(String, HealthStatus),
    ServiceUpdated(ServiceInfo),
}

impl ServiceRegistry {
    /// Create a new service registry
    pub fn new() -> Self {
        let (event_sender, _) = broadcast::channel(100);
        
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            health_check_interval: Duration::from_secs(30),
            service_ttl: Duration::from_secs(120),
            event_sender,
        }
    }
    
    /// Start the registry background tasks
    pub async fn start(&self) {
        let services = self.services.clone();
        let event_sender = self.event_sender.clone();
        let health_check_interval = self.health_check_interval;
        let service_ttl = self.service_ttl;
        
        tokio::spawn(async move {
            let mut interval = interval(health_check_interval);
            loop {
                interval.tick().await;
                Self::check_service_health(services.clone(), event_sender.clone(), service_ttl).await;
            }
        });
    }
    
    /// Register a service
    pub async fn register_service(&self, mut service_info: ServiceInfo, endpoint: EndpointInfo) -> RpcResult<()> {
        service_info.endpoints.push(endpoint);
        
        let registered_service = RegisteredService {
            info: service_info.clone(),
            last_heartbeat: SystemTime::now(),
            health_status: HealthStatus::Healthy,
        };
        
        let mut services = self.services.write().await;
        services.insert(service_info.name.clone(), registered_service);
        
        // Notify listeners
        let _ = self.event_sender.send(RegistryEvent::ServiceRegistered(service_info));
        
        Ok(())
    }
    
    /// Unregister a service
    pub async fn unregister_service(&self, service_name: &str) -> RpcResult<()> {
        let mut services = self.services.write().await;
        
        if services.remove(service_name).is_some() {
            let _ = self.event_sender.send(RegistryEvent::ServiceUnregistered(service_name.to_string()));
            Ok(())
        } else {
            Err(RpcError::MethodNotFound(format!("Service not found: {}", service_name)))
        }
    }
    
    /// Update service information
    pub async fn update_service(&self, service_info: ServiceInfo) -> RpcResult<()> {
        let mut services = self.services.write().await;
        
        if let Some(registered_service) = services.get_mut(&service_info.name) {
            registered_service.info = service_info.clone();
            registered_service.last_heartbeat = SystemTime::now();
            
            let _ = self.event_sender.send(RegistryEvent::ServiceUpdated(service_info));
            Ok(())
        } else {
            Err(RpcError::MethodNotFound(format!("Service not found: {}", service_info.name)))
        }
    }
    
    /// Send heartbeat for a service
    pub async fn heartbeat(&self, service_name: &str) -> RpcResult<()> {
        let mut services = self.services.write().await;
        
        if let Some(registered_service) = services.get_mut(service_name) {
            registered_service.last_heartbeat = SystemTime::now();
            
            // If service was unhealthy, mark as healthy again
            if registered_service.health_status != HealthStatus::Healthy {
                registered_service.health_status = HealthStatus::Healthy;
                let _ = self.event_sender.send(RegistryEvent::ServiceHealthChanged(
                    service_name.to_string(),
                    HealthStatus::Healthy,
                ));
            }
            
            Ok(())
        } else {
            Err(RpcError::MethodNotFound(format!("Service not found: {}", service_name)))
        }
    }
    
    /// Discover services by name pattern
    pub async fn discover_services(&self, name_pattern: Option<&str>) -> Vec<ServiceInfo> {
        let services = self.services.read().await;
        
        services
            .values()
            .filter(|service| {
                service.health_status == HealthStatus::Healthy &&
                name_pattern.map_or(true, |pattern| service.info.name.contains(pattern))
            })
            .map(|service| service.info.clone())
            .collect()
    }
    
    /// Get specific service information
    pub async fn get_service(&self, service_name: &str) -> Option<ServiceInfo> {
        let services = self.services.read().await;
        services.get(service_name)
            .filter(|service| service.health_status == HealthStatus::Healthy)
            .map(|service| service.info.clone())
    }
    
    /// Get all healthy endpoints for a service
    pub async fn get_service_endpoints(&self, service_name: &str) -> Vec<EndpointInfo> {
        let services = self.services.read().await;
        
        services.get(service_name)
            .filter(|service| service.health_status == HealthStatus::Healthy)
            .map(|service| service.info.endpoints.clone())
            .unwrap_or_default()
    }
    
    /// Subscribe to registry events
    pub fn subscribe(&self) -> broadcast::Receiver<RegistryEvent> {
        self.event_sender.subscribe()
    }
    
    /// Check service health and remove stale services
    async fn check_service_health(
        services: Arc<RwLock<HashMap<String, RegisteredService>>>,
        event_sender: broadcast::Sender<RegistryEvent>,
        service_ttl: Duration,
    ) {
        let mut services = services.write().await;
        let now = SystemTime::now();
        let mut stale_services = Vec::new();
        let mut unhealthy_services = Vec::new();
        
        for (name, service) in services.iter_mut() {
            if let Ok(elapsed) = now.duration_since(service.last_heartbeat) {
                if elapsed > service_ttl {
                    stale_services.push(name.clone());
                } else if elapsed > service_ttl / 2 && service.health_status == HealthStatus::Healthy {
                    service.health_status = HealthStatus::Unhealthy;
                    unhealthy_services.push(name.clone());
                }
            }
        }
        
        // Remove stale services
        for service_name in stale_services {
            services.remove(&service_name);
            let _ = event_sender.send(RegistryEvent::ServiceUnregistered(service_name));
        }
        
        // Mark unhealthy services
        for service_name in unhealthy_services {
            let _ = event_sender.send(RegistryEvent::ServiceHealthChanged(
                service_name,
                HealthStatus::Unhealthy,
            ));
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Load balancer for selecting service endpoints
pub struct LoadBalancer {
    strategy: LoadBalancingStrategy,
    round_robin_counter: std::sync::atomic::AtomicUsize,
}

impl LoadBalancer {
    /// Create a new load balancer with the specified strategy
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            strategy,
            round_robin_counter: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    
    /// Select an endpoint from the available endpoints
    pub fn select_endpoint(&self, endpoints: &[EndpointInfo]) -> Option<&EndpointInfo> {
        if endpoints.is_empty() {
            return None;
        }
        
        let healthy_endpoints: Vec<&EndpointInfo> = endpoints
            .iter()
            .filter(|endpoint| endpoint.health == HealthStatus::Healthy)
            .collect();
        
        if healthy_endpoints.is_empty() {
            return None;
        }
        
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let index = self.round_robin_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    % healthy_endpoints.len();
                Some(healthy_endpoints[index])
            }
            LoadBalancingStrategy::Random => {
                use rand::Rng;
                let index = rand::thread_rng().gen_range(0..healthy_endpoints.len());
                Some(healthy_endpoints[index])
            }
            LoadBalancingStrategy::WeightedRoundRobin => {
                // Calculate total weight
                let total_weight: u32 = healthy_endpoints.iter().map(|e| e.weight).sum();
                if total_weight == 0 {
                    return self.select_endpoint_round_robin(&healthy_endpoints);
                }
                
                // Generate random number and find corresponding endpoint
                use rand::Rng;
                let mut target = rand::thread_rng().gen_range(0..total_weight);
                
                for endpoint in healthy_endpoints {
                    if target < endpoint.weight {
                        return Some(endpoint);
                    }
                    target -= endpoint.weight;
                }
                
                // Fallback to first endpoint
                Some(healthy_endpoints[0])
            }
            LoadBalancingStrategy::LeastConnections => {
                // For now, just use round-robin as we don't track active connections
                // In a real implementation, you'd track active connections per endpoint
                self.select_endpoint_round_robin(&healthy_endpoints)
            }
            LoadBalancingStrategy::ConsistentHash => {
                // For consistent hashing, you'd typically hash the request key
                // For now, use round-robin as fallback
                self.select_endpoint_round_robin(&healthy_endpoints)
            }
        }
    }
    
    fn select_endpoint_round_robin(&self, endpoints: &[&EndpointInfo]) -> Option<&EndpointInfo> {
        let index = self.round_robin_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % endpoints.len();
        Some(endpoints[index])
    }
}

/// Service discovery client for finding and connecting to services
pub struct DiscoveryClient {
    registry: Arc<ServiceRegistry>,
    load_balancer: LoadBalancer,
}

impl DiscoveryClient {
    /// Create a new discovery client
    pub fn new(registry: Arc<ServiceRegistry>, strategy: LoadBalancingStrategy) -> Self {
        Self {
            registry,
            load_balancer: LoadBalancer::new(strategy),
        }
    }
    
    /// Find the best endpoint for a service
    pub async fn find_service_endpoint(&self, service_name: &str) -> Option<EndpointInfo> {
        let endpoints = self.registry.get_service_endpoints(service_name).await;
        self.load_balancer.select_endpoint(&endpoints).cloned()
    }
    
    /// Get all available services
    pub async fn list_services(&self) -> Vec<ServiceInfo> {
        self.registry.discover_services(None).await
    }
    
    /// Find services matching a pattern
    pub async fn find_services(&self, pattern: &str) -> Vec<ServiceInfo> {
        self.registry.discover_services(Some(pattern)).await
    }
}