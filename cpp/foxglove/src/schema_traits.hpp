#include <foxglove-c/foxglove-c.h>
#include <foxglove/schemas.hpp>

#include <functional>

namespace foxglove::internal {

template<typename Msg>
struct BuiltinSchemaTraits {
  static_assert(false, "Schema traits were not defined for this type");
};

template<>
struct BuiltinSchemaTraits<foxglove::schemas::Vector3> {
  using CType = foxglove_vector3;

  static constexpr foxglove_builtin_schema BuiltinSchema = foxglove_builtin_schema_Vector3;

  static void WithCMessage(
    const foxglove::schemas::Vector3& msg, std::function<void(const foxglove_vector3&)> callback
  ) {
    foxglove_vector3 cMsg = {
      msg.x,
      msg.y,
      msg.z,
    };
    callback(cMsg);
  }
};

}  // namespace foxglove::internal
