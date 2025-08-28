Feature: QuicCrunch API Documentation
  As a developer using QuicCrunch
  I want comprehensive API documentation with examples
  So that I can easily implement RPC services and streaming patterns

  Background:
    Given QuicCrunch has a multi-crate workspace structure
    And the core crate provides RPC and streaming functionality
    And the macros crate provides code generation
    And the CLI crate provides development tools

  Scenario: Generate comprehensive rustdoc documentation
    Given the core library has public APIs for RPC and streaming
    When I generate rustdoc documentation
    Then all public types should have detailed documentation
    And all public methods should have examples
    And the documentation should include streaming patterns
    And cross-references between related APIs should be present

  Scenario: Document RPC service creation with macros
    Given the #[rpc_service] macro generates RPC handlers and clients
    When I document the macro usage patterns
    Then developers should understand how to define service traits
    And they should see examples of generated code
    And they should understand configuration options
    And migration from legacy patterns should be documented

  Scenario: Document streaming RPC patterns
    Given QuicCrunch supports server, client, and bidirectional streaming
    When I document streaming APIs
    Then developers should see examples of each streaming pattern
    And stream ownership and lifecycle should be explained
    And error handling patterns should be documented
    And performance considerations should be covered

  Scenario: Provide migration guides for existing RPC users
    Given developers may be migrating from gRPC, tonic, or tarpc
    When I create migration guides
    Then feature comparisons should be clear
    And code migration examples should be provided
    And performance benefits should be highlighted
    And integration patterns should be documented

  Scenario: Document CLI tools for development workflow
    Given the CLI provides generate, test, serve, and benchmark commands
    When I document the CLI tools
    Then each command should have usage examples
    And development workflows should be documented
    And integration with build systems should be explained
    And troubleshooting guides should be provided

  Scenario: Create comprehensive tutorial documentation
    Given QuicCrunch has multiple components working together
    When I create tutorial documentation
    Then new users should have a clear learning path
    And examples should progress from simple to complex
    And best practices should be highlighted throughout
    And common pitfalls should be documented