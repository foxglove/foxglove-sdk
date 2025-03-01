#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;

namespace foxglove {

struct Schema {
  std::string_view name;
  std::string_view encoding;
  const std::byte* data;
  size_t dataLen;
};

class Channel final {
public:
  Channel(std::string topic, std::string messageEncoding, std::optional<Schema> schema);

  void log(
    const std::byte* data, size_t dataLen, uint64_t logTime, uint64_t publishTime, uint32_t sequence
  );

private:
  std::unique_ptr<foxglove_channel, void (*)(foxglove_channel*)> _impl;
};

}  // namespace foxglove
