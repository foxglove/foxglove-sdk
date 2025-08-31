#define FOXGLOVE_DATA_LOADER_IMPLEMENTATION
#include "foxglove_data_loader/data_loader.hpp"

#include <memory>
#include <sstream>

#include "foxglove/schemas.hpp"

using namespace foxglove_data_loader;

struct LineIndex {
  uint16_t file;
  size_t start;
  size_t end;
};

std::string print_inner(std::stringstream& ss) {
  return ss.str();
}

template<typename T, typename... Types>
std::string print_inner(std::stringstream& ss, T var1, Types... rest) {
  ss << " " << var1;
  return print_inner(ss, rest...);
}

template<typename... Types>
void log(Types... vars) {
  std::stringstream ss;
  std::string as_string = print_inner(ss, vars...);
  console_log(as_string.c_str());
}

template<typename... Types>
void warn(Types... vars) {
  std::stringstream ss;
  std::string as_string = print_inner(ss, vars...);
  console_warn(as_string.c_str());
}

template<typename... Types>
void error(Types... vars) {
  std::stringstream ss;
  std::string as_string = print_inner(ss, vars...);
  console_error(as_string.c_str());
}

/** A simple data loader implementation that loads text files and yields each line as a message.
 * This data loader is initialized with a set of text files, which it reads into memory.
 * `create_iterator` returns an iterator which iterates over each file line-by-line, assigning
 * sequential timestamps starting from zero. Each line message uses its filename as its topic name.
 */
class TextDataLoader : public foxglove_data_loader::AbstractDataLoader {
public:
  std::vector<std::string> paths;
  std::vector<std::vector<uint8_t>> files;
  std::vector<LineIndex> line_indexes;
  std::vector<size_t> file_line_counts;

  TextDataLoader(std::vector<std::string> paths);

  Result<Initialization> initialize() override;

  Result<std::unique_ptr<AbstractMessageIterator>> create_iterator(const MessageIteratorArgs& args
  ) override;
};

/** Iterates over 'messages' that match the requested args. */
class TextMessageIterator : public foxglove_data_loader::AbstractMessageIterator {
  TextDataLoader* data_loader;
  MessageIteratorArgs args;
  size_t index;
  foxglove::schemas::Log message;
  std::vector<uint8_t> last_encoded_message;

public:
  explicit TextMessageIterator(TextDataLoader* loader, MessageIteratorArgs args_);
  std::optional<Result<Message>> next() override;
};

TextDataLoader::TextDataLoader(std::vector<std::string> paths) {
  this->paths = paths;
}

/** initialize() is meant to read and return summary information to the foxglove
 * application about the set of files being read. The loader should also read any index information
 * that it needs to iterate over messages in initialize(). For simplicity, this loader reads entire
 * input files and indexes their line endings, but more sophisticated formats should not need to
 * be read from front to back.
 */
Result<Initialization> TextDataLoader::initialize() {
  std::vector<Channel> channels;
  for (uint16_t file_index = 0; file_index < paths.size(); file_index++) {
    const std::string& path = paths[file_index];
    Reader reader = Reader::open(path.c_str());
    uint64_t size = reader.size();
    std::vector<uint8_t> buf(size);
    uint64_t n_read = reader.read(buf.data(), size);

    if (n_read != size) {
      return Result<Initialization>::error_with_message("could not read entire file");
    }
    if (reader.position() != size) {
      return Result<Initialization>::error_with_message("expected reader cursor to be at EOF");
    }
    size_t line_count = 0;
    size_t line_start = 0;
    for (size_t pos = 0; pos < size; pos++) {
      if (buf[pos] == '\n') {
        this->line_indexes.push_back(LineIndex{file_index, line_start, pos});
        line_start = pos + 1;
        line_count += 1;
      }
    }
    if (line_start < size) {
      this->line_indexes.push_back(LineIndex{file_index, line_start, size_t(size - 1)});
    }
    this->files.emplace_back(buf);
    uint16_t channel_id = file_index;
    channels.push_back(Channel{
      .id = channel_id,
      .schema_id = 1,
      .topic_name = "/log",
      .message_encoding = "protobuf",
      .message_count = line_count,
    });
  }
  foxglove::Schema log_schema = foxglove::schemas::Log::schema();
  return Result<Initialization>{
    .value =
      Initialization{
        .channels = channels,
        .schemas = {Schema{
          .id = 1,
          .name = log_schema.name,
          .encoding = log_schema.encoding,
          .data =
            BytesView{
              .ptr = reinterpret_cast<const uint8_t*>(log_schema.data),
              .len = log_schema.data_len,
            }
        }},
        .time_range =
          TimeRange{
            .start_time = 0,
            .end_time = this->line_indexes.size(),
          }
      }
  };
}
/** returns an AbstractMessageIterator for the set of requested args.
 * More than one message iterator may be instantiated at a given time.
 */
Result<std::unique_ptr<AbstractMessageIterator>> TextDataLoader::create_iterator(
  const MessageIteratorArgs& args
) {
  return Result<std::unique_ptr<AbstractMessageIterator>>{
    .value = std::make_unique<TextMessageIterator>(this, args),
  };
}

TextMessageIterator::TextMessageIterator(TextDataLoader* loader, MessageIteratorArgs args_) {
  data_loader = loader;
  args = args_;
  index = 0;
  message = foxglove::schemas::Log{};
  last_encoded_message = std::vector<uint8_t>(1024);
}

/** `next()` returns the next message from the loaded files that matches the arguments provided to
 * `create_iterator(args)`. If none are left to read, it returns std::nullopt.
 */
std::optional<Result<Message>> TextMessageIterator::next() {
  for (; index < data_loader->line_indexes.size(); index++) {
    TimeNanos time = index;
    // skip lines before start time
    if (args.start_time.has_value() && args.start_time > time) {
      continue;
    }
    // if the end time is before the current line, stop iterating
    if (args.end_time.has_value() && args.end_time < time) {
      return std::nullopt;
    }

    LineIndex line = data_loader->line_indexes[index];
    // filter by channel ID
    for (const ChannelId channel_id : args.channel_ids) {
      if (channel_id == line.file) {
        message.file = data_loader->paths[line.file];
        message.level = foxglove::schemas::Log::LogLevel::INFO;
        message.name = "log line";
        message.line = index;
        message.message = std::string(
          reinterpret_cast<const char*>(&(data_loader->files[line.file][line.start])),
          line.end - line.start
        );
        size_t encoded_len = 0;

        auto result =
          message.encode(last_encoded_message.data(), last_encoded_message.size(), &encoded_len);
        if (result == foxglove::FoxgloveError::BufferTooShort) {
          last_encoded_message.resize(encoded_len);
          result =
            message.encode(last_encoded_message.data(), last_encoded_message.size(), &encoded_len);
        }
        if (result != foxglove::FoxgloveError::Ok) {
          error("failed to encode message:", foxglove::strerror(result));
          return Result<Message>{.error = "failed to encode message"};
        }
        index++;
        return Result<Message>{
          .value =
            Message{
              .channel_id = channel_id,
              .log_time = time,
              .publish_time = time,
              .data =
                BytesView{
                  .ptr = last_encoded_message.data(),
                  .len = encoded_len,
                }
            }
        };
      }
    }
  }
  return std::nullopt;
}

/** `construct_data_loader` is the hook you implement to load your data loader implementation. */
std::unique_ptr<AbstractDataLoader> construct_data_loader(const DataLoaderArgs& args) {
  return std::make_unique<TextDataLoader>(args.paths);
}
