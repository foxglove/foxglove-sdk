#include <chrono>
#include <filesystem>
#include <future>
#include <thread>

#include <gtest/gtest.h>
#include <std_msgs/msg/string.hpp>
#include <std_srvs/srv/set_bool.hpp>
#include <websocketpp/config/asio_client.hpp>

#include <foxglove_bridge/ros2_foxglove_bridge.hpp>
#include <foxglove_bridge/test/test_client.hpp>

constexpr char URI[] = "ws://localhost:8765";

// Binary representation of std_msgs/msg/String for "hello world"
constexpr uint8_t HELLO_WORLD_CDR[] = {0,   1,   0,   0,  12,  0,   0,   0,   104, 101,
                                       108, 108, 111, 32, 119, 111, 114, 108, 100, 0};
constexpr char HELLO_WORLD_JSON[] = "{\"data\": \"hello world\"}";
constexpr char STD_MSGS_STRING_SCHEMA[] = "data string";

constexpr auto ONE_SECOND = std::chrono::seconds(1);
constexpr auto DEFAULT_TIMEOUT = std::chrono::seconds(10);

class TestWithExecutor : public testing::Test {
protected:
  TestWithExecutor() {
    this->_executorThread = std::thread([this]() {
      this->executor.spin();
    });
  }

  ~TestWithExecutor() override {
    this->executor.cancel();
    this->_executorThread.join();
  }

  rclcpp::executors::SingleThreadedExecutor executor;

private:
  std::thread _executorThread;
};

class ParameterTest : public TestWithExecutor {
public:
  using PARAM_1_TYPE = std::string;
  inline static const std::string NODE_1_NAME = "node_1";
  inline static const std::string PARAM_1_NAME = "string_param";
  inline static const PARAM_1_TYPE PARAM_1_DEFAULT_VALUE = "hello";
  inline static const std::string DELETABLE_PARAM_NAME = "deletable_param";

  using PARAM_2_TYPE = std::vector<int64_t>;
  inline static const std::string NODE_2_NAME = "node_2";
  inline static const std::string PARAM_2_NAME = "int_array_param";
  inline static const PARAM_2_TYPE PARAM_2_DEFAULT_VALUE = {1, 2, 3};

  using PARAM_3_TYPE = double;
  inline static const std::string PARAM_3_NAME = "float_param";
  inline static const PARAM_3_TYPE PARAM_3_DEFAULT_VALUE = 1.123;

  using PARAM_4_TYPE = std::vector<double>;
  inline static const std::string PARAM_4_NAME = "float_array_param";
  inline static const PARAM_4_TYPE PARAM_4_DEFAULT_VALUE = {1.1, 2.2, 3.3};

  using PARAM_5_TYPE = int64_t;
  inline static const std::string PARAM_5_NAME = "int_param";
  inline static const PARAM_5_TYPE PARAM_5_DEFAULT_VALUE = 123;

protected:
  void SetUp() override {
    auto nodeOptions = rclcpp::NodeOptions();
    nodeOptions.allow_undeclared_parameters(true);
    _paramNode1 = rclcpp::Node::make_shared(NODE_1_NAME, nodeOptions);
    auto p1Param = rcl_interfaces::msg::ParameterDescriptor{};
    p1Param.name = PARAM_1_NAME;
    p1Param.type = rcl_interfaces::msg::ParameterType::PARAMETER_STRING;
    p1Param.read_only = false;
    _paramNode1->declare_parameter(p1Param.name, PARAM_1_DEFAULT_VALUE, p1Param);
    _paramNode1->set_parameter(rclcpp::Parameter(DELETABLE_PARAM_NAME, true));

    _paramNode2 = rclcpp::Node::make_shared(NODE_2_NAME);
    auto p2Param = rcl_interfaces::msg::ParameterDescriptor{};
    p2Param.name = PARAM_2_NAME;
    p2Param.type = rcl_interfaces::msg::ParameterType::PARAMETER_INTEGER_ARRAY;
    p2Param.read_only = false;
    _paramNode2->declare_parameter(p2Param.name, PARAM_2_DEFAULT_VALUE, p2Param);
    _paramNode2->declare_parameter(PARAM_3_NAME, PARAM_3_DEFAULT_VALUE);
    _paramNode2->declare_parameter(PARAM_4_NAME, PARAM_4_DEFAULT_VALUE);
    _paramNode2->declare_parameter(PARAM_5_NAME, PARAM_5_DEFAULT_VALUE);

    executor.add_node(_paramNode1);
    executor.add_node(_paramNode2);

    _wsClient = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
    ASSERT_EQ(std::future_status::ready, _wsClient->connect(URI).wait_for(DEFAULT_TIMEOUT));
  }

  void TearDown() override {
    executor.remove_node(_paramNode1);
    executor.remove_node(_paramNode2);
  }

  rclcpp::Node::SharedPtr _paramNode1;
  rclcpp::Node::SharedPtr _paramNode2;
  std::shared_ptr<foxglove::test::Client<websocketpp::config::asio_client>> _wsClient;
};

// TODO: FG-12234: Enable when Service functionality is implemented
class ServiceTest : public TestWithExecutor {
public:
  inline static const std::string SERVICE_NAME = "/foo_service";

protected:
  void SetUp() override {
    _node = rclcpp::Node::make_shared("node");
    _service = _node->create_service<std_srvs::srv::SetBool>(
      SERVICE_NAME, [&](std::shared_ptr<std_srvs::srv::SetBool::Request> req,
                        std::shared_ptr<std_srvs::srv::SetBool::Response> res) {
        res->message = "hello";
        res->success = req->data;
      });
    executor.add_node(_node);
  }

  void TearDown() override {
    executor.remove_node(_node);
  }

  rclcpp::Node::SharedPtr _node;
  rclcpp::ServiceBase::SharedPtr _service;
  std::shared_ptr<foxglove::test::Client<websocketpp::config::asio_client>> _wsClient;
};

class PublisherTest
    : public TestWithExecutor,
      public testing::WithParamInterface<std::pair<std::string, std::vector<uint8_t>>> {};

class ExistingPublisherTest : public PublisherTest {
public:
  inline static const std::string TOPIC_NAME = "/some_topic";

protected:
  void SetUp() override {
    _node = rclcpp::Node::make_shared("node");
    _publisher =
      _node->create_publisher<std_msgs::msg::String>(TOPIC_NAME, rclcpp::SystemDefaultsQoS());
    executor.add_node(_node);
  }

  void TearDown() override {
    executor.remove_node(_node);
  }

  rclcpp::Node::SharedPtr _node;
  rclcpp::PublisherBase::SharedPtr _publisher;
};

template <class T>
std::shared_ptr<rclcpp::SerializedMessage> serializeMsg(const T* msg) {
  using rosidl_typesupport_cpp::get_message_type_support_handle;
  auto typeSupportHdl = get_message_type_support_handle<T>();
  auto result = std::make_shared<rclcpp::SerializedMessage>();
  rmw_ret_t ret = rmw_serialize(msg, typeSupportHdl, &result->get_rcl_serialized_message());
  EXPECT_EQ(ret, RMW_RET_OK);
  return result;
}

template <class T>
std::shared_ptr<T> deserializeMsg(const rcl_serialized_message_t* msg) {
  using rosidl_typesupport_cpp::get_message_type_support_handle;
  auto typeSupportHdl = get_message_type_support_handle<T>();
  auto result = std::make_shared<T>();
  rmw_ret_t ret = rmw_deserialize(msg, typeSupportHdl, result.get());
  EXPECT_EQ(ret, RMW_RET_OK);
  return result;
}

TEST(SmokeTest, testConnection) {
  foxglove::test::Client<websocketpp::config::asio_client> wsClient;
  EXPECT_EQ(std::future_status::ready, wsClient.connect(URI).wait_for(DEFAULT_TIMEOUT));
}

TEST(SmokeTest, testSubscription) {
  // Publish a string message on a latched ros topic
  const std::string topic_name = "/pub_topic";
  std_msgs::msg::String rosMsg;
  rosMsg.data = "hello world";

  auto node = rclcpp::Node::make_shared("tester");
  rclcpp::QoS qos = rclcpp::QoS{rclcpp::KeepLast(1lu)};
  qos.reliable();
  qos.transient_local();
  auto pub = node->create_publisher<std_msgs::msg::String>(topic_name, qos);
  pub->publish(rosMsg);

  // Connect a few clients and make sure that they receive the correct message
  const auto clientCount = 3;
  for (auto i = 0; i < clientCount; ++i) {
    // Set up a client and subscribe to the channel.
    auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
    auto channelFuture = client->waitForChannel(topic_name);
    ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
    ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(ONE_SECOND));
    const foxglove::test::Channel channel = channelFuture.get();
    const foxglove::test::SubscriptionId subscriptionId = 1;

    // Subscribe to the channel and confirm that the promise resolves
    auto msgFuture = client->waitForChannelMsg(subscriptionId);
    client->subscribe({{subscriptionId, channel.id}});
    ASSERT_EQ(std::future_status::ready, msgFuture.wait_for(ONE_SECOND));
    const auto msgData = msgFuture.get();
    ASSERT_EQ(sizeof(HELLO_WORLD_CDR), msgData.size());
    EXPECT_EQ(0, std::memcmp(HELLO_WORLD_CDR, msgData.data(), msgData.size()));

    // Unsubscribe from the channel again.
    client->unsubscribe({subscriptionId});
  }
}

TEST(SmokeTest, testSubscriptionParallel) {
  // Publish a string message on a latched ros topic
  const std::string topic_name = "/pub_topic";
  std_msgs::msg::String rosMsg;
  rosMsg.data = "hello world";

  auto node = rclcpp::Node::make_shared("tester");
  rclcpp::QoS qos = rclcpp::QoS{rclcpp::KeepLast(1lu)};
  qos.reliable();
  qos.transient_local();
  auto pub = node->create_publisher<std_msgs::msg::String>(topic_name, qos);
  pub->publish(rosMsg);

  // Connect a few clients (in parallel) and make sure that they receive the correct message
  const foxglove::test::SubscriptionId subscriptionId = 1;
  auto clients = {
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
  };

  std::vector<std::future<std::vector<uint8_t>>> futures;
  for (auto client : clients) {
    futures.push_back(client->waitForChannelMsg(subscriptionId));
  }

  for (auto client : clients) {
    auto channelFuture = client->waitForChannel(topic_name);
    ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
    ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(ONE_SECOND));
    const foxglove::test::Channel channel = channelFuture.get();
    client->subscribe({{subscriptionId, channel.id}});
  }

  for (auto& future : futures) {
    ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
    auto msgData = future.get();
    ASSERT_EQ(sizeof(HELLO_WORLD_CDR), msgData.size());
    EXPECT_EQ(0, std::memcmp(HELLO_WORLD_CDR, msgData.data(), msgData.size()));
  }

  for (auto client : clients) {
    client->unsubscribe({subscriptionId});
  }
}

TEST_P(PublisherTest, testPublishing) {
  const auto& [encoding, message] = GetParam();
  // use a unique topic for each test to prevent tests from interfering with each other
  static size_t testPublishingCount = 0;
  ++testPublishingCount;

  foxglove::test::ClientAdvertisement advertisement;
  advertisement.channelId = 1;
  advertisement.topic = "/testPublishing_" + std::to_string(testPublishingCount);
  advertisement.encoding = encoding;
  advertisement.schemaName = "std_msgs/msg/String";
  advertisement.schema =
    std::vector<char>(STD_MSGS_STRING_SCHEMA, std::end(STD_MSGS_STRING_SCHEMA));

  // Set up a ROS node with a subscriber
  std::promise<std::string> msgPromise;
  auto msgFuture = msgPromise.get_future();
  auto node = rclcpp::Node::make_shared("tester");
  auto sub = node->create_subscription<std_msgs::msg::String>(
    advertisement.topic, 10, [&msgPromise](std::shared_ptr<const std_msgs::msg::String> msg) {
      msgPromise.set_value(msg->data);
    });
  this->executor.add_node(node);

  // Set up the client, advertise and publish the binary message
  auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  auto channelFuture = client->waitForChannel(advertisement.topic);
  ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
  client->advertise({advertisement});

  // Wait until the advertisement got advertised as channel by the server
  ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(ONE_SECOND));

  // Publish the message and unadvertise again
  client->publish(advertisement.channelId, message.data(), message.size());
  client->unadvertise({advertisement.channelId});

  // Ensure that we have received the correct message via our ROS subscriber
  ASSERT_EQ(std::future_status::ready, msgFuture.wait_for(DEFAULT_TIMEOUT));
  this->executor.remove_node(node);
  EXPECT_EQ("hello world", msgFuture.get());
}

INSTANTIATE_TEST_SUITE_P(
  TestPublishingCDR, PublisherTest,
  testing::Values(std::make_pair("cdr", std::vector<uint8_t>(HELLO_WORLD_CDR,
                                                             std::end(HELLO_WORLD_CDR)))));

INSTANTIATE_TEST_SUITE_P(
  TestPublishingJSON, PublisherTest,
  testing::Values(std::make_pair("json", std::vector<uint8_t>(HELLO_WORLD_JSON,
                                                              std::end(HELLO_WORLD_JSON)))));

TEST_P(ExistingPublisherTest, testPublishingWithExistingPublisher) {
  const auto& [encoding, message] = GetParam();

  foxglove::test::ClientAdvertisement advertisement;
  advertisement.channelId = 1;
  advertisement.topic = TOPIC_NAME;
  advertisement.encoding = encoding;
  advertisement.schemaName = "std_msgs/msg/String";
  advertisement.schema = {};  // Schema intentionally left empty.

  // Set up a ROS node with a subscriber
  std::promise<std::string> msgPromise;
  auto msgFuture = msgPromise.get_future();
  auto node = rclcpp::Node::make_shared("tester");
  auto sub = node->create_subscription<std_msgs::msg::String>(
    advertisement.topic, 10, [&msgPromise](std::shared_ptr<const std_msgs::msg::String> msg) {
      msgPromise.set_value(msg->data);
    });
  rclcpp::executors::SingleThreadedExecutor executor;
  executor.add_node(node);

  // Set up the client, advertise and publish the binary message
  auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  auto channelFuture = client->waitForChannel(advertisement.topic);
  ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
  client->advertise({advertisement});

  // Wait until the advertisement got advertised as channel by the server
  ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(ONE_SECOND));

  // Publish the message and unadvertise again
  client->publish(advertisement.channelId, message.data(), message.size());
  client->unadvertise({advertisement.channelId});

  // Ensure that we have received the correct message via our ROS subscriber
  const auto ret = executor.spin_until_future_complete(msgFuture, ONE_SECOND);
  ASSERT_EQ(rclcpp::FutureReturnCode::SUCCESS, ret);
  EXPECT_EQ("hello world", msgFuture.get());
}

INSTANTIATE_TEST_SUITE_P(
  ExistingPublisherTestCDR, ExistingPublisherTest,
  testing::Values(std::make_pair("cdr", std::vector<uint8_t>(HELLO_WORLD_CDR,
                                                             std::end(HELLO_WORLD_CDR)))));

INSTANTIATE_TEST_SUITE_P(
  ExistingPublisherTestJSON, ExistingPublisherTest,
  testing::Values(std::make_pair("json", std::vector<uint8_t>(HELLO_WORLD_JSON,
                                                              std::end(HELLO_WORLD_JSON)))));
TEST_F(ParameterTest, testGetAllParams) {
  const std::string requestId = "req-testGetAllParams";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->getParameters({}, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_GE(params.size(), 2UL);
}

TEST_F(ParameterTest, testGetNonExistingParameters) {
  const std::string requestId = "req-testGetNonExistingParameters";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->getParameters(
    {"/foo_1.non_existing_parameter", "/foo_2.non_existing.nested_parameter"}, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_TRUE(params.empty());
}

TEST_F(ParameterTest, testGetIntParameter) {
  const std::string requestId = "req-testGetIntParameter";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->getParameters({NODE_2_NAME + "." + PARAM_5_NAME}, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_EQ(1UL, params.size());
  EXPECT_EQ(PARAM_5_DEFAULT_VALUE, params.front().get<PARAM_5_TYPE>());
}

TEST_F(ParameterTest, testGetParameters) {
  const auto p1 = NODE_1_NAME + "." + PARAM_1_NAME;
  const auto p2 = NODE_2_NAME + "." + PARAM_2_NAME;

  const std::string requestId = "req-testGetParameters";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->getParameters({p1, p2}, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_EQ(2UL, params.size());
  auto p1Iter = std::find_if(params.begin(), params.end(), [&p1](const auto& param) {
    return param.name() == p1;
  });
  auto p2Iter = std::find_if(params.begin(), params.end(), [&p2](const auto& param) {
    return param.name() == p2;
  });
  ASSERT_NE(p1Iter, params.end());
  EXPECT_EQ(PARAM_1_DEFAULT_VALUE, p1Iter->get<PARAM_1_TYPE>());
  ASSERT_NE(p2Iter, params.end());

  std::vector<int64_t> int_array_val;
  const auto array_params = p2Iter->get<std::vector<foxglove::ParameterValueView>>();
  for (const auto& paramValue : array_params) {
    int_array_val.push_back(paramValue.get<int64_t>());
  }
  EXPECT_EQ(int_array_val, PARAM_2_DEFAULT_VALUE);
}

TEST_F(ParameterTest, testSetParameters) {
  const auto p1 = NODE_1_NAME + "." + PARAM_1_NAME;
  const auto p2 = NODE_2_NAME + "." + PARAM_2_NAME;
  const PARAM_1_TYPE newP1value = "world";

  // Create ParameterValue array properly
  std::vector<foxglove::ParameterValue> paramValueVec;
  paramValueVec.emplace_back(foxglove::ParameterValue(4L));
  paramValueVec.emplace_back(foxglove::ParameterValue(5L));
  paramValueVec.emplace_back(foxglove::ParameterValue(6L));
  foxglove::ParameterValue newP2value(std::move(paramValueVec));

  std::vector<foxglove::Parameter> parameters;
  parameters.emplace_back(foxglove::Parameter(p1, newP1value));
  parameters.emplace_back(
    foxglove::Parameter(p2, foxglove::ParameterType::None, std::move(newP2value)));

  _wsClient->setParameters(parameters);
  const std::string requestId = "req-testSetParameters";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->getParameters({p1, p2}, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_EQ(2UL, params.size());
  auto p1Iter = std::find_if(params.begin(), params.end(), [&p1](const auto& param) {
    return param.name() == p1;
  });
  auto p2Iter = std::find_if(params.begin(), params.end(), [&p2](const auto& param) {
    return param.name() == p2;
  });
  ASSERT_NE(p1Iter, params.end());
  EXPECT_EQ(newP1value, p1Iter->get<PARAM_1_TYPE>());
  ASSERT_NE(p2Iter, params.end());

  std::vector<int64_t> int_array_val;
  const auto array_params = p2Iter->get<std::vector<foxglove::ParameterValueView>>();
  for (const auto& paramValue : array_params) {
    int_array_val.push_back(paramValue.get<int64_t>());
  }
  const std::vector<int64_t> expected_value = {4, 5, 6};
  EXPECT_EQ(int_array_val, expected_value);
}

TEST_F(ParameterTest, testSetParametersWithReqId) {
  const auto p1 = NODE_1_NAME + "." + PARAM_1_NAME;
  const PARAM_1_TYPE newP1value = "world";
  std::vector<foxglove::Parameter> parameters;
  parameters.emplace_back(foxglove::Parameter(p1, newP1value));

  const std::string requestId = "req-testSetParameters";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->setParameters(parameters, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_EQ(1UL, params.size());
}

TEST_F(ParameterTest, testSetFloatParametersWithIntegers) {
  const auto floatParamName = NODE_2_NAME + "." + PARAM_3_NAME;
  const auto floatArrayParamName = NODE_2_NAME + "." + PARAM_4_NAME;
  const int64_t floatParamVal = 10;
  const std::vector<int64_t> floatArrayParamVal = {3, 2, 1};
  const std::string requestId = "req-testSetFloatParametersWithIntegers";
  auto future = _wsClient->waitForParameters(requestId);
  const nlohmann::json::array_t parameters = {
    {{"name", floatParamName}, {"value", floatParamVal}, {"type", "float64"}},
    {{"name", floatArrayParamName}, {"value", floatArrayParamVal}, {"type", "float64_array"}},
  };
  _wsClient->sendText(
    nlohmann::json{{"op", "setParameters"}, {"id", requestId}, {"parameters", parameters}}.dump());
  ASSERT_EQ(std::future_status::ready, future.wait_for(ONE_SECOND));
  std::vector<foxglove::Parameter> params = future.get();

  {
    const auto param =
      std::find_if(params.begin(), params.end(), [floatParamName](const foxglove::Parameter& p) {
        return p.name() == floatParamName;
      });
    ASSERT_NE(param, params.end());
    EXPECT_EQ(param->type(), foxglove::ParameterType::Float64);
    EXPECT_NEAR(param->get<double>(), static_cast<double>(floatParamVal), 1e-9);
  }
  {
    const auto param = std::find_if(params.begin(), params.end(),
                                    [floatArrayParamName](const foxglove::Parameter& p) {
                                      return p.name() == floatArrayParamName;
                                    });
    ASSERT_NE(param, params.end());
    EXPECT_EQ(param->type(), foxglove::ParameterType::Float64Array);
    const auto paramValue = param->get<std::vector<foxglove::ParameterValueView>>();
    ASSERT_EQ(paramValue.size(), floatArrayParamVal.size());
    for (size_t i = 0; i < paramValue.size(); ++i) {
      EXPECT_NEAR(paramValue[i].get<double>(), static_cast<double>(floatArrayParamVal[i]), 1e-9);
    }
  }
}

TEST_F(ParameterTest, testUnsetParameter) {
  const auto p1 = NODE_1_NAME + "." + DELETABLE_PARAM_NAME;
  std::vector<foxglove::Parameter> parameters;
  parameters.emplace_back(foxglove::Parameter(p1));

  const std::string requestId = "req-testUnsetParameter";
  auto future = _wsClient->waitForParameters(requestId);
  _wsClient->setParameters(parameters, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  EXPECT_EQ(0UL, params.size());
}

TEST_F(ParameterTest, testParameterSubscription) {
  const auto p1 = NODE_1_NAME + "." + PARAM_1_NAME;

  _wsClient->subscribeParameterUpdates({p1});
  auto future = _wsClient->waitForParameters();
  std::vector<foxglove::Parameter> setParams;
  setParams.emplace_back(foxglove::Parameter(p1, "foo"));
  _wsClient->setParameters(setParams);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  std::vector<foxglove::Parameter> params = future.get();

  ASSERT_EQ(1UL, params.size());
  EXPECT_EQ(params.front().name(), p1);

  _wsClient->unsubscribeParameterUpdates({p1});
  std::vector<foxglove::Parameter> setParams2;
  setParams2.emplace_back(foxglove::Parameter(p1, "bar"));
  _wsClient->setParameters(setParams2);

  future = _wsClient->waitForParameters();
  ASSERT_EQ(std::future_status::timeout, future.wait_for(ONE_SECOND));
}

TEST_F(ParameterTest, testGetParametersParallel) {
  // Connect a few clients (in parallel) and make sure that they all receive parameters
  auto clients = {
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
  };

  std::vector<std::future<std::vector<foxglove::Parameter>>> futures;
  for (auto client : clients) {
    futures.push_back(
      std::async(std::launch::async, [client]() -> std::vector<foxglove::Parameter> {
        if (std::future_status::ready == client->connect(URI).wait_for(DEFAULT_TIMEOUT)) {
          const std::string requestId = "req-123";
          auto future = client->waitForParameters(requestId);
          client->getParameters({}, requestId);
          future.wait_for(DEFAULT_TIMEOUT);
          if (future.valid()) {
            return future.get();
          }
        }
        return {};
      }));
  }

  for (auto& future : futures) {
    ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
    std::vector<foxglove::Parameter> parameters;
    EXPECT_NO_THROW(parameters = future.get());
    EXPECT_GE(parameters.size(), 2UL);
  }
}

TEST_F(ServiceTest, testAdvertiseService) {
  auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  auto serviceFuture = client->waitForService(SERVICE_NAME);
  ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
  ASSERT_EQ(std::future_status::ready, serviceFuture.wait_for(DEFAULT_TIMEOUT));
  const foxglove::test::Service service = serviceFuture.get();

  EXPECT_EQ(service.name, SERVICE_NAME);
  EXPECT_EQ(service.type, "std_srvs/srv/SetBool");
  EXPECT_EQ(service.requestSchema, "bool data # e.g. for hardware enabling / disabling");
  EXPECT_EQ(service.responseSchema,
            "bool success   # indicate successful run of triggered service\nstring message # "
            "informational, e.g. for error messages");
}

TEST_F(ServiceTest, testCallServiceParallel) {
  // Connect a few clients (in parallel) and make sure that they can all call the service
  auto clients = {
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
    std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>(),
  };

  auto clientIt = clients.begin();
  auto serviceFuture = (*clientIt)->waitForService(SERVICE_NAME);
  for (auto client : clients) {
    ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
  }
  ASSERT_EQ(std::future_status::ready, serviceFuture.wait_for(DEFAULT_TIMEOUT));
  const foxglove::test::Service service = serviceFuture.get();

  std_srvs::srv::SetBool::Request requestMsg;
  requestMsg.data = true;
  const auto serializedRequest = serializeMsg(&requestMsg);
  const auto& serRequestMsg = serializedRequest->get_rcl_serialized_message();

  foxglove::test::ServiceRequest request;
  request.serviceId = service.id;
  request.callId = 123lu;
  request.encoding = "cdr";
  request.data.resize(serRequestMsg.buffer_length);
  std::memcpy(request.data.data(), serRequestMsg.buffer, serRequestMsg.buffer_length);

  std::vector<std::future<foxglove::test::ServiceResponse>> futures;
  for (auto client : clients) {
    futures.push_back(client->waitForServiceResponse());
    client->sendServiceRequest(request);
  }

  for (auto& future : futures) {
    ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
    foxglove::test::ServiceResponse response;
    EXPECT_NO_THROW(response = future.get());
    EXPECT_EQ(response.serviceId, request.serviceId);
    EXPECT_EQ(response.callId, request.callId);
    EXPECT_EQ(response.encoding, request.encoding);

    rclcpp::SerializedMessage serializedResponseMsg(response.data.size());
    auto& serMsg = serializedResponseMsg.get_rcl_serialized_message();
    std::memcpy(serMsg.buffer, response.data.data(), response.data.size());
    serMsg.buffer_length = response.data.size();
    const auto resMsg = deserializeMsg<std_srvs::srv::SetBool::Response>(&serMsg);

    EXPECT_EQ(resMsg->message, "hello");
    EXPECT_EQ(resMsg->success, requestMsg.data);
  }
}

TEST_F(ServiceTest, testCallNonexistentService) {
  auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(std::chrono::seconds(5)));

  std::promise<nlohmann::json> promise;
  auto serviceFailureFuture = promise.get_future();
  client->setTextMessageHandler([&promise](const std::string& payload) mutable {
    const auto msg = nlohmann::json::parse(payload);
    if (msg["op"].get<std::string>() == "serviceCallFailure") {
      promise.set_value(msg);
    }
  });

  foxglove::test::ServiceRequest request;
  request.serviceId = 99u;
  request.callId = 123u;
  request.encoding = "";
  request.data = {};
  client->sendServiceRequest(request);

  ASSERT_EQ(std::future_status::ready, serviceFailureFuture.wait_for(std::chrono::seconds(5)));
  const auto failureMsg = serviceFailureFuture.get();
  EXPECT_EQ(failureMsg["serviceId"].get<foxglove::test::ServiceId>(), request.serviceId);
  EXPECT_EQ(failureMsg["callId"].get<uint32_t>(), request.callId);
}

TEST(SmokeTest, receiveMessagesOfMultipleTransientLocalPublishers) {
  const std::string topicName = "/latched";
  auto node = rclcpp::Node::make_shared("node");
  rclcpp::QoS qos = rclcpp::QoS(rclcpp::KeepLast(1));
  qos.transient_local();
  qos.reliable();

  rclcpp::executors::SingleThreadedExecutor executor;
  executor.add_node(node);
  auto spinnerThread = std::thread([&executor]() {
    executor.spin();
  });

  constexpr size_t nPubs = 15;
  std::vector<rclcpp::Publisher<std_msgs::msg::String>::SharedPtr> pubs;
  for (size_t i = 0; i < nPubs; ++i) {
    auto pub = pubs.emplace_back(node->create_publisher<std_msgs::msg::String>(topicName, qos));
    std_msgs::msg::String msg;
    msg.data = "Hello";
    pub->publish(msg);
  }

  // Set up a client and subscribe to the channel.
  auto client = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  auto channelFuture = client->waitForChannel(topicName);
  ASSERT_EQ(std::future_status::ready, client->connect(URI).wait_for(ONE_SECOND));
  ASSERT_EQ(std::future_status::ready, channelFuture.wait_for(ONE_SECOND));
  const foxglove::test::Channel channel = channelFuture.get();
  const foxglove::test::SubscriptionId subscriptionId = 1;

  // Set up binary message handler to resolve the promise when all nPub message have been received
  std::promise<void> promise;
  std::atomic<size_t> nReceivedMessages = 0;
  client->setBinaryMessageHandler([&promise, &nReceivedMessages](const uint8_t*, size_t) {
    if (++nReceivedMessages == nPubs) {
      promise.set_value();
    }
  });

  // Subscribe to the channel and confirm that the promise resolves
  client->subscribe({{subscriptionId, channel.id}});
  EXPECT_EQ(std::future_status::ready, promise.get_future().wait_for(DEFAULT_TIMEOUT));
  EXPECT_EQ(nReceivedMessages, nPubs);
  client->unsubscribe({subscriptionId});

  pubs.clear();
  executor.remove_node(node);
  executor.cancel();
  spinnerThread.join();
}

TEST(FetchAssetTest, fetchExistingAsset) {
  auto wsClient = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  EXPECT_EQ(std::future_status::ready, wsClient->connect(URI).wait_for(DEFAULT_TIMEOUT));

  const auto millisSinceEpoch = std::chrono::duration_cast<std::chrono::milliseconds>(
    std::chrono::system_clock::now().time_since_epoch());
  const auto tmpFilePath =
    std::filesystem::temp_directory_path() / std::to_string(millisSinceEpoch.count());
  constexpr char content[] = "Hello, world";
  FILE* tmpAssetFile = std::fopen(tmpFilePath.c_str(), "w");
  std::fputs(content, tmpAssetFile);
  std::fclose(tmpAssetFile);

  const std::string uri = std::string("file://") + tmpFilePath.string();
  const uint32_t requestId = 123;

  auto future = wsClient->waitForFetchAssetResponse();
  wsClient->fetchAsset(uri, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  const foxglove::test::FetchAssetResponse response = future.get();

  EXPECT_EQ(response.requestId, requestId);
  EXPECT_EQ(response.status, foxglove::test::FetchAssetStatus::Success);
  // +1 since NULL terminator is not written to file.
  ASSERT_EQ(response.data.size() + 1ul, sizeof(content));
  EXPECT_EQ(0, std::memcmp(content, response.data.data(), response.data.size()));
  std::remove(tmpFilePath.c_str());
}

TEST(FetchAssetTest, fetchNonExistingAsset) {
  auto wsClient = std::make_shared<foxglove::test::Client<websocketpp::config::asio_client>>();
  EXPECT_EQ(std::future_status::ready, wsClient->connect(URI).wait_for(DEFAULT_TIMEOUT));

  const std::string assetId = "file:///foo/bar";
  const uint32_t requestId = 456;

  auto future = wsClient->waitForFetchAssetResponse();
  wsClient->fetchAsset(assetId, requestId);
  ASSERT_EQ(std::future_status::ready, future.wait_for(DEFAULT_TIMEOUT));
  const foxglove::test::FetchAssetResponse response = future.get();

  EXPECT_EQ(response.requestId, requestId);
  EXPECT_EQ(response.status, foxglove::test::FetchAssetStatus::Error);
  EXPECT_FALSE(response.errorMessage.empty());
}

// Run all the tests that were declared with TEST()
int main(int argc, char** argv) {
  testing::InitGoogleTest(&argc, argv);
  rclcpp::init(argc, argv);

  rclcpp::executors::SingleThreadedExecutor executor;
  rclcpp::NodeOptions nodeOptions;
  // Explicitly allow file:// asset URIs for testing purposes.
  nodeOptions.append_parameter_override("asset_uri_allowlist",
                                        std::vector<std::string>({"file://.*"}));
  foxglove_bridge::FoxgloveBridge node(nodeOptions);
  executor.add_node(node.get_node_base_interface());

  std::thread spinnerThread([&executor]() {
    executor.spin();
  });

  const auto testResult = RUN_ALL_TESTS();

  executor.cancel();
  spinnerThread.join();
  executor.remove_node(node.get_node_base_interface());

  return testResult;
}
