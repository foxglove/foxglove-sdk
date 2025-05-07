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
  size_t data_len = 0;
};

class RawChannel final {
public:
  static FoxgloveResult<RawChannel> create(
    const std::string& topic, const std::string& message_encoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context()
  );

  FoxgloveError log(
    const std::byte* data, size_t data_len, std::optional<uint64_t> log_time = std::nullopt
  );

  [[nodiscard]] uint64_t testId() const;

  [[nodiscard]] uint64_t id() const;

  RawChannel(const RawChannel&) = delete;
  RawChannel& operator=(const RawChannel&) = delete;

  RawChannel(RawChannel&& other) noexcept = default;
  ~RawChannel() = default;

private:
  explicit RawChannel(const foxglove_channel* channel);

  schemas::ChannelUniquePtr impl_;
};

// template<class T, class = void>
// class Channel final {
// public:
//   static_assert(false, "Only schemas defined in foxglove::schemas are currently supported");
// };

// TODO can we restrict this? std::enable_if_t<internal::BuiltinSchema<T>::value>
template<class T>
class Channel final {
public:
  static FoxgloveResult<Channel<T>> create(
    const std::string& topic, const Context& context = Context()
  ) {
    auto result = internal::BuiltinSchema<T>::create(topic, context);
    if (result.has_value()) {
      return Channel(std::move(result.value()));
    }
    return foxglove::unexpected(std::move(result.error()));
  }

  FoxgloveError log(const T& value, std::optional<uint64_t> log_time = std::nullopt) {
    return internal::BuiltinSchema<T>::logTo(impl_.get(), value, log_time);
  }

  Channel(Channel&& other) noexcept = default;
  Channel& operator=(Channel&& other) noexcept = default;
  ~Channel() = default;

private:
  explicit Channel(schemas::ChannelUniquePtr&& channel)
      : impl_(std::move(channel)) {}

  schemas::ChannelUniquePtr impl_;
};

}  // namespace foxglove
