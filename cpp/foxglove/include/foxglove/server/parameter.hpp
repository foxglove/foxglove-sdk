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
  friend class ParameterView;
  friend class ParameterValue;

public:
  using Number = double;
  using Boolean = bool;
  using String = std::string;
  using Array = std::vector<ParameterValueView>;
  using Dict = std::map<std::string, ParameterValueView>;
  using Value = std::variant<Number, Boolean, String, Array, Dict>;

  [[nodiscard]] class ParameterValue clone() const;
  [[nodiscard]] Value value() const;

private:
  const foxglove_parameter_value* _impl;
  explicit ParameterValueView(const foxglove_parameter_value* ptr)
      : _impl(ptr) {}
};

/**
 * An owned parameter value.
 */
class ParameterValue final {
  friend class ParameterValueView;
  friend class Parameter;

public:
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

  [[nodiscard]] ParameterValueView view() const;
  [[nodiscard]] ParameterValue clone() const {
    return this->view().clone();
  }
  [[nodiscard]] ParameterValueView::Value getValue() const {
    return this->view().value();
  };

private:
  std::unique_ptr<foxglove_parameter_value, void (*)(foxglove_parameter_value*)> _impl;

  explicit ParameterValue(foxglove_parameter_value*);

  foxglove_parameter_value* release();
};

/**
 * A view over an unowned paramter.
 */
class ParameterView final {
  friend class Parameter;
  friend class ParameterArrayView;

public:
  [[nodiscard]] class Parameter clone() const;
  [[nodiscard]] std::string name() const;
  [[nodiscard]] ParameterType type() const;
  [[nodiscard]] std::optional<ParameterValueView> valueView() const;
  [[nodiscard]] std::optional<ParameterValueView::Value> value() const {
    if (auto valueView = this->valueView()) {
      return valueView->value();
    }
    return {};
  }

private:
  const foxglove_parameter* _impl;
  explicit ParameterView(const foxglove_parameter* ptr)
      : _impl(ptr) {}
};

/**
 * An owned parameter.
 */
class Parameter final {
  friend class ParameterView;
  friend class ParameterArray;

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

  [[nodiscard]] ParameterView view() const;
  [[nodiscard]] Parameter clone() const {
    return this->view().clone();
  };
  [[nodiscard]] std::string name() const {
    return this->view().name();
  };
  [[nodiscard]] ParameterType type() const {
    return this->view().type();
  };
  [[nodiscard]] std::optional<ParameterValueView::Value> value() const {
    return this->view().value();
  }

private:
  std::unique_ptr<foxglove_parameter, void (*)(foxglove_parameter*)> _impl;

  explicit Parameter(foxglove_parameter* param);

  foxglove_parameter* release();
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
  explicit ParameterArray(std::vector<Parameter>);

  // Default destructor & move, disable copy.
  ~ParameterArray() = default;
  ParameterArray(ParameterArray&& other) noexcept = default;
  ParameterArray& operator=(ParameterArray&& other) noexcept = default;
  ParameterArray(const ParameterArray&) = delete;
  ParameterArray& operator=(const ParameterArray&) = delete;

  [[nodiscard]] ParameterArrayView view() const;
  [[nodiscard]] std::vector<ParameterView> parameters() const {
    return this->view().parameters();
  }

  foxglove_parameter_array* release();

private:
  std::unique_ptr<foxglove_parameter_array, void (*)(foxglove_parameter_array*)> _impl;
};

}  // namespace foxglove
