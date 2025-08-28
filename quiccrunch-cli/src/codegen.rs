/// Code generation utilities for the CLI
use anyhow::Result;
use std::collections::HashMap;

/// Template engine for generating code from schemas
pub struct CodeGenerator {
    templates: HashMap<String, String>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }
    
    /// Add a template for a specific language/type
    pub fn add_template(&mut self, name: String, template: String) {
        self.templates.insert(name, template);
    }
    
    /// Generate code using a template
    pub fn generate(&self, template_name: &str, context: &serde_json::Value) -> Result<String> {
        let template = self.templates.get(template_name)
            .ok_or_else(|| anyhow::anyhow!("Template '{}' not found", template_name))?;
        
        // Simple template substitution - in a full implementation,
        // this would use a proper template engine like Handlebars
        let mut result = template.clone();
        
        if let serde_json::Value::Object(map) = context {
            for (key, value) in map {
                let placeholder = format!("{{{{{}}}}}", key);
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                result = result.replace(&placeholder, &replacement);
            }
        }
        
        Ok(result)
    }
}

/// Schema parser for Cap'n Proto files
pub struct SchemaParser;

impl SchemaParser {
    /// Parse a Cap'n Proto schema file
    pub fn parse_schema(schema_path: &str) -> Result<SchemaInfo> {
        // This is a simplified parser - in a full implementation,
        // this would use the Cap'n Proto compiler or parser
        
        let content = std::fs::read_to_string(schema_path)?;
        
        // Extract basic information from the schema
        let service_name = extract_service_name(&content)?;
        let methods = extract_methods(&content)?;
        
        Ok(SchemaInfo {
            service_name,
            methods,
            namespace: None,
        })
    }
}

/// Information extracted from a schema
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub service_name: String,
    pub methods: Vec<MethodInfo>,
    pub namespace: Option<String>,
}

/// Information about an RPC method
#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub request_type: String,
    pub response_type: String,
    pub streaming: StreamingType,
}

#[derive(Debug, Clone)]
pub enum StreamingType {
    Unary,
    ServerStreaming,
    ClientStreaming,
    Bidirectional,
}

fn extract_service_name(content: &str) -> Result<String> {
    // Simple regex-based extraction - in a real implementation,
    // this would use proper Cap'n Proto parsing
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("interface ") && line.contains(" {") {
            let name = line
                .replace("interface ", "")
                .replace(" {", "")
                .trim()
                .to_string();
            return Ok(name);
        }
    }
    
    Ok("DefaultService".to_string())
}

fn extract_methods(content: &str) -> Result<Vec<MethodInfo>> {
    let mut methods = Vec::new();
    
    // Simple extraction - in a real implementation,
    // this would properly parse the Cap'n Proto syntax
    for line in content.lines() {
        let line = line.trim();
        if line.contains("(") && line.contains(")") && line.contains(":") {
            // This looks like a method definition
            let method_name = line.split('(').next().unwrap_or("method").trim();
            
            methods.push(MethodInfo {
                name: method_name.to_string(),
                request_type: "Request".to_string(),
                response_type: "Response".to_string(),
                streaming: StreamingType::Unary,
            });
        }
    }
    
    Ok(methods)
}