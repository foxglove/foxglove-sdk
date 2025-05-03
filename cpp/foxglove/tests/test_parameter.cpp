#include <foxglove/error.hpp>
#include <foxglove/server/parameter.hpp>

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
    REQUIRE(value.is<double>());
    REQUIRE(value.get<double>() == 42.0);
  }

  SECTION("bool value") {
    foxglove::ParameterValue value(true);
    REQUIRE(value.is<bool>());
    REQUIRE(value.get<bool>());
  }

  SECTION("string value") {
    foxglove::ParameterValue value("test string");
    REQUIRE(value.is<std::string>());
    REQUIRE(value.get<std::string>() == "test string");
  }

  SECTION("array value") {
    std::vector<foxglove::ParameterValue> values;
    values.emplace_back(1.0);
    values.emplace_back(2.0);
    foxglove::ParameterValue value(std::move(values));
    REQUIRE(value.is<foxglove::ParameterValueView::Array>());
    const auto& array = value.get<foxglove::ParameterValueView::Array>();
    REQUIRE(array.size() == 2);
    REQUIRE(array[0].get<double>() == 1.0);
    REQUIRE(array[1].get<double>() == 2.0);
  }

  SECTION("dict value") {
    std::map<std::string, foxglove::ParameterValue> values;
    values.insert(std::make_pair("key1", foxglove::ParameterValue(1.0)));
    values.insert(std::make_pair("key2", foxglove::ParameterValue("value")));
    foxglove::ParameterValue value(std::move(values));
    REQUIRE(value.is<foxglove::ParameterValueView::Dict>());
    const auto& dict = value.get<foxglove::ParameterValueView::Dict>();
    REQUIRE(dict.size() == 2);
    REQUIRE(dict.at("key1").get<double>() == 1.0);
    REQUIRE(dict.at("key2").get<std::string>() == "value");
  }
}

TEST_CASE("Parameter construction and access") {
  SECTION("parameter without value") {
    foxglove::Parameter param("test_param");
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(!param.hasValue());
  }

  SECTION("parameter with double value") {
    foxglove::Parameter param("test_param", 42.0);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64);
    REQUIRE(param.is<double>());
    REQUIRE(param.get<double>() == 42.0);
  }

  SECTION("parameter with bool value") {
    foxglove::Parameter param("test_param", true);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.is<bool>());
    REQUIRE(param.get<bool>());
  }

  SECTION("parameter with string value") {
    foxglove::Parameter param("test_param", "test string");
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.is<std::string>());
    REQUIRE(param.get<std::string>() == "test string");
  }

  SECTION("parameter with byte array value") {
    std::array<uint8_t, 4> data = {1, 2, 3, 4};
    foxglove::Parameter param("test_param", data.data(), data.size());
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::ByteArray);
    REQUIRE(param.isByteArray());
    auto result = param.getByteArray();
    REQUIRE(result.has_value());
    auto decoded = result.value();
    REQUIRE(decoded.size() == data.size());
    REQUIRE(memcmp(decoded.data(), data.data(), data.size()) == 0);
  }

  SECTION("parameter with float64 array value") {
    std::vector<double> values = {1.0, 2.0, 3.0};
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64Array);
    REQUIRE(param.hasValue());
    REQUIRE(param.getArray<double>() == values);
  }

  SECTION("parameter with dict value") {
    std::map<std::string, foxglove::ParameterValue> values;
    values.insert(std::make_pair("key1", foxglove::ParameterValue(1.0)));
    values.insert(std::make_pair("key2", foxglove::ParameterValue(2.0)));
    foxglove::Parameter param("test_param", std::move(values));
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.hasValue());
    auto dict = param.getDict<double>();
    REQUIRE(dict.size() == 2);
    REQUIRE(dict["key1"] == 1.0);
    REQUIRE(dict["key2"] == 2.0);
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
  REQUIRE(parameters[0].get<double>() == 1.0);
  REQUIRE(parameters[1].get<double>() == 2.0);
  REQUIRE(parameters[2].get<double>() == 3.0);
}
