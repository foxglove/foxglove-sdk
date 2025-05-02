#pragma once

#include <foxglove/error.hpp>

#include <cstdint>
#include <memory>
#include <string>

struct foxglove_connection_graph;

namespace foxglove {

class ConnectionGraph final {
  friend class WebSocketServer;

public:
  ConnectionGraph();

  FoxgloveError setPublishedTopic(
    std::string_view topic, const std::vector<std::string>& publisher_ids
  );
  FoxgloveError setSubscribedTopic(
    std::string_view topic, const std::vector<std::string>& subscriber_ids
  );
  FoxgloveError setAdvertisedService(
    std::string_view service, const std::vector<std::string>& provider_ids
  );

private:
  std::unique_ptr<foxglove_connection_graph, void (*)(foxglove_connection_graph*)> impl_;
};

}  // namespace foxglove
