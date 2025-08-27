#define FOXGLOVE_DATA_LOADER_IMPLEMENTATION
#include "foxglove_data_loader/data_loader.hpp"
#include "foxglove/schemas.hpp"

#include <memory>
#include <sstream>

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
  std::vector<LineIndex> file_line_indexes;
  std::vector<size_t> file_line_counts;

  TextDataLoader(std::vector<std::string> paths);

  Result<Initialization> initialize() override;

  Result<std::unique_ptr<AbstractMessageIterator>> create_iterator(const MessageIteratorArgs& args
  ) override;
  Result<std::vector<Message>> get_backfill(const BackfillArgs& args) override;
};

/** Iterates over 'messages' that match the requested args. */
class TextMessageIterator : public foxglove_data_loader::AbstractMessageIterator {
  TextDataLoader* data_loader;
  MessageIteratorArgs args;
  size_t index;

public:
  explicit TextMessageIterator(TextDataLoader* loader, MessageIteratorArgs args_);
  std::optional<Result<Message>> next() override;
};

TextDataLoader::TextDataLoader(std::vector<std::string> paths) {
  this->paths = paths;
  this->files = {};
  this->file_line_indexes = {};
  this->file_line_counts = {};
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
    size_t line_count = 1;
    size_t last_line_ending = 0;
    for (size_t pos = 0; pos < size; pos++) {
      if (buf[pos] == '\n') {
        this->file_line_indexes.push_back(LineIndex{file_index, last_line_ending + 1, pos});
        last_line_ending = pos;
        line_count += 1;
      }
    }
    if (last_line_ending < (size - 1)) {
      this->file_line_indexes.push_back(
        LineIndex{file_index, last_line_ending + 1, size_t(size - 1)}
      );
    }
    this->files.emplace_back(buf);
    uint16_t channel_id = file_index;
    channels.push_back(Channel{
      .id = channel_id,
      .schema_id = 0,
      .topic_name = path,
      .message_encoding = "json",
      .message_count = line_count,
    });
  }
  return Result<Initialization>{
    .value =
      Initialization{
        .channels = channels,
        .schemas = {},
        .time_range =
          TimeRange{
            .start_time = 0,
            .end_time = this->file_line_indexes.size(),
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

/** Returns the latest message before `args.time` on the requested channels. This is used by the
 * Foxglove app to display up the state of the scene at the beginning of a requested time range,
 * before any of the messages from that time range have been read.
 */
Result<std::vector<Message>> TextDataLoader::get_backfill(const BackfillArgs& args) {
  std::vector<Message> results = {};
  for (const uint16_t id : args.channel_ids) {
    std::optional<size_t> to_push = std::nullopt;
    for (size_t message_index = 0; message_index < file_line_indexes.size(); message_index++) {
      TimeNanos time = message_index;
      LineIndex line = file_line_indexes[message_index];
      if (line.file == id) {
        if (time > args.time) {
          break;
        }
        to_push = time;
      }
    }
    if (to_push.has_value()) {
      TimeNanos time = *to_push;
      LineIndex line = file_line_indexes[*to_push];
      results.push_back(Message{
        .channel_id = id,
        .log_time = time,
        .publish_time = time,
        .data =
          BytesView{
            .ptr = &files[line.file][line.start],
            .len = line.end - line.start,
          }
      });
    }
  }
  return Result<std::vector<Message>>{
    .value = results,
  };
}

TextMessageIterator::TextMessageIterator(TextDataLoader* loader, MessageIteratorArgs args_) {
  data_loader = loader;
  args = args_;
  index = 0;
}

/** `next()` returns the next message from the loaded files that matches the arguments provided to
 * `create_iterator(args)`. If none are left to read, it returns std::nullopt.
 */
std::optional<Result<Message>> TextMessageIterator::next() {
  for (; index < data_loader->file_line_indexes.size(); index++) {
    TimeNanos time = index;
    // skip lines before start time
    if (args.start_time.has_value() && args.start_time > time) {
      continue;
    }
    // if the end time is before the current line, stop iterating
    if (args.end_time.has_value() && args.end_time < time) {
      return std::nullopt;
    }

    LineIndex line = data_loader->file_line_indexes[index];
    // filter by channel ID
    for (const ChannelId channel_id : args.channel_ids) {
      if (channel_id == line.file) {
        return Result<Message>{
          .value =
            Message{
              .channel_id = channel_id,
              .log_time = time,
              .publish_time = time,
              .data =
                BytesView{
                  .ptr = &data_loader->files[line.file][line.start],
                  .len = line.end - line.start,
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
