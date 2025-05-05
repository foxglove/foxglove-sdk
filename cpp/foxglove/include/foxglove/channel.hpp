#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>
#include <foxglove/schemas.hpp>

#include <cstdint>
#include <memory>
#include <optional>
#include <string>

struct foxglove_channel;
struct foxglove_context;

namespace foxglove {

struct Schema {
  std::string name;
  std::string encoding;
  const std::byte* data = nullptr;
  size_t dataLen = 0;
};

class RawChannel final {
public:
  static FoxgloveResult<RawChannel> create(
    const std::string& topic, const std::string& messageEncoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context()
  );

  FoxgloveError log(
    const std::byte* data, size_t dataLen, std::optional<uint64_t> logTime = std::nullopt
  );

  uint64_t id() const;

  RawChannel(const RawChannel&) = delete;
  RawChannel& operator=(const RawChannel&) = delete;

  RawChannel(RawChannel&& other) noexcept = default;

private:
  explicit RawChannel(const foxglove_channel* channel);

  std::unique_ptr<const foxglove_channel, void (*const)(const foxglove_channel*)> _impl;
};

template<class T, class = void>
class Channel final {
public:
  static_assert(false, "Only schemas defined in foxglove::schemas are currently supported");
};

template<class T>
class Channel<T, std::enable_if_t<internal::BuiltinSchema<T>::value>> final {
public:
  Channel(const RawChannel& rawChannel);

  FoxgloveError log(const T& value, std::optional<uint64_t> logTime = std::nullopt) {
    return internal::BuiltinSchema<T>::logTo(_impl.get(), value, logTime);
  }

private:
  explicit Channel(const foxglove_channel* channel);

  std::unique_ptr<const foxglove_channel, void (*const)(const foxglove_channel*)> _impl;
};

}  // namespace foxglove
