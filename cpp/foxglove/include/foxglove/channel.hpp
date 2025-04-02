#pragma once

#include <foxglove/schemas.hpp>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_raw_channel;
struct foxglove_channel;

namespace foxglove {

struct Schema {
  std::string name;
  std::string encoding;
  const std::byte* data = nullptr;
  size_t dataLen = 0;
};

class RawChannel final {
public:
  RawChannel(
    const std::string& topic, const std::string& messageEncoding, std::optional<Schema> schema
  );

  void log(
    const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime = std::nullopt,
    std::optional<uint64_t> publishTime = std::nullopt,
    std::optional<uint32_t> sequence = std::nullopt
  );

  uint64_t id() const;

private:
  std::unique_ptr<foxglove_raw_channel, void (*)(foxglove_raw_channel*)> _impl;
};

template<class TMsg, class = void>
class Channel final {
public:
  static_assert(false, "Only schemas defined in foxglove::schemas are currently supported");
};

template<class TMsg>
class Channel<TMsg, std::enable_if_t<foxglove::internal::IsBuiltinSchema<TMsg>::value>> final {
public:
  explicit Channel(const std::string& topic);

  void log(
    const TMsg& msg, std::optional<uint64_t> logTime = std::nullopt,
    std::optional<uint64_t> publishTime = std::nullopt,
    std::optional<uint32_t> sequence = std::nullopt
  );

  uint64_t id() const;

private:
  std::unique_ptr<foxglove_channel, void (*)(foxglove_channel*)> _impl;
};

}  // namespace foxglove
