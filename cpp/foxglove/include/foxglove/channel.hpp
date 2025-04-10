#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;
struct foxglove_context;

namespace foxglove {

struct Context;
typedef foxglove_context ContextInner;

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
    std::optional<Schema> schema = std::nullopt
  )
      : Channel(topic, messageEncoding, schema, nullptr) {}

  Channel(
    const std::string& topic, const std::string& messageEncoding, Schema schema,
    const Context& context
  );

  void log(
    const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime = std::nullopt,
    std::optional<uint64_t> publishTime = std::nullopt,
    std::optional<uint32_t> sequence = std::nullopt
  );

  uint64_t id() const;

private:
  Channel(
    const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema,
    const ContextInner* context
  );

  std::unique_ptr<foxglove_channel, void (*)(foxglove_channel*)> _impl;
};

}  // namespace foxglove
