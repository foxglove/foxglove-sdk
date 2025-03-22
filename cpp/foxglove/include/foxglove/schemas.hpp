#pragma once

#include <type_traits>

namespace foxglove::schemas {

/**
 * A vector in 3D space that represents a direction only
 */
struct Vector3 {
  /**
   * x coordinate length
   */
  double x = 0;
  /**
   * y coordinate length
   */
  double y = 0;
  /**
   * z coordinate length
   */
  double z = 0;
};

}  // namespace foxglove::schemas

namespace foxglove::internal {

template<class TMsg>
struct IsBuiltinSchema : std::false_type {};

template<>
struct IsBuiltinSchema<foxglove::schemas::Vector3> : std::true_type {};

}  // namespace foxglove::internal
