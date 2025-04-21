#include <foxglove-c/foxglove-c.h>
#include <foxglove/error.hpp>
#include <foxglove/server.hpp>

#include <iostream>
#include <type_traits>

namespace foxglove {

FoxgloveResult<WebSocketServer> WebSocketServer::create(
  WebSocketServerOptions&& options  // NOLINT(cppcoreguidelines-rvalue-reference-param-not-moved)
) {
  foxglove_internal_register_cpp_wrapper();

  bool hasAnyCallbacks = options.callbacks.onSubscribe || options.callbacks.onUnsubscribe ||
                         options.callbacks.onClientAdvertise || options.callbacks.onMessageData ||
                         options.callbacks.onClientUnadvertise ||
                         options.callbacks.onConnectionGraphSubscribe ||
                         options.callbacks.onConnectionGraphUnsubscribe;

  std::unique_ptr<WebSocketServerCallbacks> callbacks;

  foxglove_server_callbacks cCallbacks = {};

  if (hasAnyCallbacks) {
    callbacks = std::make_unique<WebSocketServerCallbacks>(std::move(options.callbacks));
    cCallbacks.context = callbacks.get();
    if (callbacks->onSubscribe) {
      cCallbacks.on_subscribe = [](uint64_t channel_id, const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onSubscribe(channel_id);
      };
    }
    if (callbacks->onUnsubscribe) {
      cCallbacks.on_unsubscribe = [](uint64_t channel_id, const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onUnsubscribe(channel_id);
      };
    }
    if (callbacks->onClientAdvertise) {
      cCallbacks.on_client_advertise =
        [](uint32_t client_id, const foxglove_client_channel* channel, const void* context) {
          ClientChannel cppChannel = {
            channel->id,
            channel->topic,
            channel->encoding,
            channel->schema_name,
            channel->schema_encoding == nullptr ? std::string_view{} : channel->schema_encoding,
            reinterpret_cast<const std::byte*>(channel->schema),
            channel->schema_len
          };
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onClientAdvertise(client_id, cppChannel);
        };
    }
    if (callbacks->onMessageData) {
      cCallbacks.on_message_data = [](
                                     // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
                                     uint32_t client_id,
                                     uint32_t client_channel_id,
                                     const uint8_t* payload,
                                     size_t payload_len,
                                     const void* context
                                   ) {
        (static_cast<const WebSocketServerCallbacks*>(context))
          ->onMessageData(
            client_id, client_channel_id, reinterpret_cast<const std::byte*>(payload), payload_len
          );
      };
    }
    if (callbacks->onClientUnadvertise) {
      cCallbacks.on_client_unadvertise =
        // NOLINTNEXTLINE(bugprone-easily-swappable-parameters)
        [](uint32_t client_id, uint32_t client_channel_id, const void* context) {
          (static_cast<const WebSocketServerCallbacks*>(context))
            ->onClientUnadvertise(client_id, client_channel_id);
        };
    }
    if (callbacks->onConnectionGraphSubscribe) {
      cCallbacks.on_connection_graph_subscribe = [](const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphSubscribe();
      };
    }
    if (callbacks->onConnectionGraphUnsubscribe) {
      cCallbacks.on_connection_graph_unsubscribe = [](const void* context) {
        (static_cast<const WebSocketServerCallbacks*>(context))->onConnectionGraphUnsubscribe();
      };
    }
  }

  foxglove_server_options cOptions = {};
  cOptions.name = {options.name.c_str(), options.name.length()};
  cOptions.host = {options.host.c_str(), options.host.length()};
  cOptions.port = options.port;
  cOptions.callbacks = hasAnyCallbacks ? &cCallbacks : nullptr;
  cOptions.capabilities =
    static_cast<std::underlying_type_t<decltype(options.capabilities)>>(options.capabilities);
  std::vector<foxglove_string> supportedEncodings;
  supportedEncodings.reserve(options.supportedEncodings.size());
  for (const auto& encoding : options.supportedEncodings) {
    supportedEncodings.push_back({encoding.c_str(), encoding.length()});
  }
  cOptions.supported_encodings = supportedEncodings.data();
  cOptions.supported_encodings_count = supportedEncodings.size();

  foxglove_websocket_server* server = nullptr;
  foxglove_error error = foxglove_server_start(&cOptions, &server);
  if (error != foxglove_error::FOXGLOVE_ERROR_OK || server == nullptr) {
    return foxglove::unexpected(static_cast<FoxgloveError>(error));
  }

  return WebSocketServer(server, std::move(callbacks));
}

WebSocketServer::WebSocketServer(
  foxglove_websocket_server* server, std::unique_ptr<WebSocketServerCallbacks> callbacks
)
    : _impl(server, foxglove_server_stop)
    , _callbacks(std::move(callbacks)) {}

FoxgloveError WebSocketServer::stop() {
  foxglove_error error = foxglove_server_stop(_impl.release());
  return FoxgloveError(error);
}

uint16_t WebSocketServer::port() const {
  return foxglove_server_get_port(_impl.get());
}

void WebSocketServer::publishConnectionGraph(ConnectionGraph& graph) {
  foxglove_server_publish_connection_graph(_impl.get(), &graph.impl());
}

ConnectionGraph::ConnectionGraph()
    : _impl(nullptr, foxglove_connection_graph_free) {
  std::cerr << "Creating ConnectionGraph" << std::endl;
  foxglove_connection_graph* impl = nullptr;
  foxglove_connection_graph_create(&impl);
  _impl = std::unique_ptr<foxglove_connection_graph, void (*)(foxglove_connection_graph*)>(
    impl, foxglove_connection_graph_free
  );
}

foxglove_connection_graph& ConnectionGraph::impl() {
  return *_impl.get();
}

FoxgloveError ConnectionGraph::setPublishedTopic(
  std::string_view topic, std::vector<std::string> publisherIds
) {
  std::vector<foxglove_string> ids;
  ids.reserve(publisherIds.size());
  for (const auto& id : publisherIds) {
    ids.push_back({id.c_str(), id.length()});
  }
  auto err = foxglove_connection_graph_set_published_topic(
    _impl.get(), {topic.data(), topic.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

FoxgloveError ConnectionGraph::setSubscribedTopic(
  std::string_view topic, std::vector<std::string> subscriberIds
) {
  std::vector<foxglove_string> ids;
  ids.reserve(subscriberIds.size());
  for (const auto& id : subscriberIds) {
    ids.push_back({id.c_str(), id.length()});
  }

  auto err = foxglove_connection_graph_set_subscribed_topic(
    _impl.get(), {topic.data(), topic.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

FoxgloveError ConnectionGraph::setAdvertisedService(
  std::string_view service, std::vector<std::string> providerIds
) {
  std::vector<foxglove_string> ids;
  ids.reserve(providerIds.size());
  for (const auto& id : providerIds) {
    ids.push_back({id.c_str(), id.length()});
  }

  auto err = foxglove_connection_graph_set_advertised_service(
    _impl.get(), {service.data(), service.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

}  // namespace foxglove
