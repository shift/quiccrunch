use quiccrunch_macros::rpc_service;

// Simple test trait to test the macro expansion
#[allow(dead_code)]
#[rpc_service]
trait TestService {
    async fn test_method(&self, value: i32) -> Result<i32, String>;
}

#[test]
fn test_macro_expansion() {
    // The macro should generate TestServiceHandler and TestServiceClient
    // For now, just test that they can be constructed
    let _handler = TestServiceHandler::new(TestServiceImpl);
    let _client = TestServiceClient::new();

    // Basic smoke test - these should compile without errors
    let _ = (_handler, _client);
}

// Simple implementation for testing
#[derive(Clone)]
struct TestServiceImpl;

impl TestService for TestServiceImpl {
    async fn test_method(&self, value: i32) -> Result<i32, String> {
        Ok(value * 2)
    }
}
