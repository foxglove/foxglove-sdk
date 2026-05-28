#ifndef FOXGLOVE_DATA_LOADER_HPP
#define FOXGLOVE_DATA_LOADER_HPP

#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace foxglove_data_loader {

const char* const SDK_VERSION = "0.1.0";

typedef uint16_t ChannelId;
typedef uint16_t SchemaId;

/** Nanosecond timestamp since a user-defined epoch (most commonly either the unix epoch or system
 * boot) */
typedef uint64_t TimeNanos;

struct BytesView {
  const uint8_t* ptr;
  size_t len;
};

/** Metadata about a channel of messages. Your data loader reads input files
 * and produces messages on one or more logical channels, which may differ in topic name,
 * message encoding, or message definition schema information.
 */
struct Channel {
  ChannelId id;
  /** The ID of the schema for this channel. If no schema is required to decode messages
   * because they use a schemaless encoding eg. JSON, leave this empty. Schema ID 0 is reserved
   * and may not be used.
   */
  std::optional<SchemaId> schema_id;
  /** The topic name for this channel. Multiple channels may share the same topic name. */
  std::string topic_name;
  /** The message encoding for this channel. Must match
   * one of the well-known message encodings here: https://mcap.dev/spec/registry
   */
  std::string message_encoding;
  /** The number of messages in the given file(s) for this channel. Leave this empty if
   * your data source cannot easily determine this without reading the whole file.
   */
  std::optional<uint64_t> message_count;
};

/** Defines the schema for one or more channels of messages. */
struct Schema {
  SchemaId id;
  /** A name that identifies the 'type' that this schema describes.  */
  std::string name;
  /** The encoding used to encode the schema definition into `data`. Must match
   * one of the well-known schema encodings here: https://mcap.dev/spec/registry
   */
  std::string encoding;
  /** The serialized schema definition. The underlying pointer should remain valid after
   * `initialize()` returns. It may be invalidated as soon as control is returned to the data
   * loader.
   */
  BytesView data;
};

/** An inclusive time range. */
struct TimeRange {
  TimeNanos start_time;
  TimeNanos end_time;
};

struct Problem {
  int severity;
  std::string message;
  std::optional<std::string> tip;
};

struct Initialization {
  /** All channels available in the input file(s). Channel IDs must be unique. */
  std::vector<Channel> channels;
  /** All schemas available in the input file(s). Schema IDs must be unique and nonzero. */
  std::vector<Schema> schemas;
  /** The inclusive message log_time range covered by all files provided as arguments to the data
   * loader. */
  TimeRange time_range;
  /** any data validation problems encountered when initializing the data source. */
  std::vector<Problem> problems;
};

/** A simple Result wrapper. */
template<typename T>
struct Result {
  std::optional<T> value;
  std::string error;

  /** Constructs a new error-valued Result with a message. */
  static Result<T> error_with_message(std::string message) {
    return Result<T>{
      .value = std::nullopt,
      .error = message,
    };
  }

  /** Retrieves a reference to the value, returning a null reference if this is an error result. */
  const T& get() const {
    return value.value();
  }

  /** Returns true if this is an OK result, false if error. */
  bool ok() const {
    return value.has_value();
  }
};

/** A message yielded by your data loader. */
struct Message {
  ChannelId channel_id;
  /** The time when this message was logged to the file. */
  TimeNanos log_time;
  /** The time when this message was published by its source.
   * If not known, set this to log_time.
   * */
  TimeNanos publish_time;
  /** The serialized message data. The underlying pointer should remain valid after `next()` or
   * `get_backfill()` returns. It may be invalidated as soon as control is returned to the data
   * loader.
   */
  BytesView data;
};

struct MessageIteratorArgs {
  /** Yield only messages with these channel IDs. */
  std::vector<ChannelId> channel_ids;
  /** if non-empty, only messages on or after this log time should be yielded. */
  std::optional<TimeNanos> start_time;
  /** if non-empty, only messages on or before should be yielded. */
  std::optional<TimeNanos> end_time;
};

struct BackfillArgs {
  /** For every given channel ID, retrieve the latest message available in the file(s) for that
   * channel that has log_time before or at this timestamp.
   */
  TimeNanos time;
  std::vector<ChannelId> channel_ids;
};

struct DataLoaderArgs {
  /** The set of files that this data loader should return messages from. */
  std::vector<std::string> paths;
};

/**
 * A file reader resource. This API does not provide I/O errors to the data loader,
 * these are handled by the host.
 */
class Reader {
private:
  int32_t handle;
  Reader(int32_t handle_) {
    handle = handle_;
  }

public:
  static Reader open(const char* path);
  /** Seek to this position in the file. `pos` is an offset from the start of file. */
  uint64_t seek(uint64_t pos);
  /** Get the size of the file. */
  uint64_t size();
  /** Get the current cursor position in the file. */
  uint64_t position();
  /** read up to `len` bytes into `target`, returning the number of bytes successfully read.
   */
  uint64_t read(uint8_t* target, size_t len);
};

/** Logs an info-level diagnostic message to the console. */
void console_log(const char* msg);

/** Logs a warn-level diagnostic message to the console. */
void console_warn(const char* msg);

/** Logs an error-level diagnostic message to the console. */
void console_error(const char* msg);

/** Defines the interface for a message iterator that your data loader will implement. */
class AbstractMessageIterator {
public:
  /** Return the next message from the set of files being read.
   *
   * Messages should be returned in order of their log_times.
   * std::nullopt indicates that no more messages can be read.
   * */
  virtual std::optional<Result<Message>> next() = 0;
  virtual ~AbstractMessageIterator() {};
};

class AbstractDataLoader {
public:
  /** Read summary information about the input files. */
  virtual Result<Initialization> initialize() = 0;
  /** Start iterating over messages in the input file(s). More than one iterator may be instantiated
   * at a time.
   */
  virtual Result<std::unique_ptr<AbstractMessageIterator>> create_iterator(
    const MessageIteratorArgs& args
  ) = 0;
  /** Get the latest message before the requested `time` for each channel, if present. The default
   * implementation returns no messages. You may implement this to improve the experience of seeking
   * within a recording.
   */
  virtual Result<std::vector<Message>> get_backfill(const BackfillArgs& args);
  virtual ~AbstractDataLoader() {}
};

}  // namespace foxglove_data_loader

/** Constructs a new data loader for the given arguments.
 * Your data loader module must implement this function.
 */
std::unique_ptr<foxglove_data_loader::AbstractDataLoader> construct_data_loader(
  const foxglove_data_loader::DataLoaderArgs& args
);

#ifdef FOXGLOVE_DATA_LOADER_IMPLEMENTATION
#include "data_loader.inl"
#include "host_internal.inl"
#endif
#endif
