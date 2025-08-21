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
  switch (impl_->tag) {
    case FOXGLOVE_PARAMETER_VALUE_TAG_FLOAT64:
      return impl_->data.float64;  // NOLINT(cppcoreguidelines-pro-type-union-access)
    case FOXGLOVE_PARAMETER_VALUE_TAG_INTEGER:
      return impl_->data.integer;  // NOLINT(cppcoreguidelines-pro-type-union-access)
    case FOXGLOVE_PARAMETER_VALUE_TAG_BOOLEAN:
      return impl_->data.boolean;  // NOLINT(cppcoreguidelines-pro-type-union-access)
    case FOXGLOVE_PARAMETER_VALUE_TAG_STRING: {
      // NOLINTNEXTLINE(cppcoreguidelines-pro-type-union-access)
      const foxglove_string* string = &impl_->data.string;
      return std::string_view(string->data, string->len);
    }
    case FOXGLOVE_PARAMETER_VALUE_TAG_ARRAY: {
      // NOLINTNEXTLINE(cppcoreguidelines-pro-type-union-access)
      const foxglove_parameter_value_array* array = &impl_->data.array;
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
      const foxglove_parameter_value_dict* dict = &impl_->data.dict;
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
  foxglove_parameter_value* ptr = foxglove_parameter_value_clone(impl_);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }
  return ParameterValue(ptr);
}

/**
 * ParameterValue implementation
 */
void ParameterValue::Deleter::operator()(foxglove_parameter_value* ptr) const noexcept {
  foxglove_parameter_value_free(ptr);
}

ParameterValue::ParameterValue(foxglove_parameter_value* ptr)
    : impl_(ptr) {}

ParameterValue::ParameterValue(double value)
    : impl_(nullptr) {
  foxglove_parameter_value* ptr = foxglove_parameter_value_create_float64(value);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }
  impl_.reset(ptr);
}

ParameterValue::ParameterValue(int64_t value)
    : impl_(nullptr) {
  foxglove_parameter_value* ptr = foxglove_parameter_value_create_integer(value);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }
  impl_.reset(ptr);
}

ParameterValue::ParameterValue(bool value)
    : impl_(nullptr) {
  foxglove_parameter_value* ptr = foxglove_parameter_value_create_boolean(value);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }
  impl_.reset(ptr);
}

ParameterValue::ParameterValue(std::string_view value)
    : impl_(nullptr) {
  foxglove_parameter_value* ptr = nullptr;
  auto error = foxglove_parameter_value_create_string(&ptr, {value.data(), value.length()});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

ParameterValue::ParameterValue(std::vector<ParameterValue> values)
    : impl_(nullptr) {
  foxglove_parameter_value_array* array_ptr = foxglove_parameter_value_array_create(values.size());
  if (array_ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }

  for (auto& value : values) {
    foxglove_parameter_value* value_ptr = value.release();
    auto error = foxglove_parameter_value_array_push(array_ptr, value_ptr);
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_value_array_free(array_ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  foxglove_parameter_value* ptr = foxglove_parameter_value_create_array(array_ptr);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }

  impl_.reset(ptr);
}

ParameterValue::ParameterValue(std::map<std::string, ParameterValue> value)
    : impl_(nullptr) {
  foxglove_parameter_value_dict* dict_ptr = foxglove_parameter_value_dict_create(value.size());
  if (dict_ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }

  for (auto& pair : value) {
    std::string_view key = pair.first;
    foxglove_parameter_value* value_ptr = pair.second.release();
    auto error =
      foxglove_parameter_value_dict_insert(dict_ptr, {key.data(), key.length()}, value_ptr);
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_value_dict_free(dict_ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  foxglove_parameter_value* ptr = foxglove_parameter_value_create_dict(dict_ptr);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }

  impl_.reset(ptr);
}

ParameterValueView ParameterValue::view() const noexcept {
  return ParameterValueView(impl_.get());
}

foxglove_parameter_value* ParameterValue::release() noexcept {
  return impl_.release();
}

/**
 * ParameterView implementation
 */
std::string_view ParameterView::name() const noexcept {
  const auto& name = impl_->name;
  return {name.data, name.len};
}

ParameterType ParameterView::type() const noexcept {
  return static_cast<ParameterType>(impl_->type);
}

std::optional<ParameterValueView> ParameterView::value() const noexcept {
  if (impl_->value == nullptr) {
    return {};
  }
  return ParameterValueView(impl_->value);
}

Parameter ParameterView::clone() const {
  foxglove_parameter* ptr = foxglove_parameter_clone(impl_);
  if (ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }
  return Parameter(ptr);
}

FoxgloveResult<std::vector<std::byte>> ParameterView::getByteArray() const {
  size_t len = 0;
  auto error = foxglove_parameter_get_byte_array_decoded_size(impl_, &len);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    return tl::unexpected(FoxgloveError(error));
  }
  std::vector<std::byte> bytes;
  bytes.resize(len);
  error =
    foxglove_parameter_decode_byte_array(impl_, reinterpret_cast<uint8_t*>(bytes.data()), &len);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    return tl::unexpected(FoxgloveError(error));
  }
  bytes.resize(len);
  return bytes;
}

/**
 * Parameter implementation
 */
void Parameter::Deleter::operator()(foxglove_parameter* ptr) const noexcept {
  foxglove_parameter_free(ptr);
}

Parameter::Parameter(foxglove_parameter* param)
    : impl_(param) {}

Parameter::Parameter(std::string_view name)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_empty(&ptr, {name.data(), name.length()});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, bool value)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_boolean(&ptr, {name.data(), name.length()}, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, double value)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_float64(&ptr, {name.data(), name.length()}, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, int64_t value)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_integer(&ptr, {name.data(), name.length()}, value);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, std::string_view value)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_string(
    &ptr, {name.data(), name.length()}, {value.data(), value.length()}
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, const uint8_t* data, size_t data_length)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error =
    foxglove_parameter_create_byte_array(&ptr, {name.data(), name.length()}, {data, data_length});
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, const std::vector<double>& values)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_float64_array(
    &ptr, {name.data(), name.length()}, values.data(), values.size()
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, const std::vector<int64_t>& values)
    : impl_(nullptr) {
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create_integer_array(
    &ptr, {name.data(), name.length()}, values.data(), values.size()
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

Parameter::Parameter(std::string_view name, std::map<std::string, ParameterValue> values)
    : Parameter::Parameter(name, ParameterType::None, ParameterValue(std::move(values))) {}

Parameter::Parameter(std::string_view name, ParameterType type, ParameterValue&& value)
    : impl_(nullptr) {
  // Explicit move to make the linter happy.
  foxglove_parameter_value* value_ptr = std::move(value).release();
  foxglove_parameter* ptr = nullptr;
  auto error = foxglove_parameter_create(
    &ptr, {name.data(), name.length()}, static_cast<foxglove_parameter_type>(type), value_ptr
  );
  if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
    throw std::runtime_error(foxglove_error_to_cstr(error));
  }
  impl_.reset(ptr);
}

ParameterView Parameter::view() const noexcept {
  return ParameterView(impl_.get());
}

foxglove_parameter* Parameter::release() noexcept {
  return impl_.release();
}

/**
 * ParameterArrayView implementation.
 */
ParameterArrayView::ParameterArrayView(const foxglove_parameter_array* ptr)
    : impl_(ptr) {}

std::vector<ParameterView> ParameterArrayView::parameters() const {
  std::vector<ParameterView> params;
  params.reserve(impl_->len);
  for (size_t i = 0; i < impl_->len; ++i) {
    params.emplace_back(ParameterView(&impl_->parameters[i]));
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
    : impl_(nullptr) {
  foxglove_parameter_array* array_ptr = foxglove_parameter_array_create(params.size());
  if (array_ptr == nullptr) {
    throw std::runtime_error("allocation failed");
  }

  for (auto& param : params) {
    auto error = foxglove_parameter_array_push(array_ptr, param.release());
    if (error != foxglove_error::FOXGLOVE_ERROR_OK) {
      foxglove_parameter_array_free(array_ptr);
      throw std::runtime_error(foxglove_error_to_cstr(error));
    }
  }

  impl_.reset(array_ptr);
}

ParameterArrayView ParameterArray::view() const noexcept {
  return ParameterArrayView(impl_.get());
}

foxglove_parameter_array* ParameterArray::release() noexcept {
  return impl_.release();
}

}  // namespace foxglove
