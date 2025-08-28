Feature: QUIC transport behavior remains consistent when swapping implementation

  Background:
    Given a running QUIC server

  Scenario: Basic roundtrip echo
    When a client connects and opens a bidirectional stream
    And the client sends the message "Hello, World!"
    Then the client receives the same message back

  Scenario: Multiple concurrent streams
    When a client opens 5 concurrent streams and sends unique messages
    Then each stream receives the same message back

  Scenario: Large message transfer
    When a client sends a 1048576-byte message on a stream
    Then the client receives the same message back within 10 seconds

  Scenario: Connection attempt to invalid address fails fast
    When a client attempts to connect to "127.0.0.1:1"
    Then the connection attempt fails
