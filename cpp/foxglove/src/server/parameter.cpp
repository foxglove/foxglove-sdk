#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/server/parameter.hpp>

#include <cstring>
#include <stdexcept>

namespace foxglove {

/**
 * ParameterValueView implementation
 */
ParameterValueView::Value ParameterValueView::value() const {
  // Accessing union members is safe, because the tag serves as a valid
  // discriminator.
  switch (_impl->tag) {
    case FOXGLOVE_PARAMETER_VALUE_TAG_NUMBER:
      return _impl->data.number;  // NOLINT(cppcoreguidelines-pro-type-union-access)
    case FOXGLOVE_PARAMETER_VALUE_TAG_BOOLEAN:
      return _impl->data.boolean;  // NOLINT(cppcoreguidelines-pro-type-union-access)
    case FOXGLOVE_PARAMETER_VALUE_TAG_STRING: {
      // NOLINTNEXTLINE(cppcoreguidelines-pro-type-union-access)
      const foxglove_string* string = &_impl->data.string;
      return std::string(string->data, string->len);
    }
    case FOXGLOVE_PARAMETER_VALUE_TAG_ARRAY: {
      // NOLINTNEXTLINE(cppcoreguidelines-pro-type-union-access)
      const foxglove_parameter_value_array* array = &_impl->data.array;
      std::vector<ParameterValueView> result;
      result.reserve(array->len);
      for (size_t i = 0; i < array->len; ++i) {
        auto value = ParameterValueView(&array->values[i]);
        result.emplace_back(value);
      }
      return result;
    }
    case FOXGLOVE_PARAMETER_VALUE_TAG_DICT: {
      // NOLINTNEXTLINE(cppcoreguidelines-pro-type-union-access)
      const foxglove_parameter_value_dict* dict = &_impl->data.dict;
      std::map<std::string, ParameterValueView> result;
      for (size_t i = 0; i < dict->len; ++i) {
        const auto& entry = dict->entries[i];
        auto key = std::string(entry.key.data, entry.key.len);
        result.emplace(key, ParameterValueView(entry.value));
      }
      return result;
    }
    default:
      throw std::runtime_error("Unknown parameter value tag");
  }
}

ParameterValue ParameterValueView::clone() const {
  foxglove_parameter_value* ptr = nullptr;
  auto error = foxglove_parameter_value_clone(&ptr, _impl);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  return ParameterValue(ptr);
}

/**
 * ParameterValue implementation
 */
void ParameterValue::Deleter::operator()(foxglove_parameter_value* ptr) const noexcept {
  foxglove_parameter_value_free(ptr);
}

ParameterValue::ParameterValue(foxglove_parameter_value* value)
    : _impl(value) {}

ParameterValue::ParameterValue(double value)
    : _impl(nullptr) {
  foxglove_parameter_value* ptr = nullptr;
  auto error = foxglove_parameter_value_create_number(&ptr, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

ParameterValue::ParameterValue(bool value)
    : _impl(nullptr) {
  foxglove_parameter_value* ptr = nullptr;
  auto error = foxglove_parameter_value_create_boolean(&ptr, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

ParameterValue::ParameterValue(std::string_view value)
    : _impl(nullptr) {
  foxglove_parameter_value* ptr = nullptr;
  auto error = foxglove_parameter_value_create_string(&ptr, {value.data(), value.length()});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

ParameterValue::ParameterValue(std::vector<ParameterValue> values)
    : _impl(nullptr) {
  foxglove_parameter_value_array* array_ptr = nullptr;
  auto error = foxglove_parameter_value_array_create(&array_ptr, values.size());
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }

  for (auto& value : values) {
    foxglove_parameter_value* value_ptr = value.release();
    error = foxglove_parameter_value_array_push(array_ptr, value_ptr);
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_value_array_free(array_ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  foxglove_parameter_value* ptr = nullptr;
  error = foxglove_parameter_value_create_array(&ptr, array_ptr);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }

  _impl.reset(ptr);
}

ParameterValue::ParameterValue(std::map<std::string, ParameterValue> value)
    : _impl(nullptr) {
  foxglove_parameter_value_dict* dict_ptr = nullptr;
  auto error = foxglove_parameter_value_dict_create(&dict_ptr, value.size());
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }

  for (auto& pair : value) {
    std::string_view key = pair.first;
    foxglove_parameter_value* value_ptr = pair.second.release();
    error = foxglove_parameter_value_dict_insert(dict_ptr, {key.data(), key.length()}, value_ptr);
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_value_dict_free(dict_ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  foxglove_parameter_value* ptr = nullptr;
  error = foxglove_parameter_value_create_dict(&ptr, dict_ptr);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }

  _impl.reset(ptr);
}

ParameterValueView ParameterValue::view() const noexcept {
  return ParameterValueView(_impl.get());
}

foxglove_parameter_value* ParameterValue::release() noexcept {
  return _impl.release();
}

/**
 * ParameterView implementation
 */
std::string_view ParameterView::name() const noexcept {
  const auto& name = _impl->name;
  return {name.data, name.len};
}

ParameterType ParameterView::type() const noexcept {
  return static_cast<ParameterType>(_impl->type);
}

std::optional<ParameterValueView> ParameterView::valueView() const noexcept {
  if (_impl->value == nullptr) {
    return {};
  }
  return ParameterValueView(_impl->value);
}

Parameter ParameterView::clone() const {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_clone(&ptr, _impl);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  return Parameter(ptr);
}

/**
 * Parameter implementation
 */
void Parameter::Deleter::operator()(foxglove_parameter* ptr) const noexcept {
  foxglove_parameter_free(ptr);
}

Parameter::Parameter(foxglove_parameter* param)
    : _impl(param) {}

Parameter::Parameter(std::string_view name)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_empty(&ptr, {name.data(), name.length()});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, bool value)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_boolean(&ptr, {name.data(), name.length()}, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, double value)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_float64(&ptr, {name.data(), name.length()}, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, std::string_view value)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_string(
    &ptr, {name.data(), name.length()}, {value.data(), value.length()}
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, const uint8_t* data, size_t data_length)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error =
    foxglove_parameter_create_byte_array(&ptr, {name.data(), name.length()}, {data, data_length});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, const std::vector<double>& values)
    : _impl(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_float64_array(
    &ptr, {name.data(), name.length()}, values.data(), values.size()
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

Parameter::Parameter(std::string_view name, std::map<std::string, ParameterValue> values)
    : Parameter::Parameter(name, ParameterType::None, ParameterValue(std::move(values))) {}

Parameter::Parameter(std::string_view name, ParameterType type, ParameterValue&& value)
    : _impl(nullptr) {
  // Explicit move to make the linter happy.
  foxglove_parameter_value* value_ptr = std::move(value).release();
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create(
    &ptr, {name.data(), name.length()}, static_cast<foxglove_parameter_type>(type), value_ptr
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  _impl.reset(ptr);
}

ParameterView Parameter::view() const noexcept {
  return ParameterView(_impl.get());
}

foxglove_parameter* Parameter::release() noexcept {
  return _impl.release();
}

/**
 * ParameterArrayView implementation.
 */
ParameterArrayView::ParameterArrayView(const foxglove_parameter_array* ptr)
    : _impl(ptr) {}

std::vector<ParameterView> ParameterArrayView::parameters() const {
  std::vector<ParameterView> params;
  params.reserve(_impl->len);
  for (auto i = 0; i < _impl->len; ++i) {
    params.emplace_back(ParameterView(&_impl->parameters[i]));
  }
  return params;
}

/**
 * ParameterArray implementation.
 */
void ParameterArray::Deleter::operator()(foxglove_parameter_array* ptr) const noexcept {
  foxglove_parameter_array_free(ptr);
}

// We're consuming the contents of the vector, even though we're not moving it.
// NOLINTNEXTLINE(cppcoreguidelines-rvalue-reference-param-not-moved)
ParameterArray::ParameterArray(std::vector<Parameter>&& params)
    : _impl(nullptr) {
  foxglove_parameter_array* ptr = nullptr;
  auto error = foxglove_parameter_array_create(&ptr, params.size());
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }

  for (auto& param : params) {
    error = foxglove_parameter_array_push(ptr, param.release());
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_array_free(ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  _impl.reset(ptr);
}

ParameterArrayView ParameterArray::view() const noexcept {
  return ParameterArrayView(_impl.get());
}

foxglove_parameter_array* ParameterArray::release() noexcept {
  return _impl.release();
}

}  // namespace foxglove
