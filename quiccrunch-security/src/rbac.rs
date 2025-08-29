//! Role-Based Access Control (RBAC) system

use crate::{SecurityError, SecurityResult, SecurityContext};
use crate::config::RbacConfig;
use casbin::{Enforcer, Model, MemoryAdapter, MgmtApi, RbacApi, CoreApi};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

/// RBAC role definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub inherits_from: Vec<String>, // Role inheritance
    pub created_at: DateTime<Utc>,
    pub active: bool,
}

/// RBAC permission definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    pub name: String,
    pub description: String,
    pub resource: String,
    pub action: String,
    pub conditions: Vec<String>, // Optional conditions
    pub created_at: DateTime<Utc>,
    pub active: bool,
}

/// RBAC policy definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub subject: String,    // User or role
    pub object: String,     // Resource
    pub action: String,     // Action (read, write, execute, etc.)
    pub effect: PolicyEffect, // Allow or deny
    pub conditions: Option<HashMap<String, String>>,
    pub created_at: DateTime<Utc>,
}

/// Policy effect
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PolicyEffect {
    Allow,
    Deny,
}

/// Access decision result
#[derive(Debug, Clone)]
pub struct AccessDecision {
    pub granted: bool,
    pub reason: String,
    pub policies_applied: Vec<String>,
    pub evaluation_time_ms: u64,
}

/// RBAC Manager
pub struct RbacManager {
    config: RbacConfig,
    enforcer: Arc<RwLock<Enforcer>>,
    roles: Arc<RwLock<HashMap<String, Role>>>,
    permissions: Arc<RwLock<HashMap<String, Permission>>>,
    role_cache: Arc<RwLock<HashMap<String, Vec<String>>>>, // User -> Roles cache
    permission_cache: Arc<RwLock<HashMap<String, Vec<String>>>>, // User -> Permissions cache
}

impl RbacManager {
    pub fn new(config: RbacConfig) -> SecurityResult<Self> {
        // Initialize Casbin model
        let model_text = r#"
        [request_definition]
        r = sub, obj, act

        [policy_definition]
        p = sub, obj, act

        [role_definition]
        g = _, _

        [policy_effect]
        e = some(where (p.eft == allow))

        [matchers]
        m = g(r.sub, p.sub) && r.obj == p.obj && r.act == p.act
        "#;

        let model = Model::from_str(model_text)
            .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;

        // Initialize adapter
        let adapter = MemoryAdapter::default();

        let enforcer = Enforcer::new(model, adapter)
            .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;

        let mut rbac = Self {
            config,
            enforcer: Arc::new(RwLock::new(enforcer)),
            roles: Arc::new(RwLock::new(HashMap::new())),
            permissions: Arc::new(RwLock::new(HashMap::new())),
            role_cache: Arc::new(RwLock::new(HashMap::new())),
            permission_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize default roles and permissions
        rbac.init_default_policies()?;

        Ok(rbac)
    }

    /// Initialize default roles and permissions
    fn init_default_policies(&mut self) -> SecurityResult<()> {
        // This would typically be done during the async initialization
        // For now, we'll defer this to when the manager is used
        Ok(())
    }

    /// Create a new role
    pub async fn create_role(
        &self,
        name: String,
        description: String,
        permissions: Vec<String>,
        inherits_from: Vec<String>,
    ) -> SecurityResult<Role> {
        let role = Role {
            name: name.clone(),
            description,
            permissions: permissions.clone(),
            inherits_from: inherits_from.clone(),
            created_at: Utc::now(),
            active: true,
        };

        // Store role
        {
            let mut roles = self.roles.write().await;
            roles.insert(name.clone(), role.clone());
        }

        // Add role policies to enforcer
        {
            let mut enforcer = self.enforcer.write().await;
            for permission in &permissions {
                // Parse permission into resource and action
                let parts: Vec<&str> = permission.split(':').collect();
                if parts.len() >= 2 {
                    let resource = parts[0];
                    let action = parts[1];
                    
                    enforcer.add_policy(vec![name.clone(), resource.to_string(), action.to_string()])
                        .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
                }
            }

            // Add role inheritance
            for parent_role in &inherits_from {
                enforcer.add_grouping_policy(vec![name.clone(), parent_role.clone()])
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
            }
        }

        // Clear cache
        self.clear_cache().await;

        Ok(role)
    }

    /// Create a new permission
    pub async fn create_permission(
        &self,
        name: String,
        description: String,
        resource: String,
        action: String,
        conditions: Vec<String>,
    ) -> SecurityResult<Permission> {
        let permission = Permission {
            name: name.clone(),
            description,
            resource,
            action,
            conditions,
            created_at: Utc::now(),
            active: true,
        };

        let mut permissions = self.permissions.write().await;
        permissions.insert(name, permission.clone());

        Ok(permission)
    }

    /// Assign role to user
    pub async fn assign_role_to_user(&self, user_id: &str, role_name: &str) -> SecurityResult<()> {
        // Check if role exists
        {
            let roles = self.roles.read().await;
            if !roles.contains_key(role_name) {
                return Err(SecurityError::ConfigurationError(
                    format!("Role {} does not exist", role_name)
                ));
            }
        }

        // Add user-role assignment to enforcer
        {
            let mut enforcer = self.enforcer.write().await;
            enforcer.add_grouping_policy(vec![user_id.to_string(), role_name.to_string()])
                .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
        }

        // Clear cache for user
        self.clear_user_cache(user_id).await;

        Ok(())
    }

    /// Remove role from user
    pub async fn remove_role_from_user(&self, user_id: &str, role_name: &str) -> SecurityResult<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer.remove_grouping_policy(vec![user_id.to_string(), role_name.to_string()])
            .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;

        // Clear cache for user
        drop(enforcer);
        self.clear_user_cache(user_id).await;

        Ok(())
    }

    /// Check if user has permission to perform action on resource
    pub async fn check_permission(
        &self,
        user_id: &str,
        resource: &str,
        action: &str,
    ) -> SecurityResult<AccessDecision> {
        let start_time = std::time::Instant::now();

        let enforcer = self.enforcer.read().await;
        let granted = enforcer.enforce((user_id, resource, action))
            .map_err(|e| SecurityError::AuthorizationDenied(e.to_string()))?;

        let evaluation_time = start_time.elapsed().as_millis() as u64;

        let decision = AccessDecision {
            granted,
            reason: if granted {
                "Access granted by RBAC policy".to_string()
            } else {
                "Access denied - insufficient permissions".to_string()
            },
            policies_applied: vec![], // TODO: Extract applied policies from Casbin
            evaluation_time_ms: evaluation_time,
        };

        Ok(decision)
    }

    /// Check permission with security context
    pub async fn check_permission_with_context(
        &self,
        context: &SecurityContext,
        resource: &str,
        action: &str,
    ) -> SecurityResult<AccessDecision> {
        if !context.authenticated {
            return Ok(AccessDecision {
                granted: false,
                reason: "User not authenticated".to_string(),
                policies_applied: vec![],
                evaluation_time_ms: 0,
            });
        }

        let user_id = context.user_id.as_ref()
            .ok_or_else(|| SecurityError::AuthorizationDenied("No user ID in context".to_string()))?;

        self.check_permission(user_id, resource, action).await
    }

    /// Get user roles
    pub async fn get_user_roles(&self, user_id: &str) -> SecurityResult<Vec<String>> {
        // Check cache first
        {
            let cache = self.role_cache.read().await;
            if let Some(roles) = cache.get(user_id) {
                return Ok(roles.clone());
            }
        }

        // Get roles from enforcer
        let enforcer = self.enforcer.read().await;
        let roles = enforcer.get_roles_for_user(user_id, None);

        // Cache the result
        {
            let mut cache = self.role_cache.write().await;
            cache.insert(user_id.to_string(), roles.clone());
        }

        Ok(roles)
    }

    /// Get user permissions (flattened from roles)
    pub async fn get_user_permissions(&self, user_id: &str) -> SecurityResult<Vec<String>> {
        // Check cache first
        {
            let cache = self.permission_cache.read().await;
            if let Some(permissions) = cache.get(user_id) {
                return Ok(permissions.clone());
            }
        }

        // Get roles first
        let roles = self.get_user_roles(user_id).await?;

        // Collect all permissions from roles
        let mut all_permissions = HashSet::new();
        
        {
            let role_definitions = self.roles.read().await;
            
            // Recursive function to collect permissions including inherited ones
            fn collect_permissions(
                role_name: &str,
                role_definitions: &HashMap<String, Role>,
                collected: &mut HashSet<String>,
                visited: &mut HashSet<String>,
            ) {
                if visited.contains(role_name) {
                    return; // Prevent infinite recursion
                }
                visited.insert(role_name.to_string());

                if let Some(role) = role_definitions.get(role_name) {
                    // Add direct permissions
                    for permission in &role.permissions {
                        collected.insert(permission.clone());
                    }

                    // Add inherited permissions
                    for parent_role in &role.inherits_from {
                        collect_permissions(parent_role, role_definitions, collected, visited);
                    }
                }
            }

            let mut visited = HashSet::new();
            for role_name in &roles {
                collect_permissions(role_name, &role_definitions, &mut all_permissions, &mut visited);
            }
        }

        let permissions: Vec<String> = all_permissions.into_iter().collect();

        // Cache the result
        {
            let mut cache = self.permission_cache.write().await;
            cache.insert(user_id.to_string(), permissions.clone());
        }

        Ok(permissions)
    }

    /// Get all roles
    pub async fn get_all_roles(&self) -> SecurityResult<Vec<Role>> {
        let roles = self.roles.read().await;
        Ok(roles.values().cloned().collect())
    }

    /// Get role by name
    pub async fn get_role(&self, role_name: &str) -> SecurityResult<Option<Role>> {
        let roles = self.roles.read().await;
        Ok(roles.get(role_name).cloned())
    }

    /// Update role permissions
    pub async fn update_role_permissions(
        &self,
        role_name: &str,
        permissions: Vec<String>,
    ) -> SecurityResult<()> {
        // Update role definition
        {
            let mut roles = self.roles.write().await;
            if let Some(role) = roles.get_mut(role_name) {
                role.permissions = permissions.clone();
            } else {
                return Err(SecurityError::ConfigurationError(
                    format!("Role {} not found", role_name)
                ));
            }
        }

        // Update enforcer policies
        {
            let mut enforcer = self.enforcer.write().await;
            
            // Remove existing policies for this role
            let existing_policies = enforcer.get_filtered_policy(0, vec![role_name.to_string()]);
            for policy in existing_policies {
                enforcer.remove_policy(policy)
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
            }

            // Add new policies
            for permission in &permissions {
                let parts: Vec<&str> = permission.split(':').collect();
                if parts.len() >= 2 {
                    let resource = parts[0];
                    let action = parts[1];
                    
                    enforcer.add_policy(vec![role_name.to_string(), resource.to_string(), action.to_string()])
                        .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
                }
            }
        }

        // Clear cache
        self.clear_cache().await;

        Ok(())
    }

    /// Delete role
    pub async fn delete_role(&self, role_name: &str) -> SecurityResult<()> {
        // Remove from role definitions
        {
            let mut roles = self.roles.write().await;
            roles.remove(role_name);
        }

        // Remove all policies and groupings for this role
        {
            let mut enforcer = self.enforcer.write().await;
            
            // Remove role policies
            let policies = enforcer.get_filtered_policy(0, vec![role_name.to_string()]);
            for policy in policies {
                enforcer.remove_policy(policy)
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
            }

            // Remove role groupings (both as subject and object)
            let groupings = enforcer.get_filtered_grouping_policy(0, vec![role_name.to_string()]);
            for grouping in groupings {
                enforcer.remove_grouping_policy(grouping)
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
            }

            let parent_groupings = enforcer.get_filtered_grouping_policy(1, vec![role_name.to_string()]);
            for grouping in parent_groupings {
                enforcer.remove_grouping_policy(grouping)
                    .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
            }
        }

        // Clear cache
        self.clear_cache().await;

        Ok(())
    }

    /// Check if user has any of the required roles
    pub async fn user_has_any_role(&self, user_id: &str, required_roles: &[String]) -> SecurityResult<bool> {
        let user_roles = self.get_user_roles(user_id).await?;
        Ok(required_roles.iter().any(|role| user_roles.contains(role)))
    }

    /// Check if user has all required roles
    pub async fn user_has_all_roles(&self, user_id: &str, required_roles: &[String]) -> SecurityResult<bool> {
        let user_roles = self.get_user_roles(user_id).await?;
        Ok(required_roles.iter().all(|role| user_roles.contains(role)))
    }

    /// Clear all caches
    async fn clear_cache(&self) {
        let mut role_cache = self.role_cache.write().await;
        role_cache.clear();
        
        let mut permission_cache = self.permission_cache.write().await;
        permission_cache.clear();
    }

    /// Clear cache for specific user
    async fn clear_user_cache(&self, user_id: &str) {
        let mut role_cache = self.role_cache.write().await;
        role_cache.remove(user_id);
        
        let mut permission_cache = self.permission_cache.write().await;
        permission_cache.remove(user_id);
    }

    /// Save policies to persistent storage
    pub async fn save_policies(&self) -> SecurityResult<()> {
        let enforcer = self.enforcer.read().await;
        enforcer.save_policy()
            .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
        Ok(())
    }

    /// Load policies from persistent storage
    pub async fn load_policies(&self) -> SecurityResult<()> {
        let mut enforcer = self.enforcer.write().await;
        enforcer.load_policy()
            .map_err(|e| SecurityError::ConfigurationError(e.to_string()))?;
        
        // Clear cache after loading new policies
        drop(enforcer);
        self.clear_cache().await;
        
        Ok(())
    }
}

/// RBAC middleware for RPC methods
pub struct RbacMiddleware {
    rbac_manager: Arc<RbacManager>,
}

impl RbacMiddleware {
    pub fn new(rbac_manager: Arc<RbacManager>) -> Self {
        Self { rbac_manager }
    }

    /// Check authorization for RPC method
    pub async fn authorize_rpc_method(
        &self,
        context: &SecurityContext,
        method_name: &str,
        required_permissions: &[String],
    ) -> SecurityResult<AccessDecision> {
        if !context.authenticated {
            return Ok(AccessDecision {
                granted: false,
                reason: "Authentication required".to_string(),
                policies_applied: vec![],
                evaluation_time_ms: 0,
            });
        }

        let user_id = context.user_id.as_ref()
            .ok_or_else(|| SecurityError::AuthorizationDenied("No user ID in context".to_string()))?;

        // Check if user has any of the required permissions
        for permission in required_permissions {
            let parts: Vec<&str> = permission.split(':').collect();
            if parts.len() >= 2 {
                let resource = parts[0];
                let action = parts[1];
                
                let decision = self.rbac_manager.check_permission(user_id, resource, action).await?;
                if decision.granted {
                    return Ok(decision);
                }
            }
        }

        // If no permissions matched, check method-specific permission
        let method_resource = format!("rpc_method:{}", method_name);
        self.rbac_manager.check_permission(user_id, &method_resource, "execute").await
    }
}