//! RPC Service Reflection API
//!
//! Provides introspection capabilities for RPC services:
//! - Service listing
//! - Method introspection
//! - Parameter/return type introspection
//! - Documentation retrieval

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::rpc::error::{RpcResult, RpcError};
use crate::rpc::service::{ServiceDefinition, MethodDefinition, StreamingType, ServiceRegistry};

/// Documentation for a service or method
#[derive(Debug, Clone)]
pub struct Documentation {
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub examples: Vec<Example>,
}

/// Example for documentation
#[derive(Debug, Clone)]
pub struct Example {
    pub name: String,
    pub description: Option<String>,
    pub request: Option<String>,
    pub response: Option<String>,
}

/// Parameter information for method introspection
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub type_name: String,
    pub description: Option<String>,
    pub required: bool,
    pub default_value: Option<String>,
}

/// Method reflection information
#[derive(Debug, Clone)]
pub struct MethodReflection {
    pub id: u32,
    pub name: String,
    pub streaming_type: StreamingType,
    pub input_type: String,
    pub output_type: String,
    pub parameters: Vec<ParameterInfo>,
    pub documentation: Option<Documentation>,
}

/// Service reflection information
#[derive(Debug, Clone)]
pub struct ServiceReflection {
    pub id: u32,
    pub name: String,
    pub methods: Vec<MethodReflection>,
    pub documentation: Option<Documentation>,
}

/// Reflection API for RPC services
pub struct ReflectionApi {
    registry: Arc<ServiceRegistry>,
    documentation: RwLock<HashMap<String, Documentation>>,
    method_docs: RwLock<HashMap<(String, String), Documentation>>,
}

impl ReflectionApi {
    pub fn new(registry: Arc<ServiceRegistry>) -> Self {
        Self {
            registry,
            documentation: RwLock::new(HashMap::new()),
            method_docs: RwLock::new(HashMap::new()),
        }
    }

    /// List all registered services
    pub async fn list_services(&self) -> Vec<ServiceReflection> {
        let services = self.registry.list_services().await;
        let docs = self.documentation.read().await;

        services
            .into_iter()
            .map(|service| {
                let documentation = docs.get(&service.name).cloned();
                let methods = service
                    .methods
                    .values()
                    .map(|m| MethodReflection {
                        id: m.id,
                        name: m.name.clone(),
                        streaming_type: m.streaming_type,
                        input_type: "bytes".to_string(), // Default, would be from schema
                        output_type: "bytes".to_string(), // Default, would be from schema
                        parameters: Vec::new(), // Would be parsed from schema
                        documentation: None,
                    })
                    .collect();

                ServiceReflection {
                    id: service.id,
                    name: service.name.clone(),
                    methods,
                    documentation,
                }
            })
            .collect()
    }

    /// Get reflection information for a specific service
    pub async fn get_service_reflection(&self, service_name: &str) -> Option<ServiceReflection> {
        let services = self.registry.list_services().await;
        let docs = self.documentation.read().await;
        let method_docs = self.method_docs.read().await;

        services
            .into_iter()
            .find(|s| s.name == service_name)
            .map(|service| {
                let documentation = docs.get(&service.name).cloned();
                let methods = service
                    .methods
                    .values()
                    .map(|m| {
                        let method_doc = method_docs.get(&(service.name.clone(), m.name.clone())).cloned();
                        MethodReflection {
                            id: m.id,
                            name: m.name.clone(),
                            streaming_type: m.streaming_type,
                            input_type: "bytes".to_string(),
                            output_type: "bytes".to_string(),
                            parameters: Vec::new(),
                            documentation: method_doc,
                        }
                    })
                    .collect();

                ServiceReflection {
                    id: service.id,
                    name: service.name.clone(),
                    methods,
                    documentation,
                }
            })
    }

    /// Get reflection information for a specific method
    pub async fn get_method_reflection(
        &self,
        service_name: &str,
        method_name: &str,
    ) -> Option<MethodReflection> {
        let services = self.registry.list_services().await;
        let method_docs = self.method_docs.read().await;

        services
            .into_iter()
            .find(|s| s.name == service_name)
            .and_then(|service| {
                service
                    .methods
                    .values()
                    .find(|m| m.name == method_name)
                    .map(|m| {
                        let method_doc = method_docs.get(&(service_name.to_string(), method_name.to_string())).cloned();
                        MethodReflection {
                            id: m.id,
                            name: m.name.clone(),
                            streaming_type: m.streaming_type,
                            input_type: "bytes".to_string(),
                            output_type: "bytes".to_string(),
                            parameters: Vec::new(),
                            documentation: method_doc,
                        }
                    })
            })
    }

    /// Set documentation for a service
    pub async fn set_service_documentation(
        &self,
        service_name: &str,
        documentation: Documentation,
    ) {
        let mut docs = self.documentation.write().await;
        docs.insert(service_name.to_string(), documentation);
    }

    /// Set documentation for a method
    pub async fn set_method_documentation(
        &self,
        service_name: &str,
        method_name: &str,
        documentation: Documentation,
    ) {
        let mut method_docs = self.method_docs.write().await;
        method_docs.insert(
            (service_name.to_string(), method_name.to_string()),
            documentation,
        );
    }

    /// Get service schema (protobuf-like)
    pub async fn get_service_schema(&self, service_name: &str) -> Option<String> {
        let services = self.registry.list_services().await;

        services
            .into_iter()
            .find(|s| s.name == service_name)
            .map(|service| {
                // Generate a simple schema representation
                let mut schema = format!("service {} {{\n", service.name);
                for method in service.methods.values() {
                    let streaming = match method.streaming_type {
                        StreamingType::Unary => "",
                        StreamingType::ServerStreaming => "stream ",
                        StreamingType::ClientStreaming => "stream ",
                        StreamingType::Bidirectional => "stream ",
                    };
                    schema.push_str(&format!(
                        "  {} {}({} {}) returns ({} {});\n",
                        streaming,
                        method.name,
                        streaming,
                        "Request", // Would be from schema
                        streaming,
                        "Response", // Would be from schema
                    ));
                }
                schema.push('}');
                schema
            })
    }
}

impl Default for Documentation {
    fn default() -> Self {
        Self {
            short_description: None,
            long_description: None,
            examples: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::service::{ServiceDefinition, MethodDefinition};

    #[tokio::test]
    async fn test_reflection_api() {
        let registry = Arc::new(ServiceRegistry::new());
        let reflection = ReflectionApi::new(registry.clone());

        // Create a test service
        let mut service_def = ServiceDefinition::new(1, "TestService");
        let method = MethodDefinition {
            id: 1,
            name: "TestMethod".to_string(),
            streaming_type: StreamingType::Unary,
            request_schema: None,
            response_schema: None,
        };
        service_def.add_method(method);

        // Test listing services
        let services = reflection.list_services().await;
        assert_eq!(services.len(), 0); // No services registered yet
    }
}
