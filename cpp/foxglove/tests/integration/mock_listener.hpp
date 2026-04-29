#pragma once

#include <foxglove/remote_access.hpp>

#include <atomic>
#include <cstdint>
#include <mutex>
#include <string>
#include <tuple>
#include <vector>

namespace foxglove_integration {

/// Records gateway callbacks for test assertions.
/// All methods are thread-safe.
struct MockListener {
  mutable std::mutex mutex;

  std::vector<std::pair<uint32_t, std::string>> subscribed;
  std::vector<std::pair<uint32_t, std::string>> unsubscribed;
  std::vector<std::pair<uint32_t, std::string>> client_advertised;
  std::vector<std::pair<uint32_t, std::string>> client_unadvertised;
  std::vector<std::tuple<uint32_t, std::string, std::vector<std::byte>>> message_data;
  std::atomic<uint32_t> connection_graph_subscribed{0};
  std::atomic<uint32_t> connection_graph_unsubscribed{0};

  foxglove::RemoteAccessGatewayCallbacks make_callbacks() {
    foxglove::RemoteAccessGatewayCallbacks cb;
    cb.onSubscribe = [this](uint32_t client_id, const foxglove::ChannelDescriptor& channel) {
      std::lock_guard<std::mutex> lock(mutex);
      subscribed.emplace_back(client_id, std::string(channel.topic()));
    };
    cb.onUnsubscribe = [this](uint32_t client_id, const foxglove::ChannelDescriptor& channel) {
      std::lock_guard<std::mutex> lock(mutex);
      unsubscribed.emplace_back(client_id, std::string(channel.topic()));
    };
    cb.onClientAdvertise = [this](uint32_t client_id, const foxglove::ChannelDescriptor& channel) {
      std::lock_guard<std::mutex> lock(mutex);
      client_advertised.emplace_back(client_id, std::string(channel.topic()));
    };
    cb.onClientUnadvertise =
      [this](uint32_t client_id, const foxglove::ChannelDescriptor& channel) {
        std::lock_guard<std::mutex> lock(mutex);
        client_unadvertised.emplace_back(client_id, std::string(channel.topic()));
      };
    cb.onMessageData = [this](
                         uint32_t client_id,
                         const foxglove::ChannelDescriptor& channel,
                         const std::byte* data,
                         size_t data_len
                       ) {
      std::lock_guard<std::mutex> lock(mutex);
      message_data.emplace_back(
        client_id, std::string(channel.topic()), std::vector<std::byte>(data, data + data_len)
      );
    };
    cb.onConnectionGraphSubscribe = [this]() {
      connection_graph_subscribed.fetch_add(1, std::memory_order_relaxed);
    };
    cb.onConnectionGraphUnsubscribe = [this]() {
      connection_graph_unsubscribed.fetch_add(1, std::memory_order_relaxed);
    };
    return cb;
  }

  size_t subscribed_count() const {
    std::lock_guard<std::mutex> lock(mutex);
    return subscribed.size();
  }

  size_t unsubscribed_count() const {
    std::lock_guard<std::mutex> lock(mutex);
    return unsubscribed.size();
  }

  size_t client_advertised_count() const {
    std::lock_guard<std::mutex> lock(mutex);
    return client_advertised.size();
  }

  size_t client_unadvertised_count() const {
    std::lock_guard<std::mutex> lock(mutex);
    return client_unadvertised.size();
  }

  size_t message_data_count() const {
    std::lock_guard<std::mutex> lock(mutex);
    return message_data.size();
  }
};

}  // namespace foxglove_integration
