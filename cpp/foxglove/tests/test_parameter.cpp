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

  SECTION("integer value") {
    foxglove::ParameterValue value(int64_t(42));
    REQUIRE(value.is<int64_t>());
    REQUIRE(value.get<int64_t>() == 42);
  }

  SECTION("bool value") {
    foxglove::ParameterValue value(true);
    REQUIRE(value.is<bool>());
    REQUIRE(value.get<bool>());
  }

  SECTION("string value") {
    foxglove::ParameterValue value("test string");
    REQUIRE(value.is<std::string>());
    REQUIRE(value.is<std::string_view>());
    REQUIRE(value.get<std::string>() == "test string");
    REQUIRE(value.get<std::string_view>() == "test string");
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

  SECTION("integer array value") {
    std::vector<foxglove::ParameterValue> values;
    values.emplace_back(int64_t(1));
    values.emplace_back(int64_t(2));
    foxglove::ParameterValue value(std::move(values));
    REQUIRE(value.is<foxglove::ParameterValueView::Array>());
    const auto& array = value.get<foxglove::ParameterValueView::Array>();
    REQUIRE(array.size() == 2);
    REQUIRE(array[0].get<int64_t>() == int64_t(1));
    REQUIRE(array[1].get<int64_t>() == int64_t(2));
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

  SECTION("parameter with integer value") {
    foxglove::Parameter param("test_param", int64_t(42));
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.is<int64_t>());
    REQUIRE(param.get<int64_t>() == 42);
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
    REQUIRE(param.is<std::string_view>());
    REQUIRE(!param.is<std::vector<std::byte>>());
    REQUIRE(param.get<std::string>() == "test string");
    REQUIRE(param.get<std::string_view>() == "test string");
  }

  SECTION("parameter with byte array value") {
    std::array<uint8_t, 4> data = {1, 2, 3, 4};
    foxglove::Parameter param("test_param", data.data(), data.size());
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::ByteArray);
    REQUIRE(!param.is<std::string>());
    REQUIRE(param.is<std::vector<std::byte>>());
    auto decoded = param.get<std::vector<std::byte>>();
    REQUIRE(decoded.size() == data.size());
    REQUIRE(memcmp(decoded.data(), data.data(), data.size()) == 0);

    // Alternative checkers/extractors.
    REQUIRE(param.isByteArray());
    auto result = param.getByteArray();
    REQUIRE(result.has_value());
    decoded = result.value();
    REQUIRE(decoded.size() == data.size());
    REQUIRE(memcmp(decoded.data(), data.data(), data.size()) == 0);
  }

  SECTION("parameter with float64 array value") {
    std::vector<double> values = {1.0, 2.0, 3.0};
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64Array);
    REQUIRE(param.is<std::vector<double>>());
    REQUIRE(param.get<std::vector<double>>() == values);

    // Alternative checkers/extractors.
    REQUIRE(param.isArray<double>());
    REQUIRE(param.getArray<double>() == values);

    REQUIRE(param.isArray<foxglove::ParameterValueView>());
    auto generic_array = param.getArray<foxglove::ParameterValueView>();
    REQUIRE(generic_array.size() == 3);
    REQUIRE(generic_array[0].get<double>() == 1.0);
    REQUIRE(generic_array[1].get<double>() == 2.0);
    REQUIRE(generic_array[2].get<double>() == 3.0);

    REQUIRE(param.is<foxglove::ParameterValueView::Array>());
    generic_array = param.get<foxglove::ParameterValueView::Array>();
    REQUIRE(generic_array.size() == 3);
    REQUIRE(generic_array[0].get<double>() == 1.0);
    REQUIRE(generic_array[1].get<double>() == 2.0);
    REQUIRE(generic_array[2].get<double>() == 3.0);
  }

  SECTION("parameter with empty float64 array value") {
    std::vector<double> values;
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::Float64Array);
    REQUIRE(param.is<std::vector<double>>());
    REQUIRE(param.get<std::vector<double>>() == values);

    // Alternative checkers/extractors.
    REQUIRE(param.isArray<double>());
    REQUIRE(param.getArray<double>() == values);

    REQUIRE(param.isArray<foxglove::ParameterValueView>());
    auto generic_array = param.getArray<foxglove::ParameterValueView>();
    REQUIRE(generic_array.empty());

    REQUIRE(param.is<foxglove::ParameterValueView::Array>());
    generic_array = param.get<foxglove::ParameterValueView::Array>();
    REQUIRE(generic_array.empty());
  }

  SECTION("parameter with integer array value") {
    std::vector<int64_t> values = {1LL, 2LL, 3LL};
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.is<std::vector<int64_t>>());
    REQUIRE(param.get<std::vector<int64_t>>() == values);

    // Alternative checkers/extractors.
    REQUIRE(param.isArray<int64_t>());
    REQUIRE(param.getArray<int64_t>() == values);

    REQUIRE(param.isArray<foxglove::ParameterValueView>());
    auto generic_array = param.getArray<foxglove::ParameterValueView>();
    REQUIRE(generic_array.size() == 3);
    REQUIRE(generic_array[0].get<int64_t>() == 1LL);
    REQUIRE(generic_array[1].get<int64_t>() == 2LL);
    REQUIRE(generic_array[2].get<int64_t>() == 3LL);

    REQUIRE(param.is<foxglove::ParameterValueView::Array>());
    generic_array = param.get<foxglove::ParameterValueView::Array>();
    REQUIRE(generic_array.size() == 3);
    REQUIRE(generic_array[0].get<int64_t>() == 1LL);
    REQUIRE(generic_array[1].get<int64_t>() == 2LL);
    REQUIRE(generic_array[2].get<int64_t>() == 3LL);
  }

  SECTION("parameter with empty integer array value") {
    std::vector<int64_t> values;
    foxglove::Parameter param("test_param", values);
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.is<std::vector<int64_t>>());
    REQUIRE(param.get<std::vector<int64_t>>() == values);

    // Alternative checkers/extractors.
    REQUIRE(param.isArray<int64_t>());
    REQUIRE(param.getArray<int64_t>() == values);

    REQUIRE(param.isArray<foxglove::ParameterValueView>());
    auto generic_array = param.getArray<foxglove::ParameterValueView>();
    REQUIRE(generic_array.empty());

    REQUIRE(param.is<foxglove::ParameterValueView::Array>());
    generic_array = param.get<foxglove::ParameterValueView::Array>();
    REQUIRE(generic_array.empty());
  }

  SECTION("parameter with dict value") {
    std::map<std::string, foxglove::ParameterValue> values;
    values.insert(std::make_pair("key1", foxglove::ParameterValue(1.0)));
    values.insert(std::make_pair("key2", foxglove::ParameterValue(2.0)));
    foxglove::Parameter param("test_param", std::move(values));
    REQUIRE(param.name() == "test_param");
    REQUIRE(param.type() == foxglove::ParameterType::None);
    REQUIRE(param.isDict<double>());
    auto dict = param.getDict<double>();
    REQUIRE(dict.size() == 2);
    REQUIRE(dict["key1"] == 1.0);
    REQUIRE(dict["key2"] == 2.0);

    // Alternative checkers/extractors.
    REQUIRE(param.isDict<foxglove::ParameterValueView>());
    auto generic_dict = param.getDict<foxglove::ParameterValueView>();
    REQUIRE(generic_dict.size() == 2);
    REQUIRE(generic_dict.at("key1").get<double>() == 1.0);
    REQUIRE(generic_dict.at("key2").get<double>() == 2.0);

    REQUIRE(param.is<foxglove::ParameterValueView::Dict>());
    generic_dict = param.get<foxglove::ParameterValueView::Dict>();
    REQUIRE(generic_dict.size() == 2);
    REQUIRE(generic_dict.at("key1").get<double>() == 1.0);
    REQUIRE(generic_dict.at("key2").get<double>() == 2.0);
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

TEST_CASE("ParameterArray functionality with integers") {
  std::vector<foxglove::Parameter> params;
  params.emplace_back("param1", int64_t(1));
  params.emplace_back("param2", int64_t(2));
  params.emplace_back("param3", int64_t(3));

  foxglove::ParameterArray array(std::move(params));
  auto parameters = array.parameters();

  REQUIRE(parameters.size() == 3);
  REQUIRE(parameters[0].name() == "param1");
  REQUIRE(parameters[1].name() == "param2");
  REQUIRE(parameters[2].name() == "param3");
  REQUIRE(parameters[0].get<int64_t>() == 1);
  REQUIRE(parameters[1].get<int64_t>() == 2);
  REQUIRE(parameters[2].get<int64_t>() == 3);
}

TEST_CASE("Parameter error cases") {
  SECTION("invalid type conversions") {
    foxglove::Parameter param("test_param", 42.0);
    REQUIRE_THROWS_AS(param.get<bool>(), std::bad_variant_access);
    REQUIRE_THROWS_AS(param.get<std::string>(), std::bad_variant_access);
    REQUIRE_THROWS_AS(param.get<std::vector<double>>(), std::bad_variant_access);
  }

  SECTION("accessing unset values") {
    foxglove::Parameter param("test_param");
    REQUIRE_THROWS_AS(param.get<double>(), std::bad_optional_access);
    REQUIRE_THROWS_AS(param.get<bool>(), std::bad_optional_access);
    REQUIRE_THROWS_AS(param.get<std::string>(), std::bad_optional_access);
  }

  SECTION("invalid byte array decoding") {
    foxglove::Parameter param(
      "test_param", foxglove::ParameterType::ByteArray, foxglove::ParameterValue("invalid-base64!")
    );
    auto result = param.getByteArray();
    REQUIRE(!result.has_value());
    REQUIRE(result.error() == foxglove::FoxgloveError::Base64DecodeError);
  }
}

TEST_CASE("Empty collections") {
  SECTION("empty parameter array") {
    std::vector<foxglove::Parameter> params;
    foxglove::ParameterArray array(std::move(params));
    auto parameters = array.parameters();
    REQUIRE(parameters.empty());
  }

  SECTION("empty array value") {
    std::vector<foxglove::ParameterValue> values;
    foxglove::ParameterValue value(std::move(values));
    REQUIRE(value.is<foxglove::ParameterValueView::Array>());
    const auto& array = value.get<foxglove::ParameterValueView::Array>();
    REQUIRE(array.empty());
  }

  SECTION("empty dictionary value") {
    std::map<std::string, foxglove::ParameterValue> values;
    foxglove::ParameterValue value(std::move(values));
    REQUIRE(value.is<foxglove::ParameterValueView::Dict>());
    const auto& dict = value.get<foxglove::ParameterValueView::Dict>();
    REQUIRE(dict.empty());
  }
}

TEST_CASE("Parameter cloning") {
  SECTION("clone simple parameter") {
    foxglove::Parameter original("test_param", 42.0);
    auto clone = original.clone();
    REQUIRE(clone.name() == original.name());
    REQUIRE(clone.type() == original.type());
    REQUIRE(clone.get<double>() == original.get<double>());
  }

  SECTION("clone complex parameter") {
    std::vector<foxglove::ParameterValue> array_values;
    array_values.emplace_back(1.0);
    array_values.emplace_back(2.0);

    std::map<std::string, foxglove::ParameterValue> dict_values;
    dict_values.insert(std::make_pair("nested", foxglove::ParameterValue(std::move(array_values))));

    foxglove::Parameter original("test_param", std::move(dict_values));
    auto clone = original.clone();

    REQUIRE(clone.name() == original.name());
    REQUIRE(clone.type() == original.type());

    const auto& original_dict = original.get<foxglove::ParameterValueView::Dict>();
    const auto& clone_dict = clone.get<foxglove::ParameterValueView::Dict>();

    REQUIRE(original_dict.size() == clone_dict.size());
    REQUIRE(original_dict.at("nested").is<foxglove::ParameterValueView::Array>());
    REQUIRE(clone_dict.at("nested").is<foxglove::ParameterValueView::Array>());

    const auto& original_array =
      original_dict.at("nested").get<foxglove::ParameterValueView::Array>();
    const auto& clone_array = clone_dict.at("nested").get<foxglove::ParameterValueView::Array>();

    REQUIRE(original_array.size() == clone_array.size());
    REQUIRE(original_array[0].get<double>() == clone_array[0].get<double>());
    REQUIRE(original_array[1].get<double>() == clone_array[1].get<double>());
  }

  SECTION("clone parameter value") {
    std::vector<foxglove::ParameterValue> values;
    values.emplace_back(1.0);
    values.emplace_back(2.0);
    foxglove::ParameterValue original(std::move(values));

    auto clone = original.clone();
    REQUIRE(clone.is<foxglove::ParameterValueView::Array>());

    const auto& original_array = original.get<foxglove::ParameterValueView::Array>();
    const auto& clone_array = clone.get<foxglove::ParameterValueView::Array>();

    REQUIRE(original_array.size() == clone_array.size());
    REQUIRE(original_array[0].get<double>() == clone_array[0].get<double>());
    REQUIRE(original_array[1].get<double>() == clone_array[1].get<double>());
  }

  SECTION("clone parameter value with integer array") {
    std::vector<foxglove::ParameterValue> values;
    values.emplace_back(int64_t(1));
    values.emplace_back(int64_t(2));
    foxglove::ParameterValue original(std::move(values));
    auto clone = original.clone();
    REQUIRE(clone.is<foxglove::ParameterValueView::Array>());
    const auto& original_array = original.get<foxglove::ParameterValueView::Array>();
    const auto& clone_array = clone.get<foxglove::ParameterValueView::Array>();
    REQUIRE(original_array.size() == clone_array.size());
    REQUIRE(original_array[0].get<int64_t>() == clone_array[0].get<int64_t>());
    REQUIRE(original_array[1].get<int64_t>() == clone_array[1].get<int64_t>());
  }
}
