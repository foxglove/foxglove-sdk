#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;

namespace foxglove {

struct Context;

struct Schema {
  std::string name;
  std::string encoding;
  const std::byte* data = nullptr;
  size_t dataLen = 0;
};

class Channel final {
public:
  Channel(
    const std::string& topic, const std::string& messageEncoding,
    std::optional<Schema> schema = std::nullopt,
    std::optional<const Context&> context = std::nullopt
  );

  void log(
    const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime = std::nullopt,
    std::optional<uint64_t> publishTime = std::nullopt,
    std::optional<uint32_t> sequence = std::nullopt
  );

  uint64_t id() const;

private:
  std::unique_ptr<foxglove_channel, void (*)(foxglove_channel*)> _impl;
};

}  // namespace foxglove
