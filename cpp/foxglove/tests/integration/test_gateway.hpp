#pragma once

#include "mock_listener.hpp"
#include "mock_server.hpp"
#include "test_helpers.hpp"

#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#include <memory>
#include <optional>
#include <string>

namespace foxglove_integration {

/// Options for starting a TestGateway.
struct TestGatewayOptions {
  foxglove::RemoteAccessGatewayCallbacks callbacks;
  foxglove::RemoteAccessGatewayCapabilities capabilities =
    foxglove::RemoteAccessGatewayCapabilities::None;
  foxglove::SinkChannelFilterFn channel_filter;
  std::vector<std::string> supported_encodings = {"json"};
};

/// A test gateway backed by a mock Foxglove API server.
class TestGateway {
public:
  std::string room_name;

  /// Starts a gateway with default options.
  static TestGateway start(const foxglove::Context& ctx) {
    return start_with_options(ctx, {});
  }

  /// Starts a gateway with the given options.
  static TestGateway start_with_options(
    const foxglove::Context& ctx, TestGatewayOptions opts
  ) {
    auto room_name = "test-room-" + unique_id();
    auto mock = start_mock_server(room_name);

    foxglove::RemoteAccessGatewayOptions gw_opts;
    gw_opts.context = ctx;
    gw_opts.name = "test-device-" + unique_id();
    gw_opts.device_token = TEST_DEVICE_TOKEN;
    gw_opts.foxglove_api_url = mock.url();
    gw_opts.supported_encodings = std::move(opts.supported_encodings);
    gw_opts.callbacks = std::move(opts.callbacks);
    gw_opts.capabilities = opts.capabilities;
    gw_opts.sink_channel_filter = std::move(opts.channel_filter);

    auto result = foxglove::RemoteAccessGateway::create(std::move(gw_opts));
    if (!result.has_value()) {
      throw std::runtime_error(
        std::string("Failed to create gateway: ") + foxglove::strerror(result.error())
      );
    }

    TestGateway gw;
    gw.room_name = std::move(room_name);
    gw.mock_.emplace(std::move(mock));
    gw.gateway_ = std::make_unique<foxglove::RemoteAccessGateway>(std::move(result.value()));
    return gw;
  }

  foxglove::RemoteAccessGateway& gateway() {
    return *gateway_;
  }

  foxglove::RemoteAccessConnectionStatus connection_status() const {
    return gateway_->connectionStatus();
  }

  void stop() {
    if (gateway_) {
      gateway_->stop();
    }
  }

private:
  TestGateway() = default;

  std::optional<MockServerHandle> mock_;
  std::unique_ptr<foxglove::RemoteAccessGateway> gateway_;
};

}  // namespace foxglove_integration
