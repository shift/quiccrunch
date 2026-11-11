/* QuicCrunch FFI C Headers */

/* Generated with cbindgen:0.26.0 */

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Error codes for QuicCrunch FFI operations
 */
typedef enum QuicError {
  /**
   * Operation completed successfully
   */
  Success = 0,
  /**
   * Generic error occurred
   */
  Error = 1,
  /**
   * Invalid argument passed to function
   */
  InvalidArgument = 2,
  /**
   * Connection failed
   */
  ConnectionFailed = 3,
  /**
   * Operation timed out
   */
  TimeoutError = 4,
  /**
   * Serialization/deserialization error
   */
  SerializationError = 5,
  /**
   * TLS/certificate error
   */
  TlsError = 6,
  /**
   * Address parsing error
   */
  AddressError = 7,
  /**
   * Out of memory
   */
  OutOfMemory = 8,
  /**
   * Invalid handle/pointer
   */
  InvalidHandle = 9,
  /**
   * Operation would block (for non-blocking operations)
   */
  WouldBlock = 10,
  /**
   * Stream was closed
   */
  StreamClosed = 11,
  /**
   * Not implemented
   */
  NotImplemented = 12,
} QuicError;

/**
 * Opaque handle to a QuicTransport
 */
typedef struct QuicTransportHandle {
  uint8_t mPrivate[0];
} QuicTransportHandle;

/**
 * Configuration for TLS
 */
typedef struct QuicTlsConfig {
  /**
   * Path to certificate file (PEM format)
   */
  const char *mCertPath;
  /**
   * Path to private key file (PEM format)
   */
  const char *mKeyPath;
  /**
   * Path to CA certificate file (PEM format) - optional, can be null
   */
  const char *mCaPath;
} QuicTlsConfig;

/**
 * Callback function type for async operations
 *
 * Parameters:
 * - user_data: User-provided data pointer
 * - error: Error code (QuicError::Success if no error)
 * - result_data: Operation-specific result data (can be null)
 */
typedef void (*QuicCallback)(void *user_data, enum QuicError error, void *result_data);

/**
 * Message data structure for sending/receiving
 */
typedef struct QuicMessage {
  /**
   * Pointer to message data
   */
  const uint8_t *mData;
  /**
   * Length of message data in bytes
   */
  uintptr_t mLen;
} QuicMessage;

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
typedef enum QuicError (*QuicMessageHandler)(void *user_data,
                                             uint32_t connection_id,
                                             const struct QuicMessage *message,
                                             struct QuicMessage *response);

/**
 * Connection configuration
 */
typedef struct QuicConnectionConfig {
  /**
   * Maximum number of concurrent streams
   */
  uint32_t mMaxConcurrentStreams;
  /**
   * Connection timeout in milliseconds
   */
  uint32_t mConnectionTimeoutMs;
  /**
   * Keep alive interval in milliseconds (0 to disable)
   */
  uint32_t mKeepAliveMs;
} QuicConnectionConfig;

/**
 * Initialize the QuicCrunch FFI library.
 * This must be called before using any other FFI functions.
 *
 * Returns 0 on success, non-zero on error.
 */
quiccrunch_ enum QuicError quiccrunch_init(void) ;

/**
 * Clean up and shutdown the QuicCrunch FFI library.
 * This should be called when done using the library.
 */
quiccrunch_ enum QuicError quiccrunch_shutdown(void) ;

/**
 * Free a string that was allocated by QuicCrunch FFI functions.
 *
 * # Safety
 * The string pointer must have been returned by a QuicCrunch FFI function
 * and must not be used after calling this function.
 */
quiccrunch_ void quiccrunch_string_free(char *aS) ;

/**
 * Get the version of the QuicCrunch library.
 * Returns a null-terminated string that must be freed with quiccrunch_string_free().
 */
quiccrunch_ char *quiccrunch_version(void) ;

/**
 * Create a new QUIC client
 *
 * Returns a handle to the client, or null on error.
 * The handle must be freed with quiccrunch_client_free().
 */
quiccrunch_ struct QuicTransportHandle *quiccrunch_client_new(void) ;

/**
 * Create a new QUIC client with TLS configuration
 *
 * # Arguments
 * * `tls_config` - TLS configuration for the client
 *
 * Returns a handle to the client, or null on error.
 */
quiccrunch_
struct QuicTransportHandle *quiccrunch_client_new_with_tls(const struct QuicTlsConfig *aTlsConfig)
;

/**
 * Free a QUIC client handle
 *
 * # Safety
 * The handle must have been created by quiccrunch_client_new() and
 * must not be used after calling this function.
 */
quiccrunch_ void quiccrunch_client_free(struct QuicTransportHandle *aHandle) ;

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
quiccrunch_ enum QuicError quiccrunch_client_init(struct QuicTransportHandle *aHandle) ;

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
quiccrunch_
enum QuicError quiccrunch_client_connect_async(struct QuicTransportHandle *aHandle,
                                               const char *aAddress,
                                               QuicCallback aCallback,
                                               void *aUserData)
;

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
quiccrunch_
enum QuicError quiccrunch_client_connect(struct QuicTransportHandle *aHandle,
                                         const char *aAddress,
                                         uint32_t *aConnectionId)
;

/**
 * Disconnect from a server
 *
 * # Arguments
 * * `handle` - Client handle
 * * `connection_id` - Connection ID to disconnect
 *
 * Returns QuicError::Success on success, error code on failure.
 */
quiccrunch_
enum QuicError quiccrunch_client_disconnect(struct QuicTransportHandle *aHandle,
                                            uint32_t aConnectionId)
;

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
quiccrunch_
enum QuicError quiccrunch_client_send_message(struct QuicTransportHandle *aHandle,
                                              uint32_t aConnectionId,
                                              const struct QuicMessage *aMessage)
;

/**
 * Get the number of active connections
 *
 * # Arguments
 * * `handle` - Client handle
 *
 * Returns the number of active connections, or 0 if handle is invalid.
 */
quiccrunch_ uint32_t quiccrunch_client_connection_count(struct QuicTransportHandle *aHandle) ;

/**
 * Create a new QUIC server
 *
 * Returns a handle to the server, or null on error.
 * The handle must be freed with quiccrunch_server_free().
 */
quiccrunch_ struct QuicTransportHandle *quiccrunch_server_new(void) ;

/**
 * Create a new QUIC server with TLS configuration
 *
 * # Arguments
 * * `tls_config` - TLS configuration for the server
 *
 * Returns a handle to the server, or null on error.
 */
quiccrunch_
struct QuicTransportHandle *quiccrunch_server_new_with_tls(const struct QuicTlsConfig *aTlsConfig)
;

/**
 * Free a QUIC server handle
 *
 * # Safety
 * The handle must have been created by quiccrunch_server_new() and
 * must not be used after calling this function.
 */
quiccrunch_ void quiccrunch_server_free(struct QuicTransportHandle *aHandle) ;

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
quiccrunch_ enum QuicError quiccrunch_server_init(struct QuicTransportHandle *aHandle) ;

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
quiccrunch_
enum QuicError quiccrunch_server_set_message_handler(struct QuicTransportHandle *aHandle,
                                                     QuicMessageHandler aHandler,
                                                     void *aUserData)
;

/**
 * Start listening on an address
 *
 * # Arguments
 * * `handle` - Server handle
 * * `address` - Address to listen on (e.g., "127.0.0.1:50051")
 *
 * Returns QuicError::Success on success, error code on failure.
 */
quiccrunch_
enum QuicError quiccrunch_server_listen(struct QuicTransportHandle *aHandle,
                                        const char *aAddress)
;

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
quiccrunch_ enum QuicError quiccrunch_server_serve(struct QuicTransportHandle *aHandle) ;

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
quiccrunch_ enum QuicError quiccrunch_server_serve_async(struct QuicTransportHandle *aHandle) ;

/**
 * Stop the server
 *
 * # Arguments
 * * `handle` - Server handle
 *
 * Returns QuicError::Success on success, error code on failure.
 */
quiccrunch_ enum QuicError quiccrunch_server_stop(struct QuicTransportHandle *aHandle) ;

/**
 * Get the number of active connections
 *
 * # Arguments
 * * `handle` - Server handle
 *
 * Returns the number of active connections, or 0 if handle is invalid.
 */
quiccrunch_ uint32_t quiccrunch_server_connection_count(struct QuicTransportHandle *aHandle) ;
