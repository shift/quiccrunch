//! BDD tests for RPC Code Generation Macros
//!
//! Testing macro expansion and generated code functionality

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use quiccrunch_core::{
    rpc::{RpcContext, RpcError, RpcResult, RpcService},
    rpc_params, rpc_result, rpc_service,
};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CalculatorRequest {
    a: i32,
    b: i32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CalculatorResponse {
    result: i32,
}

/// BDD Scenario: Generate working service from macro
#[tokio::test]
async fn test_bdd_scenario_generate_working_service_from_macro() {
    // Given: a service definition using rpc_service! macro
    // The macro should expand to a working service implementation

    // When: the macro expands the service code
    rpc_service! {
        CalculatorService {
            async fn add(a: i32, b: i32) -> i32;
            async fn multiply(a: i32, b: i32) -> i32;
        }
    }

    // Create a test implementation
    struct TestCalculatorService;

    #[async_trait]
    impl CalculatorService for TestCalculatorService {
        async fn add(&self, a: i32, b: i32) -> RpcResult<i32> {
            Ok(a + b)
        }

        async fn multiply(&self, a: i32, b: i32) -> RpcResult<i32> {
            Ok(a * b)
        }
    }

    // Then: the generated service should compile without errors
    let service = TestCalculatorService;

    // And: parameter serialization should work correctly
    let wrapper = CalculatorServiceWrapper::new(service);
    let context = RpcContext::new("CalculatorService".to_string(), "add".to_string());

    // Test that the wrapper implements RpcService
    let params = rpc_params!(a: 5, b: 3);
    let result = wrapper.call(&context, "add", params).await;

    // And: method dispatch should route to correct functions
    assert!(result.is_ok(), "Service call should succeed");
    let response_bytes = result.unwrap();
    let response: i32 = rpc_result!(response_bytes, i32).unwrap();
    assert_eq!(response, 8);
}

/// BDD Scenario: Generated client calls work end-to-end
#[tokio::test]
async fn test_bdd_scenario_generated_client_calls_work_end_to_end() {
    // Given: a service generated with rpc_service! macro
    rpc_service! {
        TestService {
            async fn echo(message: String) -> String;
        }
    }

    struct EchoService;

    #[async_trait]
    impl TestService for EchoService {
        async fn echo(&self, message: String) -> RpcResult<String> {
            Ok(format!("Echo: {}", message))
        }
    }

    // When: a client generated with rpc_client! macro makes a call
    // (This part will test the client generation once we implement it)

    // For now, test the service wrapper functionality
    let service = EchoService;
    let wrapper = TestServiceWrapper::new(service);
    let context = RpcContext::new("TestService".to_string(), "echo".to_string());

    // Then: the request should serialize correctly
    let params = rpc_params!(message: "Hello World".to_string());
    let result = wrapper.call(&context, "echo", params).await;

    // And: the response should deserialize correctly
    assert!(result.is_ok(), "Service call should succeed");
    let response_bytes = result.unwrap();
    let response: String = rpc_result!(response_bytes, String).unwrap();

    // And: the correct service method should be invoked
    assert_eq!(response, "Echo: Hello World");
}

/// BDD Scenario: Macro handles various parameter types
#[tokio::test]
async fn test_bdd_scenario_macro_handles_various_parameter_types() {
    // Given: a service with different parameter types
    rpc_service! {
        ComplexService {
            async fn process_data(
                id: u64,
                name: String,
                values: Vec<i32>,
                optional: Option<bool>
            ) -> bool;
        }
    }

    struct TestComplexService;

    #[async_trait]
    impl ComplexService for TestComplexService {
        async fn process_data(
            &self,
            id: u64,
            name: String,
            values: Vec<i32>,
            optional: Option<bool>,
        ) -> RpcResult<bool> {
            // Simple processing logic for testing
            Ok(id > 0 && !name.is_empty() && !values.is_empty() && optional.unwrap_or(true))
        }
    }

    // When: the macro processes complex parameter types
    let service = TestComplexService;
    let wrapper = ComplexServiceWrapper::new(service);
    let context = RpcContext::new("ComplexService".to_string(), "process_data".to_string());

    // Then: all parameter types should be handled correctly
    let params = rpc_params!(
        id: 123u64,
        name: "test".to_string(),
        values: vec![1, 2, 3],
        optional: Some(true)
    );

    let result = wrapper.call(&context, "process_data", params).await;
    assert!(result.is_ok(), "Complex service call should succeed");

    let response_bytes = result.unwrap();
    let response: bool = rpc_result!(response_bytes, bool).unwrap();
    assert!(response);
}

/// BDD Scenario: Macro generates correct method listing
#[tokio::test]
async fn test_bdd_scenario_macro_generates_correct_method_listing() {
    // Given: a service with multiple methods
    rpc_service! {
        MultiMethodService {
            async fn method_a() -> String;
            async fn method_b(x: i32) -> i32;
            async fn method_c(a: String, b: bool) -> bool;
        }
    }

    struct TestMultiMethodService;

    #[async_trait]
    impl MultiMethodService for TestMultiMethodService {
        async fn method_a(&self) -> RpcResult<String> {
            Ok("method_a".to_string())
        }

        async fn method_b(&self, x: i32) -> RpcResult<i32> {
            Ok(x * 2)
        }

        async fn method_c(&self, a: String, b: bool) -> RpcResult<bool> {
            Ok(!a.is_empty() && b)
        }
    }

    // When: we query the generated wrapper for available methods
    let service = TestMultiMethodService;
    let wrapper = MultiMethodServiceWrapper::new(service);
    let methods = wrapper.methods();

    // Then: all methods should be listed correctly
    assert_eq!(methods.len(), 3);
    assert!(methods.contains(&"method_a".to_string()));
    assert!(methods.contains(&"method_b".to_string()));
    assert!(methods.contains(&"method_c".to_string()));
}

/// BDD Scenario: Macro handles errors gracefully
#[tokio::test]
async fn test_bdd_scenario_macro_handles_errors_gracefully() {
    // Given: a service that can return errors
    rpc_service! {
        ErrorService {
            async fn divide(a: i32, b: i32) -> i32;
        }
    }

    struct TestErrorService;

    #[async_trait]
    impl ErrorService for TestErrorService {
        async fn divide(&self, a: i32, b: i32) -> RpcResult<i32> {
            if b == 0 {
                Err(RpcError::InvalidParameters("Division by zero".to_string()))
            } else {
                Ok(a / b)
            }
        }
    }

    // When: the service method returns an error
    let service = TestErrorService;
    let wrapper = ErrorServiceWrapper::new(service);
    let context = RpcContext::new("ErrorService".to_string(), "divide".to_string());

    // Then: the error should be properly propagated
    let params = rpc_params!(a: 10, b: 0);
    let result = wrapper.call(&context, "divide", params).await;

    assert!(result.is_err(), "Division by zero should return an error");
    match result.unwrap_err() {
        RpcError::InvalidParameters(msg) => {
            assert_eq!(msg, "Division by zero");
        }
        other => panic!("Expected InvalidParameters error, got: {:?}", other),
    }
}
