//! Example RPC Service using QuicCrunch
//! 
//! Demonstrates a complete RPC service with user management functionality.

use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use quiccrunch_core::{
    rpc::{RpcService, RpcServer, RpcClient, RpcContext, RpcResult, RpcError},
    QuicTransport, TlsConfig, Connection,
};

/// User data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub created_at: u64,
}

/// User creation request
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
}

/// User management service implementation
pub struct UserService {
    users: Arc<RwLock<HashMap<u64, User>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl UserService {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }
    
    async fn get_user_impl(&self, user_id: u64) -> RpcResult<Option<User>> {
        let users = self.users.read().await;
        Ok(users.get(&user_id).cloned())
    }
    
    async fn create_user_impl(&self, request: CreateUserRequest) -> RpcResult<User> {
        let user_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let user = User {
            id: user_id,
            name: request.name,
            email: request.email,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        let mut users = self.users.write().await;
        users.insert(user_id, user.clone());
        
        Ok(user)
    }
    
    async fn list_users_impl(&self) -> RpcResult<Vec<User>> {
        let users = self.users.read().await;
        Ok(users.values().cloned().collect())
    }
    
    async fn delete_user_impl(&self, user_id: u64) -> RpcResult<bool> {
        let mut users = self.users.write().await;
        Ok(users.remove(&user_id).is_some())
    }
}

#[async_trait]
impl RpcService for UserService {
    fn name(&self) -> &str {
        "UserService"
    }
    
    async fn call_method(&self, context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        match context.method.as_str() {
            "getUser" => {
                let request: serde_json::Value = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let user_id = request["user_id"].as_u64()
                    .ok_or_else(|| RpcError::InvalidParameters("Missing user_id".to_string()))?;
                
                let user = self.get_user_impl(user_id).await?;
                let response = serde_json::to_vec(&user)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            "createUser" => {
                let request: CreateUserRequest = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let user = self.create_user_impl(request).await?;
                let response = serde_json::to_vec(&user)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            "listUsers" => {
                let users = self.list_users_impl().await?;
                let response = serde_json::to_vec(&users)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            "deleteUser" => {
                let request: serde_json::Value = serde_json::from_slice(&params)
                    .map_err(|e| RpcError::InvalidParameters(e.to_string()))?;
                
                let user_id = request["user_id"].as_u64()
                    .ok_or_else(|| RpcError::InvalidParameters("Missing user_id".to_string()))?;
                
                let deleted = self.delete_user_impl(user_id).await?;
                let response = serde_json::to_vec(&deleted)
                    .map_err(|e| RpcError::SerializationError(e.to_string()))?;
                
                Ok(Bytes::from(response))
            }
            _ => Err(RpcError::MethodNotFound(context.method.clone())),
        }
    }
    
    fn methods(&self) -> Vec<String> {
        vec![
            "getUser".to_string(),
            "createUser".to_string(),
            "listUsers".to_string(),
            "deleteUser".to_string(),
        ]
    }
}

/// Example echo service for testing
pub struct EchoService;

#[async_trait]
impl RpcService for EchoService {
    fn name(&self) -> &str {
        "EchoService"
    }
    
    async fn call_method(&self, _context: &RpcContext, params: Bytes) -> RpcResult<Bytes> {
        // Simply echo back the parameters
        Ok(params)
    }
    
    fn methods(&self) -> Vec<String> {
        vec!["echo".to_string()]
    }
}

/// High-level RPC client for UserService
pub struct UserServiceClient {
    client: RpcClient,
}

impl UserServiceClient {
    pub fn new(connection: Connection) -> Self {
        Self {
            client: RpcClient::new(connection),
        }
    }
    
    pub async fn get_user(&self, user_id: u64) -> RpcResult<Option<User>> {
        let params = serde_json::json!({ "user_id": user_id });
        let params_bytes = Bytes::from(params.to_string());
        
        let result = self.client.call("UserService", "getUser", params_bytes).await?;
        let user: Option<User> = serde_json::from_slice(&result)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(user)
    }
    
    pub async fn create_user(&self, name: String, email: String) -> RpcResult<User> {
        let request = CreateUserRequest { name, email };
        let params_bytes = Bytes::from(serde_json::to_string(&request).unwrap());
        
        let result = self.client.call("UserService", "createUser", params_bytes).await?;
        let user: User = serde_json::from_slice(&result)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(user)
    }
    
    pub async fn list_users(&self) -> RpcResult<Vec<User>> {
        let params_bytes = Bytes::from("{}");
        
        let result = self.client.call("UserService", "listUsers", params_bytes).await?;
        let users: Vec<User> = serde_json::from_slice(&result)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(users)
    }
    
    pub async fn delete_user(&self, user_id: u64) -> RpcResult<bool> {
        let params = serde_json::json!({ "user_id": user_id });
        let params_bytes = Bytes::from(params.to_string());
        
        let result = self.client.call("UserService", "deleteUser", params_bytes).await?;
        let deleted: bool = serde_json::from_slice(&result)
            .map_err(|e| RpcError::SerializationError(e.to_string()))?;
        
        Ok(deleted)
    }
}

/// Start an RPC server with example services
pub async fn start_example_server(bind_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting QuicCrunch RPC server on {}", bind_addr);
    
    // Create TLS configuration
    let tls_config = TlsConfig::default();
    
    // Create transport and start listening
    let transport = QuicTransport::new(tls_config)?;
    let mut incoming = transport.listen(bind_addr).await?;
    
    // Create RPC server and register services
    let rpc_server = RpcServer::new();
    rpc_server.register_service(Arc::new(UserService::new())).await;
    rpc_server.register_service(Arc::new(EchoService)).await;
    
    println!("✅ Registered services: UserService, EchoService");
    println!("📝 Available methods:");
    println!("   UserService: getUser, createUser, listUsers, deleteUser");
    println!("   EchoService: echo");
    
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

/// Example client usage
pub async fn example_client_usage(server_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔗 Connecting to RPC server at {}", server_addr);
    
    // Connect to server
    let tls_config = TlsConfig::default();
    let transport = QuicTransport::new(tls_config)?;
    let connection = transport.connect(server_addr).await?;
    
    // Create client
    let client = UserServiceClient::new(connection);
    
    println!("📞 Testing RPC calls...");
    
    // Test user creation
    println!("Creating user...");
    let user = client.create_user("Alice".to_string(), "alice@example.com".to_string()).await?;
    println!("✅ Created user: {:?}", user);
    
    // Test user retrieval
    println!("Getting user...");
    let retrieved_user = client.get_user(user.id).await?;
    println!("✅ Retrieved user: {:?}", retrieved_user);
    
    // Test listing users
    println!("Listing all users...");
    let users = client.list_users().await?;
    println!("✅ All users: {:?}", users);
    
    // Test user deletion
    println!("Deleting user...");
    let deleted = client.delete_user(user.id).await?;
    println!("✅ User deleted: {}", deleted);
    
    println!("🎉 All RPC tests completed successfully!");
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    
    #[tokio::test]
    async fn test_user_service() {
        let service = UserService::new();
        
        // Test user creation
        let request = CreateUserRequest {
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
        };
        
        let user = service.create_user_impl(request).await.unwrap();
        assert_eq!(user.name, "Test User");
        assert_eq!(user.email, "test@example.com");
        
        // Test user retrieval
        let retrieved_user = service.get_user_impl(user.id).await.unwrap();
        assert!(retrieved_user.is_some());
        assert_eq!(retrieved_user.unwrap().id, user.id);
        
        // Test user deletion
        let deleted = service.delete_user_impl(user.id).await.unwrap();
        assert!(deleted);
        
        // Verify user is deleted
        let not_found = service.get_user_impl(user.id).await.unwrap();
        assert!(not_found.is_none());
    }
}