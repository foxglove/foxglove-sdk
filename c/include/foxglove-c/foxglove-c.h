/*
 * NOTE: This file is autogenerated by cbindgen.
 *
 * Foxglove SDK
 * https://github.com/foxglove/foxglove-sdk
 */


#ifndef FOXGLOVE_H
#define FOXGLOVE_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#ifndef FOXGLOVE_NONNULL
#define FOXGLOVE_NONNULL
#endif


/**
 * Allow clients to advertise channels to send data messages to the server.
 */
#define FOXGLOVE_SERVER_CAPABILITY_CLIENT_PUBLISH (1 << 0)

/**
 * Allow clients to subscribe and make connection graph updates
 */
#define FOXGLOVE_SERVER_CAPABILITY_CONNECTION_GRAPH (1 << 1)

/**
 * Allow clients to get & set parameters.
 */
#define FOXGLOVE_SERVER_CAPABILITY_PARAMETERS (1 << 2)

/**
 * Inform clients about the latest server time.
 *
 * This allows accelerated, slowed, or stepped control over the progress of time. If the
 * server publishes time data, then timestamps of published messages must originate from the
 * same time source.
 */
#define FOXGLOVE_SERVER_CAPABILITY_TIME (1 << 3)

/**
 * Allow clients to call services.
 */
#define FOXGLOVE_SERVER_CAPABILITY_SERVICES (1 << 4)

enum foxglove_error
#ifdef __cplusplus
  : uint8_t
#endif // __cplusplus
 {
  FOXGLOVE_ERROR_OK,
  FOXGLOVE_ERROR_UNSPECIFIED,
  FOXGLOVE_ERROR_VALUE_ERROR,
  FOXGLOVE_ERROR_UTF8_ERROR,
  FOXGLOVE_ERROR_SINK_CLOSED,
  FOXGLOVE_ERROR_SCHEMA_REQUIRED,
  FOXGLOVE_ERROR_MESSAGE_ENCODING_REQUIRED,
  FOXGLOVE_ERROR_SERVER_ALREADY_STARTED,
  FOXGLOVE_ERROR_BIND,
  FOXGLOVE_ERROR_DUPLICATE_CHANNEL,
  FOXGLOVE_ERROR_DUPLICATE_SERVICE,
  FOXGLOVE_ERROR_MISSING_REQUEST_ENCODING,
  FOXGLOVE_ERROR_SERVICES_NOT_SUPPORTED,
  FOXGLOVE_ERROR_CONNECTION_GRAPH_NOT_SUPPORTED,
  FOXGLOVE_ERROR_IO_ERROR,
  FOXGLOVE_ERROR_MCAP_ERROR,
};
#ifndef __cplusplus
typedef uint8_t foxglove_error;
#endif // __cplusplus

enum foxglove_mcap_compression
#ifdef __cplusplus
  : uint8_t
#endif // __cplusplus
 {
  FOXGLOVE_MCAP_COMPRESSION_NONE,
  FOXGLOVE_MCAP_COMPRESSION_ZSTD,
  FOXGLOVE_MCAP_COMPRESSION_LZ4,
};
#ifndef __cplusplus
typedef uint8_t foxglove_mcap_compression;
#endif // __cplusplus

typedef struct foxglove_channel foxglove_channel;

typedef struct foxglove_mcap_writer foxglove_mcap_writer;

typedef struct foxglove_websocket_server foxglove_websocket_server;

/**
 * A string with associated length.
 */
typedef struct foxglove_string {
  /**
   * Pointer to valid UTF-8 data
   */
  const char *data;
  /**
   * Number of bytes in the string
   */
  size_t len;
} foxglove_string;

typedef struct foxglove_client_channel {
  uint32_t id;
  const char *topic;
  const char *encoding;
  const char *schema_name;
  const char *schema_encoding;
  const void *schema;
  size_t schema_len;
} foxglove_client_channel;

typedef struct foxglove_server_callbacks {
  /**
   * A user-defined value that will be passed to callback functions
   */
  const void *context;
  void (*on_subscribe)(uint64_t channel_id, const void *context);
  void (*on_unsubscribe)(uint64_t channel_id, const void *context);
  void (*on_client_advertise)(uint32_t client_id,
                              const struct foxglove_client_channel *channel,
                              const void *context);
  void (*on_message_data)(uint32_t client_id,
                          uint32_t client_channel_id,
                          const uint8_t *payload,
                          size_t payload_len,
                          const void *context);
  void (*on_client_unadvertise)(uint32_t client_id, uint32_t client_channel_id, const void *context);
} foxglove_server_callbacks;

typedef uint8_t foxglove_server_capability;

typedef struct foxglove_server_options {
  struct foxglove_string name;
  struct foxglove_string host;
  uint16_t port;
  const struct foxglove_server_callbacks *callbacks;
  foxglove_server_capability capabilities;
  const struct foxglove_string *supported_encodings;
  size_t supported_encodings_count;
} foxglove_server_options;

typedef struct foxglove_mcap_options {
  struct foxglove_string path;
  size_t path_len;
  bool truncate;
  foxglove_mcap_compression compression;
  struct foxglove_string profile;
  /**
   * chunk_size of 0 is treated as if it was omitted (None)
   */
  uint64_t chunk_size;
  bool use_chunks;
  bool disable_seeking;
  bool emit_statistics;
  bool emit_summary_offsets;
  bool emit_message_indexes;
  bool emit_chunk_indexes;
  bool emit_attachment_indexes;
  bool emit_metadata_indexes;
  bool repeat_channels;
  bool repeat_schemas;
} foxglove_mcap_options;

typedef struct foxglove_schema {
  struct foxglove_string name;
  struct foxglove_string encoding;
  const uint8_t *data;
  size_t data_len;
} foxglove_schema;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * Create and start a server.
 * Resources must later be freed by calling `foxglove_server_stop`.
 *
 * `port` may be 0, in which case an available port will be automatically selected.
 *
 * Returns 0 on success, or returns a FoxgloveError code on error.
 *
 * # Safety
 * If `name` is supplied in options, it must contain valid UTF8.
 * If `host` is supplied in options, it must contain valid UTF8.
 * If `supported_encodings` is supplied in options, all `supported_encodings` must contain valid
 * UTF8, and `supported_encodings` must have length equal to `supported_encodings_count`.
 */
foxglove_error foxglove_server_start(const struct foxglove_server_options *FOXGLOVE_NONNULL options,
                                     struct foxglove_websocket_server **server);

/**
 * Get the port on which the server is listening.
 */
uint16_t foxglove_server_get_port(struct foxglove_websocket_server *server);

/**
 * Stop and shut down `server` and free the resources associated with it.
 */
foxglove_error foxglove_server_stop(struct foxglove_websocket_server *server);

/**
 * Create or open an MCAP file for writing.
 * Resources must later be freed with `foxglove_mcap_close`.
 *
 * Returns 0 on success, or returns a FoxgloveError code on error.
 *
 * # Safety
 * `path` and `profile` must contain valid UTF8.
 */
foxglove_error foxglove_mcap_open(const struct foxglove_mcap_options *FOXGLOVE_NONNULL options,
                                  struct foxglove_mcap_writer **writer);

/**
 * Close an MCAP file writer created via `foxglove_mcap_open`.
 *
 * Returns 0 on success, or returns a FoxgloveError code on error.
 *
 * # Safety
 * `writer` must be a valid pointer to a `FoxgloveMcapWriter` created via `foxglove_mcap_open`.
 */
foxglove_error foxglove_mcap_close(struct foxglove_mcap_writer *writer);

/**
 * Create a new channel. The channel must later be freed with `foxglove_channel_free`.
 *
 * Returns 0 on success, or returns a FoxgloveError code on error.
 *
 * # Safety
 * `topic` and `message_encoding` must contain valid UTF8. `schema` is an optional pointer to a
 * schema. The schema and the data it points to need only remain alive for the duration of this
 * function call (they will be copied).
 */
foxglove_error foxglove_channel_create(struct foxglove_string topic,
                                       struct foxglove_string message_encoding,
                                       const struct foxglove_schema *schema,
                                       const struct foxglove_channel **channel);

/**
 * Free a channel created via `foxglove_channel_create`.
 * # Safety
 * `channel` must be a valid pointer to a `FoxgloveChannel` created via `foxglove_channel_create`.
 * If channel is null, this does nothing.
 */
void foxglove_channel_free(const struct foxglove_channel *channel);

/**
 * Get the ID of a channel.
 *
 * # Safety
 * `channel` must be a valid pointer to a `FoxgloveChannel` created via `foxglove_channel_create`.
 *
 * If the passed channel is null, an invalid id of 0 is returned.
 */
uint64_t foxglove_channel_get_id(const struct foxglove_channel *channel);

/**
 * Log a message on a channel.
 *
 * # Safety
 * `data` must be non-null, and the range `[data, data + data_len)` must contain initialized data
 * contained within a single allocated object.
 *
 * `log_time` may be null or may point to a valid value.
 */
foxglove_error foxglove_channel_log(const struct foxglove_channel *channel,
                                    const uint8_t *data,
                                    size_t data_len,
                                    const uint64_t *log_time);

/**
 * For use by the C++ SDK. Identifies that wrapper as the source of logs.
 */
void foxglove_internal_register_cpp_wrapper(void);

/**
 * Convert a `FoxgloveError` code to a C string.
 */
const char *foxglove_error_to_cstr(foxglove_error error);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* FOXGLOVE_H */
