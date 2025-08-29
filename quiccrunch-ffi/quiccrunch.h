/* QuicCrunch FFI C Headers */
/* Generated for QuicCrunch v0.1.0 */

#ifndef QUICCRUNCH_H
#define QUICCRUNCH_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Error codes for QuicCrunch FFI operations
 */
typedef enum {
  /**
   * Operation completed successfully
   */
  QUIC_ERROR_SUCCESS = 0,
  /**
   * Generic error occurred
   */
  QUIC_ERROR_ERROR = 1,
  /**
   * Invalid argument passed to function
   */
  QUIC_ERROR_INVALID_ARGUMENT = 2,
  /**
   * Connection failed
   */
  QUIC_ERROR_CONNECTION_FAILED = 3,
  /**
   * Operation timed out
   */
  QUIC_ERROR_TIMEOUT_ERROR = 4,
  /**
   * Serialization/deserialization error
   */
  QUIC_ERROR_SERIALIZATION_ERROR = 5,
  /**
   * TLS/certificate error
   */
  QUIC_ERROR_TLS_ERROR = 6,
  /**
   * Address parsing error
   */
  QUIC_ERROR_ADDRESS_ERROR = 7,
  /**
   * Out of memory
   */
  QUIC_ERROR_OUT_OF_MEMORY = 8,
  /**
   * Invalid handle/pointer
   */
  QUIC_ERROR_INVALID_HANDLE = 9,
  /**
   * Operation would block (for non-blocking operations)
   */
  QUIC_ERROR_WOULD_BLOCK = 10,
  /**
   * Stream was closed
   */
  QUIC_ERROR_STREAM_CLOSED = 11,
  /**
   * Not implemented
   */
  QUIC_ERROR_NOT_IMPLEMENTED = 12,
} QuicError;

/**
 * Opaque handle to a QuicTransport
 */
typedef struct QuicTransportHandle QuicTransportHandle;

/**
 * Opaque handle to a QuicConnection
 */
typedef struct QuicConnectionHandle QuicConnectionHandle;

/**
 * Opaque handle to a QuicStream
 */
typedef struct QuicStreamHandle QuicStreamHandle;

/**
 * Configuration for TLS
 */
typedef struct {
  /**
   * Path to certificate file (PEM format)
   */
  const char *cert_path;
  /**
   * Path to private key file (PEM format)
   */
  const char *key_path;
  /**
   * Path to CA certificate file (PEM format) - optional, can be null
   */
  const char *ca_path;
} QuicTlsConfig;

/**
 * Connection configuration
 */
typedef struct {
  /**
   * Maximum number of concurrent streams
   */
  uint32_t max_concurrent_streams;
  /**
   * Connection timeout in milliseconds
   */
  uint32_t connection_timeout_ms;
  /**
   * Keep alive interval in milliseconds (0 to disable)
   */
  uint32_t keep_alive_ms;
} QuicConnectionConfig;

/**
 * Message data structure for sending/receiving
 */
typedef struct {
  /**
   * Pointer to message data
   */
  const uint8_t *data;
  /**
   * Length of message data in bytes
   */
  size_t len;
} QuicMessage;

/**
 * Callback function type for async operations
 * 
 * Parameters:
 * - user_data: User-provided data pointer
 * - error: Error code (QuicError::Success if no error)
 * - result_data: Operation-specific result data (can be null)
 */
typedef void (*QuicCallback)(void *user_data, QuicError error, void *result_data);

/**
 * Message handler callback type
 * 
 * Parameters:
 * - user_data: User-provided data pointer
 * - connection_id: ID of the connection that sent the message
 * - message: The received message
 * - response: Output parameter - set this to the response message
 * 
 * Returns QuicError::Success to send the response, or an error code to close the connection
 */
typedef QuicError (*QuicMessageHandler)(void *user_data,
                                        uint32_t connection_id,
                                        const QuicMessage *message,
                                        QuicMessage *response);

/**
 * Initialize the QuicCrunch FFI library.
 * This must be called before using any other FFI functions.
 * 
 * Returns 0 on success, non-zero on error.
 */
QuicError quiccrunch_init(void);

/**
 * Clean up and shutdown the QuicCrunch FFI library.
 * This should be called when done using the library.
 */
QuicError quiccrunch_shutdown(void);

/**
 * Get the version of the QuicCrunch library.
 * Returns a null-terminated string that must be freed with quiccrunch_string_free().
 */
char *quiccrunch_version(void);

/**
 * Free a string that was allocated by QuicCrunch FFI functions.
 * 
 * # Safety
 * The string pointer must have been returned by a QuicCrunch FFI function
 * and must not be used after calling this function.
 */
void quiccrunch_string_free(char *s);

/**
 * Create a new QUIC client
 * 
 * Returns a handle to the client, or null on error.
 * The handle must be freed with quiccrunch_client_free().
 */
QuicTransportHandle *quiccrunch_client_new(void);

/**
 * Create a new QUIC client with TLS configuration
 * 
 * # Arguments
 * * `tls_config` - TLS configuration for the client
 * 
 * Returns a handle to the client, or null on error.
 */
QuicTransportHandle *quiccrunch_client_new_with_tls(const QuicTlsConfig *tls_config);

/**
 * Free a QUIC client handle
 * 
 * # Safety
 * The handle must have been created by quiccrunch_client_new() and
 * must not be used after calling this function.
 */
void quiccrunch_client_free(QuicTransportHandle *handle);

/**
 * Initialize the client (creates the underlying transport)
 * 
 * This must be called before connecting to servers.
 * 
 * # Arguments
 * * `handle` - Client handle
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_client_init(QuicTransportHandle *handle);

/**
 * Connect to a server synchronously (blocks until connected)
 * 
 * # Arguments
 * * `handle` - Client handle
 * * `address` - Server address (e.g., "127.0.0.1:50051")
 * * `connection_id` - Output parameter for connection ID
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_client_connect(QuicTransportHandle *handle,
                                    const char *address,
                                    uint32_t *connection_id);

/**
 * Connect to a server asynchronously
 * 
 * # Arguments
 * * `handle` - Client handle
 * * `address` - Server address (e.g., "127.0.0.1:50051")
 * * `callback` - Callback function to call when connection completes
 * * `user_data` - User data to pass to callback
 * 
 * The callback will be called with:
 * - user_data: The user_data parameter
 * - error: QuicError::Success on success, error code on failure  
 * - result_data: Connection ID (u32*) on success, null on error
 */
QuicError quiccrunch_client_connect_async(QuicTransportHandle *handle,
                                          const char *address,
                                          QuicCallback callback,
                                          void *user_data);

/**
 * Disconnect from a server
 * 
 * # Arguments
 * * `handle` - Client handle
 * * `connection_id` - Connection ID to disconnect
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_client_disconnect(QuicTransportHandle *handle, uint32_t connection_id);

/**
 * Send a message on a connection
 * 
 * # Arguments
 * * `handle` - Client handle
 * * `connection_id` - Connection ID
 * * `message` - Message to send
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_client_send_message(QuicTransportHandle *handle,
                                         uint32_t connection_id,
                                         const QuicMessage *message);

/**
 * Get the number of active connections
 * 
 * # Arguments
 * * `handle` - Client handle
 * 
 * Returns the number of active connections, or 0 if handle is invalid.
 */
uint32_t quiccrunch_client_connection_count(QuicTransportHandle *handle);

/**
 * Create a new QUIC server
 * 
 * Returns a handle to the server, or null on error.
 * The handle must be freed with quiccrunch_server_free().
 */
QuicTransportHandle *quiccrunch_server_new(void);

/**
 * Create a new QUIC server with TLS configuration
 * 
 * # Arguments
 * * `tls_config` - TLS configuration for the server
 * 
 * Returns a handle to the server, or null on error.
 */
QuicTransportHandle *quiccrunch_server_new_with_tls(const QuicTlsConfig *tls_config);

/**
 * Free a QUIC server handle
 * 
 * # Safety
 * The handle must have been created by quiccrunch_server_new() and
 * must not be used after calling this function.
 */
void quiccrunch_server_free(QuicTransportHandle *handle);

/**
 * Initialize the server (creates the underlying transport)
 * 
 * This must be called before starting to listen.
 * 
 * # Arguments
 * * `handle` - Server handle
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_server_init(QuicTransportHandle *handle);

/**
 * Set a message handler for the server
 * 
 * # Arguments
 * * `handle` - Server handle
 * * `handler` - Message handler callback function
 * * `user_data` - User data to pass to the handler
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_server_set_message_handler(QuicTransportHandle *handle,
                                                QuicMessageHandler handler,
                                                void *user_data);

/**
 * Start listening on an address
 * 
 * # Arguments
 * * `handle` - Server handle
 * * `address` - Address to listen on (e.g., "127.0.0.1:50051")
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_server_listen(QuicTransportHandle *handle, const char *address);

/**
 * Start serving (accept connections and handle messages)
 * 
 * This function runs the server loop. It will block until quiccrunch_server_stop() is called
 * or an error occurs. In a real application, this should be called in a separate thread.
 * 
 * # Arguments
 * * `handle` - Server handle
 * 
 * Returns QuicError::Success when stopped normally, error code on failure.
 */
QuicError quiccrunch_server_serve(QuicTransportHandle *handle);

/**
 * Start serving asynchronously
 * 
 * This starts the server in the background and returns immediately.
 * 
 * # Arguments
 * * `handle` - Server handle
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_server_serve_async(QuicTransportHandle *handle);

/**
 * Stop the server
 * 
 * # Arguments
 * * `handle` - Server handle
 * 
 * Returns QuicError::Success on success, error code on failure.
 */
QuicError quiccrunch_server_stop(QuicTransportHandle *handle);

/**
 * Get the number of active connections
 * 
 * # Arguments
 * * `handle` - Server handle
 * 
 * Returns the number of active connections, or 0 if handle is invalid.
 */
uint32_t quiccrunch_server_connection_count(QuicTransportHandle *handle);

#ifdef __cplusplus
}
#endif

#endif /* QUICCRUNCH_H */