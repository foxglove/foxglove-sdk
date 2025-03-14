#pragma once

#include <memory>
#include <string>

struct foxglove_mcap_writer;

namespace foxglove {

struct McapWriterOptions {
  std::string_view path;
  bool create;
  bool truncate;
};

class McapWriter final {
public:
  explicit McapWriter(McapWriterOptions options);

  void close();
private:
  std::unique_ptr<foxglove_mcap_writer, void (*)(foxglove_mcap_writer*)> _impl;
};

}  // namespace foxglove
