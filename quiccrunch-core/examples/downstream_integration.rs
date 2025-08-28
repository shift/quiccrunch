// Copyright (c) 2025 QuicCrunch Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Complete example showing how downstream projects can integrate QuicCrunch
//! This demonstrates various integration patterns and best practices

use quiccrunch_core::{QuicClient, QuicServer, MessageHandler, run_server_with_handler, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

// Example: Custom message types for a downstream application
#[derive(Serialize, Deserialize, Debug)]
struct ApiRequest {
    method: String,
    path: String,
    data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ApiResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

// Example: Custom handler implementing business logic
#[derive(Clone)]
struct ApiHandler {
    name: String,
}

impl ApiHandler {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl MessageHandler for ApiHandler {
    type Response = Bytes;
    
    async fn handle(&self, message: Bytes) -> Bytes {
        // Parse incoming request
        match serde_json::from_slice::<ApiRequest>(&message) {
            Ok(request) => {
                println!("[{}] Handling {} {}", self.name, request.method, request.path);
                
                // Simulate API processing
                let response = ApiResponse {
                    status: 200,
                    headers: vec![
                        ("content-type".to_string(), "application/json".to_string()),
                        ("server".to_string(), format!("QuicCrunch-{}", self.name)),
                    ],
                    body: format!("{{\"message\": \"Processed {} {}\"}}", 
                                request.method, request.path).into_bytes(),
                };
                
                serde_json::to_vec(&response).unwrap_or_default().into()
            }
            Err(_) => {
                // Handle invalid JSON
                let error_response = ApiResponse {
                    status: 400,
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                    body: b"{\"error\": \"Invalid JSON request\"}".to_vec(),
                };
                
                serde_json::to_vec(&error_response).unwrap_or_default().into()
            }
        }
    }
}

// Example: Downstream application using QuicCrunch
struct MyApplication {
    client: QuicClient,
    server_addr: String,
}

impl MyApplication {
    fn new(server_addr: &str) -> Result<Self> {
        let client = QuicClient::new()?;
        Ok(Self {
            client,
            server_addr: server_addr.to_string(),
        })
    }
    
    async fn make_api_call(&self, method: &str, path: &str, data: &[u8]) -> Result<ApiResponse> {
        let request = ApiRequest {
            method: method.to_string(),
            path: path.to_string(),
            data: data.to_vec(),
        };
        
        let connection = self.client.connect(&self.server_addr).await?;
        let request_bytes = serde_json::to_vec(&request).unwrap();
        
        let response_bytes = timeout(
            Duration::from_secs(10),
            connection.request(request_bytes)
        ).await??;
        
        let response: ApiResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| format!("Failed to parse response: {}", e))?;
            
        Ok(response)
    }
    
    async fn upload_file(&self, filename: &str, content: &[u8]) -> Result<ApiResponse> {
        let path = format!("/files/{}", filename);
        self.make_api_call("PUT", &path, content).await
    }
    
    async fn download_file(&self, filename: &str) -> Result<Vec<u8>> {
        let path = format!("/files/{}", filename);
        let response = self.make_api_call("GET", &path, &[]).await?;
        Ok(response.body)
    }
    
    async fn list_files(&self) -> Result<Vec<String>> {
        let response = self.make_api_call("GET", "/files", &[]).await?;
        let file_list: serde_json::Value = serde_json::from_slice(&response.body)?;
        Ok(file_list["files"].as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect())
    }
}

// Example: Multiple concurrent connections
async fn stress_test_example(app: &MyApplication) -> Result<()> {
    println!("Running stress test with 100 concurrent requests...");
    
    let mut handles = Vec::new();
    for i in 0..100 {
        let app_clone = Arc::new(app);
        let handle = tokio::spawn(async move {
            let data = format!("test data {}", i);
            app_clone.make_api_call("POST", "/data", data.as_bytes()).await
        });
        handles.push(handle);
    }
    
    let mut success_count = 0;
    for handle in handles {
        match handle.await? {
            Ok(response) if response.status == 200 => success_count += 1,
            Ok(response) => println!("Unexpected status: {}", response.status),
            Err(e) => println!("Request failed: {}", e),
        }
    }
    
    println!("Stress test completed: {}/100 requests successful", success_count);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    
    match args.get(1).map(|s| s.as_str()) {
        Some("server") => {
            println!("Starting QuicCrunch API server...");
            let handler = ApiHandler::new("MyAPI");
            run_server_with_handler("127.0.0.1:50051", handler).await
        }
        
        Some("client") => {
            println!("Starting QuicCrunch API client...");
            
            // Give server time to start if running locally
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            let app = MyApplication::new("127.0.0.1:50051")?;
            
            // Example 1: Simple API call
            let response = app.make_api_call("GET", "/health", &[]).await?;
            println!("Health check: {} - {}", response.status, 
                    String::from_utf8_lossy(&response.body));
            
            // Example 2: File operations
            let upload_response = app.upload_file("test.txt", b"Hello, QuicCrunch!").await?;
            println!("File upload: {}", upload_response.status);
            
            let downloaded_content = app.download_file("test.txt").await?;
            println!("Downloaded: {}", String::from_utf8_lossy(&downloaded_content));
            
            // Example 3: List operations
            let files = app.list_files().await?;
            println!("Files: {:?}", files);
            
            // Example 4: Stress test
            stress_test_example(&app).await?;
            
            Ok(())
        }
        
        Some("both") => {
            // Run both server and client in the same process
            println!("Starting integrated server and client...");
            
            // Start server in background
            let server_handle = tokio::spawn(async {
                let handler = ApiHandler::new("Integrated");
                run_server_with_handler("127.0.0.1:50052", handler).await
            });
            
            // Give server time to start
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Run client
            let app = MyApplication::new("127.0.0.1:50052")?;
            let response = app.make_api_call("GET", "/integrated", &[]).await?;
            println!("Integrated test: {} - {}", response.status, 
                    String::from_utf8_lossy(&response.body));
            
            // Stop server
            server_handle.abort();
            Ok(())
        }
        
        _ => {
            println!("Usage:");
            println!("  cargo run --example downstream_integration server   # Start server");
            println!("  cargo run --example downstream_integration client   # Run client");
            println!("  cargo run --example downstream_integration both     # Run both");
            Ok(())
        }
    }
}

// Example of how to integrate with existing async frameworks
#[cfg(feature = "axum-integration")]
mod axum_integration {
    use super::*;
    use axum::{Router, response::Json, extract::State};
    
    // Example: Using QuicCrunch as backend for HTTP server
    struct AppState {
        quic_client: QuicClient,
        backend_addr: String,
    }
    
    async fn proxy_handler(
        State(state): State<AppState>,
        body: bytes::Bytes,
    ) -> Result<Json<serde_json::Value>, String> {
        let app = MyApplication::new(&state.backend_addr)
            .map_err(|e| format!("Failed to create app: {}", e))?;
            
        let response = app.make_api_call("POST", "/proxy", &body).await
            .map_err(|e| format!("QUIC request failed: {}", e))?;
            
        let json: serde_json::Value = serde_json::from_slice(&response.body)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;
            
        Ok(Json(json))
    }
    
    pub fn create_router(backend_addr: String) -> Router {
        let client = QuicClient::new().expect("Failed to create QUIC client");
        let state = AppState {
            quic_client: client,
            backend_addr,
        };
        
        Router::new()
            .route("/api/*path", axum::routing::post(proxy_handler))
            .with_state(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_downstream_integration() -> Result<()> {
        // Start test server
        let server = QuicServer::new()?;
        let mut listener = server.listen("127.0.0.1:0").await?;
        let server_addr = listener.local_addr()?;
        
        // Run handler in background
        let handler = ApiHandler::new("Test");
        tokio::spawn(async move {
            while let Ok(connection) = listener.accept().await {
                let handler = handler.clone();
                tokio::spawn(async move {
                    while let Ok(mut stream) = connection.accept_stream().await {
                        let handler = handler.clone();
                        tokio::spawn(async move {
                            while let Ok(message) = stream.receive_message().await {
                                let response = handler.handle(message).await;
                                let _ = stream.send_message(response).await;
                            }
                        });
                    }
                });
            }
        });
        
        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Test client
        let app = MyApplication::new(&server_addr)?;
        let response = app.make_api_call("GET", "/test", &[]).await?;
        
        assert_eq!(response.status, 200);
        assert!(String::from_utf8_lossy(&response.body).contains("Processed GET /test"));
        
        Ok(())
    }
}