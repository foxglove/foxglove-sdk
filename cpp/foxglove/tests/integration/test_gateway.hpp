#pragma once

#include <foxglove/foxglove.hpp>
#include <foxglove/remote_access.hpp>

#include <atomic>
#include <memory>
#include <optional>
#include <string>
#include <utility>

#include "mock_listener.hpp"
#include "mock_server.hpp"
#include "test_helpers.hpp"

namespace foxglove_integration {

/// Options for starting a TestGateway.
struct TestGatewayOptions {
  foxglove::RemoteAccessGatewayCallbacks callbacks;
  foxglove::RemoteAccessGatewayCapabilities capabilities =
    foxglove::RemoteAccessGatewayCapabilities::None;
  foxglove::SinkChannelFilterFn channel_filter;
  foxglove::QosClassifierFn qos_classifier;
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

  /// Starts a gateway with the given options and waits for it to reach the
  /// `Connected` state before returning.
  ///
  /// Without this wait, callers that immediately connect a viewer race the
  /// gateway's own WebRTC handshake: the viewer joins LiveKit, LiveKit
  /// notifies the gateway, and the gateway's `stream_bytes` call hangs
  /// waiting for its SUBSCRIBER DTLS connection to come up. That manifests
  /// as "timeout waiting for gateway to open byte stream" in tests.
  static TestGateway start_with_options(const foxglove::Context& ctx, TestGatewayOptions opts) {
    auto room_name = "test-room-" + unique_id();
    auto mock = start_mock_server(room_name);

    auto connected = std::make_shared<std::atomic<bool>>(false);
    auto user_status_cb = std::move(opts.callbacks.onConnectionStatusChanged);
    opts.callbacks.onConnectionStatusChanged =
      [connected, user_status_cb](foxglove::RemoteAccessConnectionStatus status) {
        if (status == foxglove::RemoteAccessConnectionStatus::Connected) {
          connected->store(true);
        }
        if (user_status_cb) {
          user_status_cb(status);
        }
      };

    foxglove::RemoteAccessGatewayOptions gw_opts;
    gw_opts.context = ctx;
    gw_opts.name = "test-device-" + unique_id();
    gw_opts.device_token = TEST_DEVICE_TOKEN;
    gw_opts.foxglove_api_url = mock.url();
    gw_opts.supported_encodings = std::move(opts.supported_encodings);
    gw_opts.callbacks = std::move(opts.callbacks);
    gw_opts.capabilities = opts.capabilities;
    gw_opts.sink_channel_filter = std::move(opts.channel_filter);
    gw_opts.qos_classifier = std::move(opts.qos_classifier);

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

    poll_until([&] {
      return connected->load() ||
             gw.gateway_->connectionStatus() == foxglove::RemoteAccessConnectionStatus::Connected;
    });

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
