#pragma once

#include <foxglove/context.hpp>
#include <foxglove/error.hpp>

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

class Channel final {
public:
  static FoxgloveResult<Channel> create(
    const std::string& topic, const std::string& message_encoding,
    std::optional<Schema> schema = std::nullopt, const Context& context = Context()
  );

  FoxgloveError log(
    const std::byte* data, size_t data_len, std::optional<uint64_t> log_time = std::nullopt
  );

  [[nodiscard]] uint64_t testId() const;

  [[nodiscard]] uint64_t id() const;

  Channel(const Channel&) = delete;
  Channel& operator=(const Channel&) = delete;

  Channel(Channel&& other) noexcept = default;
  Channel& operator=(Channel&& other) noexcept = default;

  ~Channel() = default;

private:
  explicit Channel(const foxglove_channel* channel);

  struct Deleter {
    void operator()(const foxglove_channel* ptr) const noexcept;
  };

  std::unique_ptr<const foxglove_channel, Deleter> impl_;
};

}  // namespace foxglove
