#include <foxglove/mcap.hpp>

#include <iostream>

#include "../include/tl/expected.hpp"

// NOLINTNEXTLINE(bugprone-exception-escape)
int main() {
  foxglove::McapWriterOptions options = {};
  options.path = "test.mcap";
  options.truncate = true;
  auto writer_result = foxglove::McapWriter::create(options);
  if (!writer_result.has_value()) {
    std::cerr << "Failed to create writer: " << foxglove::strerror(writer_result.error()) << '\n';
    return 1;
  }
  auto writer = std::move(writer_result.value());
  writer.close();
}
