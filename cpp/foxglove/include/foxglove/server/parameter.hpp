#pragma once

#include <foxglove/error.hpp>

#include <cstdint>
#include <map>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <variant>
#include <vector>

struct foxglove_parameter_value;
struct foxglove_parameter;
struct foxglove_parameter_array;

namespace foxglove {

/**
 * A parameter type.
 *
 * This enum is used to disambiguate `Parameter` values, in situations where the
 * wire representation is ambiguous.
 */
enum class ParameterType : uint8_t {
  // The parameter value can be inferred from the inner parameter value tag.
  None,
  // An array of bytes.
  ByteArray,
  // A decimal or integer value that can be represented as a `float64`.
  Float64,
  // An array of decimal or integer values that can be represented as `float64`s.
  Float64Array,
};

/**
 * A view over an unowned parameter value.
 *
 * This lifetime of this view is tied to the `ParameterValue` from which it was
 * derived. It is the caller's responsibility to ensure the validity of this
 * lifetime when accessing the view.
 */
class ParameterValueView final {
public:
  using Array = std::vector<ParameterValueView>;
  using Dict = std::map<std::string, ParameterValueView>;
  using Value = std::variant<double, bool, std::string, Array, Dict>;

  // Creates a deep clone of this parameter value.
  [[nodiscard]] class ParameterValue clone() const;

  // Returns a variant representation of the value.
  [[nodiscard]] Value value() const;

  // Value type checker.
  template<typename T>
  [[nodiscard]] bool is() const {
    auto value = this->value();
    return std::holds_alternative<T>(value);
  }

  // Value extractor.
  template<typename T>
  [[nodiscard]] T get() const {
    throw std::runtime_error("Unsupported type");
  }
  template<>
  [[nodiscard]] ParameterValueView get<ParameterValueView>() const {
    return *this;
  }
  template<>
  [[nodiscard]] double get<double>() const {
    return std::get<double>(this->value());
  }
  template<>
  [[nodiscard]] bool get<bool>() const {
    return std::get<bool>(this->value());
  }
  template<>
  [[nodiscard]] std::string get<std::string>() const {
    return std::get<std::string>(this->value());
  }
  template<>
  [[nodiscard]] Array get<Array>() const {
    return std::get<Array>(this->value());
  }
  template<>
  [[nodiscard]] Dict get<Dict>() const {
    return std::get<Dict>(this->value());
  }

private:
  friend class ParameterView;
  friend class ParameterValue;

  const foxglove_parameter_value* _impl;
  explicit ParameterValueView(const foxglove_parameter_value* ptr)
      : _impl(ptr) {}
};

/**
 * An owned parameter value.
 */
class ParameterValue final {
public:
  // Constructors
  explicit ParameterValue(double value);
  explicit ParameterValue(bool value);
  explicit ParameterValue(std::string_view value);
  explicit ParameterValue(const char* value)
      : ParameterValue(std::string_view(value)) {}
  explicit ParameterValue(std::vector<ParameterValue> values);
  explicit ParameterValue(std::map<std::string, ParameterValue> values);

  // Default destructor & move, disable copy.
  ~ParameterValue() = default;
  ParameterValue(ParameterValue&& other) noexcept = default;
  ParameterValue& operator=(ParameterValue&& other) noexcept = default;
  ParameterValue(const ParameterValue&) = delete;
  ParameterValue& operator=(const ParameterValue&) = delete;

  // Creates a deep clone of this parameter value.
  [[nodiscard]] ParameterValue clone() const {
    return this->view().clone();
  }

  // Accessors
  [[nodiscard]] ParameterValueView view() const noexcept;
  [[nodiscard]] ParameterValueView::Value value() const {
    return this->view().value();
  }

  // Value type checker.
  template<typename T>
  [[nodiscard]] bool is() const {
    return this->view().is<T>();
  }

  // Value extractor.
  template<typename T>
  [[nodiscard]] T get() const {
    return this->view().get<T>();
  }

private:
  friend class ParameterValueView;
  friend class Parameter;

  struct Deleter {
    void operator()(foxglove_parameter_value* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_parameter_value, Deleter> _impl;

  // Constructor from raw pointer.
  explicit ParameterValue(foxglove_parameter_value*);

  // Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter_value* release() noexcept;
};

/**
 * A view over an unowned parameter.
 *
 * This lifetime of this view is tied to the `Parameter` from which it was
 * derived. It is the caller's responsibility to ensure the validity of this
 * lifetime when accessing the view.
 */
class ParameterView final {
public:
  // Creates a deep clone of this parameter.
  [[nodiscard]] class Parameter clone() const;

  // Accessors
  [[nodiscard]] std::string_view name() const noexcept;
  [[nodiscard]] ParameterType type() const noexcept;
  [[nodiscard]] std::optional<ParameterValueView> value() const noexcept;
  [[nodiscard]] bool hasValue() const noexcept {
    return this->value().has_value();
  };

  // Value type checkers.
  template<typename T>
  [[nodiscard]] bool is() const {
    return this->hasValue() && this->value()->is<T>();
  }
  template<>
  [[nodiscard]] bool is<std::string>() const {
    return this->hasValue() && this->type() != ParameterType::ByteArray &&
           this->value()->is<std::string>();
  }
  [[nodiscard]] bool isArray() const noexcept {
    return this->hasValue() && this->value()->is<ParameterValueView::Array>();
  }
  [[nodiscard]] bool isDict() const noexcept {
    return this->hasValue() && this->value()->is<ParameterValueView::Dict>();
  }
  [[nodiscard]] bool isFloat64Array() const noexcept {
    return this->hasValue() && this->type() == ParameterType::Float64Array && this->isArray();
  }
  [[nodiscard]] bool isByteArray() const noexcept {
    return this->hasValue() && this->type() == ParameterType::ByteArray &&
           this->value()->is<std::string>();
  }

  // Value extractor.
  template<typename T>
  [[nodiscard]] T get() const {
    auto value = this->value();
    if (!value) {
      throw std::bad_optional_access();
    }
    return value->template get<T>();
  }

  // Value array extractor.
  template<typename T>
  [[nodiscard]] std::vector<T> getArray() const {
    auto value = this->value();
    if (!value) {
      throw std::bad_optional_access();
    }
    const auto& arr = value->get<ParameterValueView::Array>();
    std::vector<T> result;
    result.reserve(arr.size());
    for (const auto& elem : arr) {
      result.push_back(elem.get<T>());
    }
    return result;
  }

  // Value dict extractor.
  template<typename T>
  [[nodiscard]] std::map<std::string, T> getDict() const {
    auto value = this->value();
    if (!value) {
      throw std::bad_optional_access();
    }
    const auto& dict = value->get<ParameterValueView::Dict>();
    std::map<std::string, T> result;
    for (const auto& elem : dict) {
      std::string key(elem.first);
      auto value = elem.second.get<T>();
      result.emplace(key, value);
    }
    return result;
  }

  // Value byte array extractor.
  [[nodiscard]] FoxgloveResult<std::vector<std::byte>> getByteArray() const;

private:
  friend class Parameter;
  friend class ParameterArrayView;

  const foxglove_parameter* _impl;
  explicit ParameterView(const foxglove_parameter* ptr)
      : _impl(ptr) {}
};

/**
 * An owned parameter.
 */
class Parameter final {
public:
  explicit Parameter(std::string_view);
  explicit Parameter(std::string_view name, double value);
  explicit Parameter(std::string_view name, bool value);
  explicit Parameter(std::string_view name, std::string_view value);
  explicit Parameter(std::string_view name, const char* value)
      : Parameter(name, std::string_view(value)) {}
  explicit Parameter(std::string_view name, const uint8_t* data, size_t data_length);
  explicit Parameter(std::string_view name, const std::vector<std::byte>& bytes)
      : Parameter(name, reinterpret_cast<const uint8_t*>(bytes.data()), bytes.size()) {}
  explicit Parameter(std::string_view name, const std::vector<double>& values);
  explicit Parameter(std::string_view name, std::map<std::string, ParameterValue> values);
  explicit Parameter(std::string_view name, ParameterType, ParameterValue&& value);

  // Default destructor & move, disable copy.
  ~Parameter() = default;
  Parameter(Parameter&& other) noexcept = default;
  Parameter& operator=(Parameter&& other) noexcept = default;
  Parameter(const Parameter&) = delete;
  Parameter& operator=(const Parameter&) = delete;

  // Creates a deep clone of this parameter.
  [[nodiscard]] Parameter clone() const {
    return this->view().clone();
  }

  // Accessors
  [[nodiscard]] ParameterView view() const noexcept;
  [[nodiscard]] std::string_view name() const noexcept {
    return this->view().name();
  }
  [[nodiscard]] ParameterType type() const noexcept {
    return this->view().type();
  }
  [[nodiscard]] std::optional<ParameterValueView> value() const noexcept {
    return this->view().value();
  }
  [[nodiscard]] bool hasValue() const noexcept {
    return this->view().hasValue();
  };

  // Value type checkers.
  template<typename T>
  [[nodiscard]] bool is() const {
    return this->view().is<T>();
  }
  [[nodiscard]] bool isArray() const noexcept {
    return this->view().isArray();
  }
  [[nodiscard]] bool isDict() const noexcept {
    return this->view().isDict();
  }
  [[nodiscard]] bool isFloat64Array() const noexcept {
    return this->view().isFloat64Array();
  }
  [[nodiscard]] bool isByteArray() const noexcept {
    return this->view().isByteArray();
  }

  // Value extractor.
  template<typename T>
  [[nodiscard]] T get() const {
    return this->view().get<T>();
  }

  // Value array extractor.
  template<typename T>
  [[nodiscard]] std::vector<T> getArray() const {
    return this->view().getArray<T>();
  }

  // Value dict extractor.
  template<typename T>
  [[nodiscard]] std::map<std::string, T> getDict() const {
    return this->view().getDict<T>();
  }

  // Value byte array extractor.
  [[nodiscard]] FoxgloveResult<std::vector<std::byte>> getByteArray() const {
    return this->view().getByteArray();
  }

private:
  friend class ParameterView;
  friend class ParameterArray;

  struct Deleter {
    void operator()(foxglove_parameter* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_parameter, Deleter> _impl;

  explicit Parameter(foxglove_parameter* param);

  // Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter* release() noexcept;
};

/**
 * A view over an unowned parameter array.
 *
 * This lifetime of this view is tied to the `ParameterArray` from which it was
 * derived. It is the caller's responsibility to ensure the validity of this
 * lifetime when accessing the view.
 */
class ParameterArrayView final {
public:
  explicit ParameterArrayView(const foxglove_parameter_array*);

  [[nodiscard]] std::vector<ParameterView> parameters() const;

private:
  const foxglove_parameter_array* _impl;
};

/**
 * An owned parameter array.
 */
class ParameterArray final {
public:
  explicit ParameterArray(std::vector<Parameter>&&);

  // Default destructor & move, disable copy.
  ~ParameterArray() = default;
  ParameterArray(ParameterArray&& other) noexcept = default;
  ParameterArray& operator=(ParameterArray&& other) noexcept = default;
  ParameterArray(const ParameterArray&) = delete;
  ParameterArray& operator=(const ParameterArray&) = delete;

  // Accessors
  [[nodiscard]] ParameterArrayView view() const noexcept;
  [[nodiscard]] std::vector<ParameterView> parameters() const {
    return this->view().parameters();
  }

  // Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter_array* release() noexcept;

private:
  struct Deleter {
    void operator()(foxglove_parameter_array* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_parameter_array, Deleter> _impl;
};

}  // namespace foxglove
