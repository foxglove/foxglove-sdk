#include <foxglove-c/foxglove-c.h>
#include <foxglove/server/connection_graph.hpp>

namespace foxglove {

ConnectionGraph::ConnectionGraph()
    : _impl(nullptr, foxglove_connection_graph_free) {
  foxglove_connection_graph* impl = nullptr;
  foxglove_connection_graph_create(&impl);
  _impl = std::unique_ptr<foxglove_connection_graph, void (*)(foxglove_connection_graph*)>(
    impl, foxglove_connection_graph_free
  );
}

FoxgloveError ConnectionGraph::setPublishedTopic(
  std::string_view topic, const std::vector<std::string>& publisher_ids
) {
  std::vector<foxglove_string> ids;
  ids.reserve(publisher_ids.size());
  for (const auto& id : publisher_ids) {
    ids.push_back({id.c_str(), id.length()});
  }
  auto err = foxglove_connection_graph_set_published_topic(
    _impl.get(), {topic.data(), topic.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

FoxgloveError ConnectionGraph::setSubscribedTopic(
  std::string_view topic, const std::vector<std::string>& subscriber_ids
) {
  std::vector<foxglove_string> ids;
  ids.reserve(subscriber_ids.size());
  for (const auto& id : subscriber_ids) {
    ids.push_back({id.c_str(), id.length()});
  }

  auto err = foxglove_connection_graph_set_subscribed_topic(
    _impl.get(), {topic.data(), topic.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

FoxgloveError ConnectionGraph::setAdvertisedService(
  std::string_view service, const std::vector<std::string>& provider_ids
) {
  std::vector<foxglove_string> ids;
  ids.reserve(provider_ids.size());
  for (const auto& id : provider_ids) {
    ids.push_back({id.c_str(), id.length()});
  }

  auto err = foxglove_connection_graph_set_advertised_service(
    _impl.get(), {service.data(), service.length()}, ids.data(), ids.size()
  );
  return FoxgloveError(err);
}

}  // namespace foxglove
