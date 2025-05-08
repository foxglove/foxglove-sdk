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
    const std::string_view& topic, const std::string_view& message_encoding,
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

}  // namespace foxglove
