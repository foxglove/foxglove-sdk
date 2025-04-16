#pragma once

#include <foxglove/error.hpp>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;

namespace foxglove {

struct Schema {
  std::string name;
  std::string encoding;
  const std::byte* data = nullptr;
  size_t dataLen = 0;
};

class Channel final {
public:
  static FoxgloveResult<Channel> create(
    const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema
  );

  FoxgloveError log(
    const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime = std::nullopt
  );

  uint64_t id() const;

  Channel(const Channel&) = delete;
  Channel& operator=(const Channel&) = delete;

  Channel(Channel&& other) noexcept = default;
  Channel& operator=(Channel&& other) noexcept = default;

private:
  explicit Channel(const foxglove_channel* channel);

  std::unique_ptr<const foxglove_channel, void (*const)(const foxglove_channel*)> _impl;
};

}  // namespace foxglove
