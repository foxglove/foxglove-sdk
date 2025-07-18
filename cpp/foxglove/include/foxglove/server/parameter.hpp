#pragma once

#include <foxglove/error.hpp>

#include <algorithm>
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

/// @brief A parameter type.
///
/// This enum is used to disambiguate `Parameter` values, in situations where the
/// wire representation is ambiguous.
enum class ParameterType : uint8_t {
  /// The parameter value can be inferred from the inner parameter value tag.
  None,
  /// An array of bytes.
  ByteArray,
  /// A floating-point value that can be represented as a `float64`.
  Float64,
  /// An array of floating-point values that can be represented as `float64`s.
  Float64Array,
};

/// @brief A view over an unowned parameter value.
///
/// This lifetime of this view is tied to the `ParameterValue` from which it was
/// derived. It is the caller's responsibility to ensure the validity of this
/// lifetime when accessing the view.
class ParameterValueView final {
public:
  /// @brief An array of values.
  using Array = std::vector<ParameterValueView>;
  /// @brief A dictionary of values, mapped by string.
  using Dict = std::map<std::string, ParameterValueView>;
  /// @brief A sum type representing the possible values.
  using Value = std::variant<double, bool, int64_t, std::string_view, Array, Dict>;

  /// @brief Creates a deep clone of this parameter value.
  [[nodiscard]] class ParameterValue clone() const;

  /// @brief Returns a variant representation of the value.
  [[nodiscard]] Value value() const;

  /// @brief Checks whether the parameter value matches the specified type.
  ///
  /// @tparam T The expected type of the parameter value.
  template<typename T>
  [[nodiscard]] bool is() const noexcept {
    try {
      auto value = this->value();
      return std::holds_alternative<T>(value);
    } catch (...) {
      // Unknown value tag.
      return false;
    }
  }

  /// @brief Extracts a value from the parameter.
  ///
  /// This method will throw an exception if the type does not match. You can
  /// use `ParameterValueView::is<T>()` to check that the type matches before
  /// calling this method.
  ///
  /// @tparam T The type of the value to extract.
  template<typename T>
  [[nodiscard]] T get() const {
    return std::get<T>(this->value());
  }

private:
  friend class ParameterView;
  friend class ParameterValue;

  const foxglove_parameter_value* impl_;

  /// @brief Constructor from raw pointer.
  explicit ParameterValueView(const foxglove_parameter_value* ptr)
      : impl_(ptr) {}
};

// Template specializations for ParameterValueView
//
// Documented manually as tparams because specializations are merged:
// https://github.com/doxygen/doxygen/issues/9468

/// @fn ParameterValueView::is<std::string>()
/// @tparam std::string Checks whether the parameter value is a string.

/// @fn ParameterValueView::get<ParameterValueView>()
/// @tparam ParameterValueView Extracts the parameter as itself.

/// @fn ParameterValueView::get<std::string>()
/// @tparam std::string Extracts the parameter as an owned string.

/// @cond ignore-template-specializations
/// @brief Checks whether the parameter value is a string.
template<>
[[nodiscard]] inline bool ParameterValueView::is<std::string>() const noexcept {
  return this->is<std::string_view>();
}
/// @brief Extracts the parameter as itself.
template<>
[[nodiscard]] inline ParameterValueView ParameterValueView::get<ParameterValueView>() const {
  return *this;
}
/// @brief Extracts the parameter as an owned string.
template<>
[[nodiscard]] inline std::string ParameterValueView::get<std::string>() const {
  auto sv = this->get<std::string_view>();
  return std::string(sv);
}
/// @endcond

/// @brief An owned parameter value.
class ParameterValue final {
public:
  /// @brief Constructor for a floating point value.
  explicit ParameterValue(double value);
  /// @brief Constructor for an integer value.
  explicit ParameterValue(int64_t value);
  /// @brief Constructor for a boolean value.
  explicit ParameterValue(bool value);
  /// @brief Constructor for a string value.
  explicit ParameterValue(std::string_view value);
  /// @brief Constructor for a string value from a C-style string.
  explicit ParameterValue(const char* value)
      : ParameterValue(std::string_view(value)) {}
  /// @brief Constructor for an array value.
  explicit ParameterValue(std::vector<ParameterValue> values);
  /// @brief Constructor for an dict value.
  explicit ParameterValue(std::map<std::string, ParameterValue> values);

  ~ParameterValue() = default;
  /// @brief Default move constructor.
  ParameterValue(ParameterValue&& other) noexcept = default;
  /// @brief Default move assignment.
  ParameterValue& operator=(ParameterValue&& other) noexcept = default;
  ParameterValue(const ParameterValue&) = delete;
  ParameterValue& operator=(const ParameterValue&) = delete;

  /// @brief Creates a deep clone of this parameter value.
  [[nodiscard]] ParameterValue clone() const {
    return this->view().clone();
  }

  /// @brief Returns an immutable view of this parameter value.
  [[nodiscard]] ParameterValueView view() const noexcept;

  /// @brief Checks whether the parameter value matches the specified type.
  ///
  /// @tparam T The expected type of the parameter value.
  template<typename T>
  [[nodiscard]] bool is() const {
    return this->view().is<T>();
  }

  /// @brief Extracts a value from the parameter.
  ///
  /// This method will throw an exception if the type does not match. You can
  /// use `ParameterValue::is<T>()` to check that the type matches before
  /// calling this method.
  ///
  /// @tparam T The type of the value to extract.
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

  std::unique_ptr<foxglove_parameter_value, Deleter> impl_;

  /// @brief Constructor from raw pointer.
  explicit ParameterValue(foxglove_parameter_value* ptr);

  /// @brief Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter_value* release() noexcept;
};

/// @brief A view over an unowned parameter.
///
/// This lifetime of this view is tied to the `Parameter` from which it was
/// derived. It is the caller's responsibility to ensure the validity of this
/// lifetime when accessing the view.
class ParameterView final {
public:
  /// @brief Creates a deep clone of this parameter.
  [[nodiscard]] class Parameter clone() const;

  /// @brief Returns the parameter name.
  [[nodiscard]] std::string_view name() const noexcept;
  /// @brief Returns the parameter type, used for disambiguation.
  [[nodiscard]] ParameterType type() const noexcept;
  /// @brief Returns the parameter value.
  [[nodiscard]] std::optional<ParameterValueView> value() const noexcept;
  /// @brief Returns true if the parameter has a value.
  [[nodiscard]] bool hasValue() const noexcept {
    return this->value().has_value();
  };

  /// @brief Checks whether the parameter value matches the specified type.
  ///
  /// Returns false if the parameter does not have a value.
  ///
  /// @tparam T The expected type of the parameter value.
  template<typename T>
  [[nodiscard]] bool is() const noexcept {
    auto value = this->value();
    return value.has_value() && value->is<T>();
  }

  /// @brief Checks whether the parameter value is an array whose elements match
  /// the specified type.
  ///
  /// Returns false if the parameter does not have a value. Returns true for
  /// empty arrays.
  ///
  /// @tparam T The expected type of the array's elements.
  template<typename T>
  [[nodiscard]] bool isArray() const noexcept {
    if (!this->isArray<ParameterValueView>()) {
      return false;
    }
    try {
      const auto& arr = this->get<ParameterValueView::Array>();
      return std::all_of(arr.begin(), arr.end(), [](const ParameterValueView& elem) noexcept {
        return elem.is<T>();
      });
    } catch (...) {
      return false;
    }
  }

  /// @brief Checks whether the parameter value is a dictionary whose values
  /// match the specified type.
  ///
  /// Returns false if the parameter does not have a value. Returns true for an
  /// empty dictionary.
  ///
  /// @tparam T The expected type of the dictionary's values.
  template<typename T>
  [[nodiscard]] bool isDict() const noexcept {
    if (!this->isDict<ParameterValueView>()) {
      return false;
    }
    try {
      const auto& dict = this->get<ParameterValueView::Dict>();
      return std::all_of(
        dict.begin(),
        dict.end(),
        [](const std::pair<std::string, ParameterValueView>& elem) noexcept {
          return elem.second.is<T>();
        }
      );
    } catch (...) {
      return false;
    }
  }

  /// @brief Checks whether the parameter value is a byte array.
  [[nodiscard]] bool isByteArray() const noexcept;

  /// @brief Extracts a value from the parameter.
  ///
  /// This method will throw an exception if the value is unset or the type does
  /// not match. You can use `ParameterView::is<T>()` to check that the type
  /// matches before calling this method.
  ///
  /// @tparam T The type of the value to extract.
  template<typename T>
  [[nodiscard]] T get() const {
    auto value = this->value();
    if (!value) {
      throw std::bad_optional_access();
    }
    return value->template get<T>();
  }

  /// @brief Extracts an array value from the parameter.
  ///
  /// This method will throw an exception if the value is unset, the value is
  /// not an array or the element type does not match. You can use
  /// `ParameterView::isArray<T>()` to check that the element type matches
  /// before calling this method.
  ///
  /// @tparam T The type of the array's elements.
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

  /// @brief Extracts a dictionary value from the parameter.
  ///
  /// This method will throw an exception if the value is unset, the value is
  /// not a dictionary or the value type does not match. You can use
  /// `ParameterView::isDict<T>()` to check that the value type matches before
  /// calling this method.
  ///
  /// @tparam T The type of the dictionary's values.
  template<typename T>
  [[nodiscard]] std::map<std::string, T> getDict() const {
    auto value = this->value();
    if (!value) {
      throw std::bad_optional_access();
    }
    const auto& dict = value->get<ParameterValueView::Dict>();
    std::map<std::string, T> result;
    for (const auto& [dictKey, dictValue] : dict) {
      result.emplace(dictKey, dictValue.template get<T>());
    }
    return result;
  }

  /// @brief Extracts a dictionary value from the parameter.
  ///
  /// This method will return `ValueError` if the value is unset or the value is
  /// not a byte array. It will return `Base64DecodeError` if the value is not a
  /// valid base64-encoding.
  [[nodiscard]] FoxgloveResult<std::vector<std::byte>> getByteArray() const;

private:
  friend class Parameter;
  friend class ParameterArrayView;

  const foxglove_parameter* impl_;

  /// @brief Constructor from raw pointer.
  explicit ParameterView(const foxglove_parameter* ptr)
      : impl_(ptr) {}
};

/// @brief Checks whether the paramter value is an array of generic elements.
template<>
[[nodiscard]] inline bool ParameterView::isArray<ParameterValueView>() const noexcept {
  auto value = this->value();
  return value.has_value() && value->is<ParameterValueView::Array>();
}

/// @brief Checks whether the paramter value is an dictionary of generic
/// values.
template<>
[[nodiscard]] inline bool ParameterView::isDict<ParameterValueView>() const noexcept {
  auto value = this->value();
  return value.has_value() && value->is<ParameterValueView::Dict>();
}

// Template specializations for ParameterView
//
// Documented manually as tparams because specializations are merged:
// https://github.com/doxygen/doxygen/issues/9468

/// @fn ParameterView::is<std::string_view>()
/// @tparam std::string_view Checks whether the parameter value is a string.

/// @fn ParameterView::is<std::string>()
/// @tparam std::string Checks whether the parameter value is a string.

/// @fn ParameterView::is<std::vector<std::byte>>()
/// @tparam std::vector<std::byte> Checks whether the parameter value is a byte array.

/// @fn ParameterView::is<std::vector<double>>()
/// @tparam std::vector<double> Checks whether the parameter value is a vector of floating point
/// values.

/// @fn ParameterView::is<std::vector<int64_t>>()
/// @tparam std::vector<int64_t> Checks whether the parameter value is a vector of integer values.

/// @cond ignore-template-specializations
/// @brief Checks whether the parameter value is a std::string_view.
///
/// Returns false if the parameter type indicates that it is a byte array.
template<>
[[nodiscard]] inline bool ParameterView::is<std::string_view>() const noexcept {
  auto value = this->value();
  return value.has_value() && value->is<std::string_view>() &&
         this->type() != ParameterType::ByteArray;
}

/// @brief Checks whether the parameter value is a std::string.
///
/// Returns false if the parameter type indicates that it is a byte array.
template<>
[[nodiscard]] inline bool ParameterView::is<std::string>() const noexcept {
  return this->is<std::string_view>();
}

/// @brief Checks whether the parameter value is a std::vector<std::byte>.
template<>
[[nodiscard]] inline bool ParameterView::is<std::vector<std::byte>>() const noexcept {
  auto value = this->value();
  return value.has_value() && value->is<std::string_view>() &&
         this->type() == ParameterType::ByteArray;
}

/// @brief Checks whether the parameter value is a std::vector<double>.
template<>
[[nodiscard]] inline bool ParameterView::is<std::vector<double>>() const noexcept {
  return this->isArray<double>();
}

/// @brief Checks whether the parameter value is a std::vector<int64_t>.
template<>
[[nodiscard]] inline bool ParameterView::is<std::vector<int64_t>>() const noexcept {
  return this->isArray<int64_t>();
}

/// @endcond

[[nodiscard]] inline bool ParameterView::isByteArray() const noexcept {
  return this->is<std::vector<std::byte>>();
}

/// @brief Extracts the value as a byte array.
///
/// This method will throw an exception if the value is unset or is not a byte
/// array. You can use `ParameterView::is<std::vector<std::byte>>()` to check
/// that the type matches before calling this method.
template<>
[[nodiscard]] inline std::vector<std::byte> ParameterView::get() const {
  auto result = this->getByteArray();
  if (!result.has_value()) {
    throw std::runtime_error(strerror(result.error()));
  }
  return result.value();
}

/// @brief Extracts the value as an array of floating point values.
///
/// This method will throw an exception if the value is unset or is not an
/// array of floating point values. You can use
/// `ParameterView::is<std::vector<double>>()` to check that the type matches
/// before calling this method.
template<>
[[nodiscard]] inline std::vector<double> ParameterView::get() const {
  return this->getArray<double>();
}

/// @brief Extracts the value as an array of integer values.
///
/// This method will throw an exception if the value is unset or is not an
/// array of integer values. You can use
/// `ParameterView::is<std::vector<int64_t>>()` to check that the type matches
/// before calling this method.
template<>
[[nodiscard]] inline std::vector<int64_t> ParameterView::get() const {
  return this->getArray<int64_t>();
}

/// @brief Extracts the value as an array of generic values.
///
/// This method will throw an exception if the value is unset or is not an
/// array.
template<>
[[nodiscard]] inline std::vector<ParameterValueView> ParameterView::get() const {
  return this->getArray<ParameterValueView>();
}

/// @brief Extracts the value as a dictionary of generic values.
///
/// This method will throw an exception if the value is unset or is not an
/// dictionary.
template<>
[[nodiscard]] inline std::map<std::string, ParameterValueView> ParameterView::get() const {
  return this->getDict<ParameterValueView>();
}

/// @brief An owned parameter.
class Parameter final {
public:
  /// @brief Constructor for an unset parameter.
  explicit Parameter(std::string_view name);
  /// @brief Constructor for a floating point parameter.
  explicit Parameter(std::string_view name, double value);
  /// @brief Constructor for an integer parameter.
  explicit Parameter(std::string_view name, int64_t value);
  /// @brief Constructor for a boolean parameter.
  explicit Parameter(std::string_view name, bool value);
  /// @brief Constructor for a string parameter.
  explicit Parameter(std::string_view name, std::string_view value);
  /// @brief Constructor for a string parameter from a C-style string.
  explicit Parameter(std::string_view name, const char* value)
      : Parameter(name, std::string_view(value)) {}
  /// @brief Constructor for a byte array parameter.
  explicit Parameter(std::string_view name, const uint8_t* data, size_t data_length);
  /// @brief Constructor for a byte array parameter from a vector of bytes.
  explicit Parameter(std::string_view name, const std::vector<std::byte>& bytes)
      : Parameter(name, reinterpret_cast<const uint8_t*>(bytes.data()), bytes.size()) {}
  /// @brief Constructor for an array of floating point values.
  explicit Parameter(std::string_view name, const std::vector<double>& values);
  /// @brief Constructor for an array of integer values.
  explicit Parameter(std::string_view name, const std::vector<int64_t>& values);
  /// @brief Constructor for an dictionary parameter.
  explicit Parameter(std::string_view name, std::map<std::string, ParameterValue> values);
  /// @brief Constructor for a parameter from raw parts.
  explicit Parameter(std::string_view name, ParameterType type, ParameterValue&& value);

  ~Parameter() = default;
  /// @brief Default move constructor.
  Parameter(Parameter&& other) noexcept = default;
  /// @brief Default move assignment.
  Parameter& operator=(Parameter&& other) noexcept = default;
  Parameter(const Parameter&) = delete;
  Parameter& operator=(const Parameter&) = delete;

  /// @brief Creates a deep clone of this parameter.
  [[nodiscard]] Parameter clone() const {
    return this->view().clone();
  }

  /// @brief Returns an immutable view of this parameter.
  [[nodiscard]] ParameterView view() const noexcept;

  /// @brief Returns the parameter name.
  [[nodiscard]] std::string_view name() const noexcept {
    return this->view().name();
  }
  /// @brief Returns the parameter type, used for disambiguation.
  [[nodiscard]] ParameterType type() const noexcept {
    return this->view().type();
  }
  /// @brief Returns the parameter value.
  [[nodiscard]] std::optional<ParameterValueView> value() const noexcept {
    return this->view().value();
  }
  /// @brief Returns true if the parameter has a value.
  [[nodiscard]] bool hasValue() const noexcept {
    return this->view().hasValue();
  };

  /// @brief Checks whether the parameter value matches the specified type.
  ///
  /// Returns false if the parameter does not have a value.
  ///
  /// @tparam T The expected type of the parameter value.
  template<typename T>
  [[nodiscard]] bool is() const {
    return this->view().is<T>();
  }

  /// @brief Checks whether the parameter value is an array whose elements match
  /// the specified type.
  ///
  /// Returns false if the parameter does not have a value. Returns true for
  /// empty arrays.
  ///
  /// @tparam T The expected type of the array's elements.
  template<typename T>
  [[nodiscard]] bool isArray() const noexcept {
    return this->view().isArray<T>();
  }

  /// @brief Checks whether the parameter value is a dictionary whose values
  /// match the specified type.
  ///
  /// Returns false if the parameter does not have a value. Returns true for an
  /// empty dictionary.
  ///
  /// @tparam T The expected type of the dictionary's values.
  template<typename T>
  [[nodiscard]] bool isDict() const noexcept {
    return this->view().isDict<T>();
  }

  /// @brief Checks whether the parameter value is a byte array.
  [[nodiscard]] bool isByteArray() const noexcept {
    return this->view().isByteArray();
  }

  /// @brief Extracts a value from the parameter.
  ///
  /// This method will throw an exception if the value is unset or the type does
  /// not match. You can use `ParameterView::is<T>()` to check that the type
  /// matches before calling this method.
  ///
  /// @tparam T The type of the value to extract.
  template<typename T>
  [[nodiscard]] T get() const {
    return this->view().get<T>();
  }

  /// @brief Extracts an array value from the parameter.
  ///
  /// This method will throw an exception if the value is unset, the value is
  /// not an array or the element type does not match. You can use
  /// `ParameterView::isArray<T>()` to check that the element type matches
  /// before calling this method.
  ///
  /// @tparam T The type of the array's elements.
  template<typename T>
  [[nodiscard]] std::vector<T> getArray() const {
    return this->view().getArray<T>();
  }

  /// @brief Extracts a dictionary value from the parameter.
  ///
  /// This method will throw an exception if the value is unset, the value is
  /// not a dictionary or the value type does not match. You can use
  /// `ParameterView::isDict<T>()` to check that the value type matches before
  /// calling this method.
  ///
  /// @tparam T The type of the dictionary's values.
  template<typename T>
  [[nodiscard]] std::map<std::string, T> getDict() const {
    return this->view().getDict<T>();
  }

  /// @brief Extracts a dictionary value from the parameter.
  ///
  /// This method will return `ValueError` if the value is unset or the value is
  /// not a byte array. It will return `Base64DecodeError` if the value is not a
  /// valid base64-encoding.
  [[nodiscard]] FoxgloveResult<std::vector<std::byte>> getByteArray() const {
    return this->view().getByteArray();
  }

private:
  friend class ParameterView;
  friend class ParameterArray;

  struct Deleter {
    void operator()(foxglove_parameter* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_parameter, Deleter> impl_;

  /// @brief Constructor from raw pointer.
  explicit Parameter(foxglove_parameter* param);

  /// @brief Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter* release() noexcept;
};

/// @brief A view over an unowned parameter array.
///
/// This lifetime of this view is tied to the `ParameterArray` from which it was
/// derived. It is the caller's responsibility to ensure the validity of this
/// lifetime when accessing the view.
class ParameterArrayView final {
public:
  /// @brief Constructs a parameter array from a raw pointer.
  explicit ParameterArrayView(const foxglove_parameter_array* ptr);

  /// @brief Returns a vector of immutable parameter views.
  [[nodiscard]] std::vector<ParameterView> parameters() const;

private:
  const foxglove_parameter_array* impl_;
};

/// @brief An owned parameter array
class ParameterArray final {
public:
  /// @brief Constructs a parameter array.
  explicit ParameterArray(std::vector<Parameter>&& params);

  ~ParameterArray() = default;
  /// @brief Default move constructor.
  ParameterArray(ParameterArray&& other) noexcept = default;
  /// @brief Default move assignment.
  ParameterArray& operator=(ParameterArray&& other) noexcept = default;
  ParameterArray(const ParameterArray&) = delete;
  ParameterArray& operator=(const ParameterArray&) = delete;

  /// @brief Returns an immutable view of this parameter array.
  [[nodiscard]] ParameterArrayView view() const noexcept;
  /// @brief Returns a vector of immutable parameter views.
  [[nodiscard]] std::vector<ParameterView> parameters() const {
    return this->view().parameters();
  }

private:
  friend class WebSocketServer;

  struct Deleter {
    void operator()(foxglove_parameter_array* ptr) const noexcept;
  };

  std::unique_ptr<foxglove_parameter_array, Deleter> impl_;

  /// @brief Releases ownership of the underlying storage.
  [[nodiscard]] foxglove_parameter_array* release() noexcept;
};

}  // namespace foxglove
