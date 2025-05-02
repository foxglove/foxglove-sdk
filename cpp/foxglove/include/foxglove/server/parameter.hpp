#pragma once

#include <foxglove/error.hpp>

#include <cstdint>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <variant>
#include <vector>

struct foxglove_parameter_value;
struct foxglove_parameter;
struct foxglove_parameter_array;

namespace foxglove {

/**
 * A parameter type
 */
enum class ParameterType : uint8_t {
  None,
  ByteArray,
  Float64,
  Float64Array,
};

/**
 * A view over an unowned parameter value.
 */
class ParameterValueView final {
public:
  using Number = double;
  using Boolean = bool;
  using String = std::string;
  using Array = std::vector<ParameterValueView>;
  using Dict = std::map<std::string, ParameterValueView>;
  using Value = std::variant<Number, Boolean, String, Array, Dict>;

  // Accessors
  [[nodiscard]] Value value() const;

  // Creates a deep clone of this parameter value.
  [[nodiscard]] class ParameterValue clone() const;

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

  // Accessors
  [[nodiscard]] ParameterValueView view() const noexcept;
  [[nodiscard]] ParameterValueView::Value value() const {
    return this->view().value();
  }

  // Creates a deep clone of this parameter value.
  [[nodiscard]] ParameterValue clone() const {
    return this->view().clone();
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
 */
class ParameterView final {
public:
  // Accessors
  [[nodiscard]] std::string_view name() const noexcept;
  [[nodiscard]] ParameterType type() const noexcept;
  [[nodiscard]] std::optional<ParameterValueView> valueView() const noexcept;
  [[nodiscard]] std::optional<ParameterValueView::Value> value() const {
    if (auto valueView = this->valueView()) {
      return valueView->value();
    }
    return {};
  }

  // Creates a deep clone of this parameter.
  [[nodiscard]] class Parameter clone() const;

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
  explicit Parameter(std::string_view name, const std::vector<double>& values);
  explicit Parameter(std::string_view name, std::map<std::string, ParameterValue> values);
  explicit Parameter(std::string_view name, ParameterType, ParameterValue&& value);

  // Default destructor & move, disable copy.
  ~Parameter() = default;
  Parameter(Parameter&& other) noexcept = default;
  Parameter& operator=(Parameter&& other) noexcept = default;
  Parameter(const Parameter&) = delete;
  Parameter& operator=(const Parameter&) = delete;

  // Accessors
  [[nodiscard]] ParameterView view() const noexcept;
  [[nodiscard]] std::string_view name() const noexcept {
    return this->view().name();
  }
  [[nodiscard]] ParameterType type() const noexcept {
    return this->view().type();
  }
  [[nodiscard]] std::optional<ParameterValueView::Value> value() const {
    return this->view().value();
  }

  // Creates a deep clone of this parameter.
  [[nodiscard]] Parameter clone() const {
    return this->view().clone();
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
