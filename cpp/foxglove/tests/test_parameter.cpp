#include <foxglove/error.hpp>
#include <foxglove/parameter.hpp>

#include <catch2/catch_test_macros.hpp>
#include <catch2/matchers/catch_matchers_string.hpp>

#include <array>
#include <map>
#include <string>
#include <vector>

using Catch::Matchers::Equals;

TEST_CASE("ParameterValue construction and access") {
  SECTION("double value") {
    foxglove::ParameterValue value(42.0);
    auto view = value.view();
    auto variant = view.value();
    REQUIRE(std::holds_alternative<double>(variant));
    REQUIRE(std::get<double>(variant) == 42.0);
  }

  SECTION("bool value") {
    foxglove::ParameterValue value(true);
    auto view = value.view();
    auto variant = view.value();
    REQUIRE(std::holds_alternative<bool>(variant));
    REQUIRE(std::get<bool>(variant) == true);
  }

  SECTION("string value") {
    foxglove::ParameterValue value("test string");
    auto view = value.view();
    auto variant = view.value();
    REQUIRE(std::holds_alternative<std::string>(variant));
    REQUIRE(std::get<std::string>(variant) == "test string");
  }

  SECTION("array value") {
    std::vector<foxglove::ParameterValue> values;
    values.emplace_back(1.0);
    values.emplace_back(2.0);
    foxglove::ParameterValue value(std::move(values));
    auto view = value.view();
    auto variant = view.value();
    REQUIRE(std::holds_alternative<foxglove::ParameterValueView::Array>(variant));
    const auto& array = std::get<foxglove::ParameterValueView::Array>(variant);
    REQUIRE(array.size() == 2);
    REQUIRE(std::get<double>(array[0].value()) == 1.0);
    REQUIRE(std::get<double>(array[1].value()) == 2.0);
  }

  SECTION("dict value") {
    std::map<std::string, foxglove::ParameterValue> values;
    values.insert(std::make_pair("key1", foxglove::ParameterValue(1.0)));
    values.insert(std::make_pair("key2", foxglove::ParameterValue("value")));
    foxglove::ParameterValue value(std::move(values));
    auto view = value.view();
    auto variant = view.value();
    REQUIRE(std::holds_alternative<foxglove::ParameterValueView::Dict>(variant));
    const auto& dict = std::get<foxglove::ParameterValueView::Dict>(variant);
    REQUIRE(dict.size() == 2);
    REQUIRE(std::get<double>(dict.at("key1").value()) == 1.0);
    REQUIRE(std::get<std::string>(dict.at("key2").value()) == "value");
  }
}

TEST_CASE("Parameter construction and access") {
  SECTION("parameter without value") {
    foxglove::Parameter param("test_param");
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    auto value = param.value();
    REQUIRE(!value.has_value());
  }

  SECTION("parameter with double value") {
    foxglove::Parameter param("test_param", 42.0);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64);
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<double>(*value));
      REQUIRE(std::get<double>(*value) == 42.0);
    }
  }

  SECTION("parameter with bool value") {
    foxglove::Parameter param("test_param", true);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);  // bool is not a supported type
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<bool>(*value));
      REQUIRE(std::get<bool>(*value) == true);
    }
  }

  SECTION("parameter with string value") {
    foxglove::Parameter param("test_param", "test string");
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);  // string is not a supported type
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<std::string>(*value));
      REQUIRE(std::get<std::string>(*value) == "test string");
    }
  }

  SECTION("parameter with byte array value") {
    std::array<uint8_t, 4> data = {1, 2, 3, 4};
    foxglove::Parameter param("test_param", data.data(), data.size());
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::ByteArray);
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<std::string>(*value));
      REQUIRE(std::get<std::string>(*value) == "AQIDBA==");
    }
  }

  SECTION("parameter with float64 array value") {
    std::vector<double> values = {1.0, 2.0, 3.0};
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64Array);
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<foxglove::ParameterValueView::Array>(*value));
      const auto& array = std::get<foxglove::ParameterValueView::Array>(*value);
      REQUIRE(array.size() == 3);
      REQUIRE(std::get<double>(array[0].value()) == 1.0);
      REQUIRE(std::get<double>(array[1].value()) == 2.0);
      REQUIRE(std::get<double>(array[2].value()) == 3.0);
    }
  }

  SECTION("parameter with dict value") {
    std::map<std::string, foxglove::ParameterValue> values;
    values.insert(std::make_pair("key1", foxglove::ParameterValue(1.0)));
    values.insert(std::make_pair("key2", foxglove::ParameterValue("value")));
    foxglove::Parameter param("test_param", std::move(values));
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    auto value = param.value();
    REQUIRE(value.has_value());
    if (value.has_value()) {
      REQUIRE(std::holds_alternative<foxglove::ParameterValueView::Dict>(*value));
      const auto& dict = std::get<foxglove::ParameterValueView::Dict>(*value);
      REQUIRE(dict.size() == 2);
      REQUIRE(std::get<double>(dict.at("key1").value()) == 1.0);
      REQUIRE(std::get<std::string>(dict.at("key2").value()) == "value");
    }
  }
}

TEST_CASE("ParameterArray functionality") {
  std::vector<foxglove::Parameter> params;
  params.emplace_back("param1", 1.0);
  params.emplace_back("param2", 2.0);
  params.emplace_back("param3", 3.0);

  foxglove::ParameterArray array(std::move(params));
  auto parameters = array.parameters();

  REQUIRE(parameters.size() == 3);
  REQUIRE(parameters[0].name() == "param1");
  REQUIRE(parameters[1].name() == "param2");
  REQUIRE(parameters[2].name() == "param3");

  auto value1 = parameters[0].value();
  auto value2 = parameters[1].value();
  auto value3 = parameters[2].value();

  REQUIRE(value1.has_value());
  REQUIRE(value2.has_value());
  REQUIRE(value3.has_value());

  if (value1.has_value()) {
    REQUIRE(std::get<double>(*value1) == 1.0);
  }
  if (value2.has_value()) {
    REQUIRE(std::get<double>(*value2) == 2.0);
  }
  if (value3.has_value()) {
    REQUIRE(std::get<double>(*value3) == 3.0);
  }
}
