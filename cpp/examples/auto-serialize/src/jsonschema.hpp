#include <nlohmann/json.hpp>

#include <vector>

using json = nlohmann::json;

namespace jsonschema {
template<typename T, typename U>
struct field_type;

template<typename T, typename U>
struct field_type<T U::*, U> {
  using type = T;
};

/**
 * Generate a simple JSON schema for object types, iterating over fields. Nested objects and
 * arrays are not described, and enum values are treated as their JSON representation.
 */
template<typename T>
json generate_schema() {
  T instance{};
  json j = instance;

  // Start building the schema
  json schema = {
    {"$schema", "http://json-schema.org/draft-07/schema#"},
    {"type", "object"},
    {"properties", json::object()},
    {"required", json::array()},
  };

  // For each property in the JSON object
  for (auto& [key, value] : j.items()) {
    json property_schema;

    // Get the type of the property by checking the JSON value type
    if (value.is_string()) {
      property_schema["type"] = "string";
    } else if (value.is_number_integer()) {
      property_schema["type"] = "integer";
    } else if (value.is_number_float()) {
      property_schema["type"] = "number";
    } else if (value.is_boolean()) {
      property_schema["type"] = "boolean";
    } else if (value.is_array()) {
      property_schema["type"] = "array";
    } else if (value.is_object()) {
      property_schema["type"] = "object";
    }

    schema["properties"][key] = property_schema;
    schema["required"].push_back(key);
  }

  return schema;
}
}  // namespace jsonschema
