#pragma once

#include <cstdint>

namespace foxglove {

struct Timestamp {
  uint32_t sec;
  uint32_t nsec;
};

struct Duration {
  int32_t sec;
  uint32_t nsec;
};

}  // namespace foxglove
