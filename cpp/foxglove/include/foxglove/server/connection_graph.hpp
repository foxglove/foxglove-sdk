#pragma once

#include <foxglove/error.hpp>

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

struct foxglove_connection_graph;

/// The foxglove namespace.
namespace foxglove {

/// @brief A connection graph describing a topology of subscribers, publishers, topics, and
/// services.
///
/// Connection graph data can be published with WebSocketServer::publishConnectionGraph, and
/// visualized in the Foxglove [Topic Graph panel].
///
/// [Topic Graph panel]: https://docs.foxglove.dev/docs/visualization/panels/topic-graph
class ConnectionGraph final {
  friend class WebSocketServer;

public:
  ConnectionGraph();

  /// @brief Set a published topic and its associated publisher ids.
  ///
  /// Overwrites any existing topic with the same name.
  FoxgloveError setPublishedTopic(
    std::string_view topic, const std::vector<std::string>& publisher_ids
  );
  /// @brief Set a subscribed topic and its associated subscriber ids.
  ///
  /// Overwrites any existing topic with the same name.
  FoxgloveError setSubscribedTopic(
    std::string_view topic, const std::vector<std::string>& subscriber_ids
  );
  /// @brief Set an advertised service and its associated provider ids.
  ///
  /// Overwrites any existing service with the same name.
  FoxgloveError setAdvertisedService(
    std::string_view service, const std::vector<std::string>& provider_ids
  );

private:
  std::unique_ptr<foxglove_connection_graph, void (*)(foxglove_connection_graph*)> impl_;
};

}  // namespace foxglove
