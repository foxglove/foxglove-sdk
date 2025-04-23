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
  std::string_view topic, const std::vector<std::string>& publisherIds
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
  std::string_view topic, const std::vector<std::string>& subscriberIds
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
  std::string_view service, const std::vector<std::string>& providerIds
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
