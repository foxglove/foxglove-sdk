#pragma once

#include <cstdint>

namespace foxglove {

struct Timestamp {
  uint32_t seconds;
  uint32_t nanos;
};

struct Duration {
  int32_t seconds;
  uint32_t nanos;
};

}  // namespace foxglove
