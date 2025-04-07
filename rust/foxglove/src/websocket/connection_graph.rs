use crate::websocket::protocol::server::{
    ConnectionGraphDiff, NewAdvertisedService, NewPublishedTopic, NewSubscribedTopic,
};
use std::collections::{HashMap, HashSet};

use super::ClientId;

/// A HashMap where the keys are the topic or service name and the value is a set of string ids.
type MapOfSets = HashMap<String, HashSet<String>>;

/// The connection graph data. Requires capability [`ConnectionGraph`](super::Capability::ConnectionGraph).
/// See <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update>
#[derive(Debug, Default, Clone)]
pub struct ConnectionGraph {
    /// A map of active topic names to the set of string publisher ids.
    published_topics: MapOfSets,
    /// A map of active topic names to the set of string subscriber ids.
    subscribed_topics: MapOfSets,
    /// A map of active service names to the set of string provider ids.
    advertised_services: MapOfSets,
    /// A set of subscribers.
    subscribers: HashSet<ClientId>,
}

impl ConnectionGraph {
    /// Create a new, empty connection graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a published topic and its associated publisher ids.
    /// Overwrites any existing topic with the same name.
    pub fn set_published_topic(
        &mut self,
        topic: impl Into<String>,
        publisher_ids: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.published_topics.insert(
            topic.into(),
            HashSet::from_iter(publisher_ids.into_iter().map(|id| id.into())),
        );
    }

    /// Set a subscribed topic and its associated subscriber ids.
    /// Overwrites any existing topic with the same name.
    pub fn set_subscribed_topic(
        &mut self,
        topic: impl Into<String>,
        subscriber_ids: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.subscribed_topics.insert(
            topic.into(),
            HashSet::from_iter(subscriber_ids.into_iter().map(|id| id.into())),
        );
    }

    /// Set an advertised service and its associated provider ids.
    /// Overwrites any existing service with the same name.
    pub fn set_advertised_service(
        &mut self,
        service: impl Into<String>,
        provider_ids: impl IntoIterator<Item = impl Into<String>>,
    ) {
        self.advertised_services.insert(
            service.into(),
            HashSet::from_iter(provider_ids.into_iter().map(|id| id.into())),
        );
    }

    /// Adds a connection graph subscription for the client.
    ///
    /// Returns false if this client is already subscribed.
    pub(crate) fn add_subscriber(&mut self, client_id: ClientId) -> bool {
        self.subscribers.insert(client_id)
    }

    /// Removes a connection graph subscription for the client.
    ///
    /// Returns false if this client is already unsubscribed.
    pub(crate) fn remove_subscriber(&mut self, client_id: ClientId) -> bool {
        self.subscribers.remove(&client_id)
    }

    /// Returns true if the graph has subscribers.
    pub(crate) fn has_subscribers(&self) -> bool {
        !self.subscribers.is_empty()
    }

    /// Returns true if the client is a subscriber.
    pub(crate) fn is_subscriber(&self, client_id: ClientId) -> bool {
        self.subscribers.contains(&client_id)
    }

    /// Computes a diff of this connection graph against the other.
    pub(crate) fn diff<'a>(&self, other: &'a ConnectionGraph) -> ConnectionGraphDiff<'a> {
        let mut diff = ConnectionGraphDiff::new();

        // Get new or changed published topics
        for (name, publisher_ids) in &other.published_topics {
            if let Some(self_publisher_ids) = self.published_topics.get(name) {
                if self_publisher_ids == publisher_ids {
                    // No change
                    continue;
                }
            }

            diff.published_topics.push(NewPublishedTopic {
                name,
                publisher_ids,
            });
        }

        // Get new or changed subscribed topics
        for (name, subscriber_ids) in &other.subscribed_topics {
            if let Some(self_subscriber_ids) = self.subscribed_topics.get(name) {
                if self_subscriber_ids == subscriber_ids {
                    // No change
                    continue;
                }
            }

            diff.subscribed_topics.push(NewSubscribedTopic {
                name,
                subscriber_ids,
            });
        }

        // Get new or changed advertised services
        for (name, provider_ids) in &other.advertised_services {
            if let Some(self_provider_ids) = self.advertised_services.get(name) {
                if self_provider_ids == provider_ids {
                    // No change
                    continue;
                }
            }

            diff.advertised_services
                .push(NewAdvertisedService { name, provider_ids });
        }

        // Get removed advertised services
        for name in self.advertised_services.keys() {
            if !other.advertised_services.contains_key(name) {
                diff.removed_services.push(name.clone());
            }
        }

        // Get the topics from both published_topics and subscribed_topics that are no longer in either
        for name in self
            .published_topics
            .keys()
            .chain(self.subscribed_topics.keys())
        {
            if other.published_topics.contains_key(name) {
                continue;
            }
            if other.subscribed_topics.contains_key(name) {
                continue;
            }
            diff.removed_topics.insert(name.clone());
        }

        diff
    }

    /// Returns a `ConnectionGraphUpdate` message for the initial state of the graph.
    ///
    /// See: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update>
    pub(crate) fn as_initial_update(&self) -> String {
        ConnectionGraph::default().diff(self).to_json()
    }

    /// Replaces the connection graph content.
    ///
    /// The set of subscribers is not modified.
    ///
    /// Returns a `ConnectionGraphUpdate` message as a JSON string.
    /// See: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update>
    pub(crate) fn update(&mut self, new: ConnectionGraph) -> String {
        let diff = self.diff(&new).to_json();
        self.published_topics = new.published_topics;
        self.subscribed_topics = new.subscribed_topics;
        self.advertised_services = new.advertised_services;
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn assert_json_eq(left: String, right: Value) {
        let left: Value = serde_json::from_str(&left).unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn test_empty_update() {
        let mut graph = ConnectionGraph::new();
        let updated = ConnectionGraph::new();
        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [],
                "subscribedTopics": [],
                "advertisedServices": [],
                "removedTopics": [],
                "removedServices": []
            }),
        );
    }

    #[test]
    fn test_new_published_topic() {
        let mut graph = ConnectionGraph::new();
        let mut updated = ConnectionGraph::new();

        updated.published_topics.insert(
            "topic1".to_string(),
            HashSet::from(["publisher1".to_string()]),
        );

        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [{
                    "name": "topic1",
                    "publisherIds": ["publisher1"]
                }],
                "subscribedTopics": [],
                "advertisedServices": [],
                "removedTopics": [],
                "removedServices": []
            }),
        );
    }

    #[test]
    fn test_removed_topic() {
        let mut graph = ConnectionGraph::new();
        graph.published_topics.insert(
            "topic1".to_string(),
            HashSet::from(["publisher1".to_string()]),
        );

        let updated = ConnectionGraph::new();
        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [],
                "subscribedTopics": [],
                "advertisedServices": [],
                "removedTopics": ["topic1"],
                "removedServices": []
            }),
        );
    }

    #[test]
    fn test_changed_publishers() {
        let mut graph = ConnectionGraph::new();
        graph.published_topics.insert(
            "topic1".to_string(),
            HashSet::from(["publisher1".to_string()]),
        );

        let mut updated = ConnectionGraph::new();
        updated.published_topics.insert(
            "topic1".to_string(),
            HashSet::from(["publisher2".to_string()]),
        );

        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [{
                    "name": "topic1",
                    "publisherIds": ["publisher2"]
                }],
                "subscribedTopics": [],
                "advertisedServices": [],
                "removedTopics": [],
                "removedServices": []
            }),
        );
    }

    #[test]
    fn test_service_changes() {
        let mut graph = ConnectionGraph::new();
        graph.advertised_services.insert(
            "service1".to_string(),
            HashSet::from(["provider1".to_string()]),
        );

        let mut updated = ConnectionGraph::new();
        updated.advertised_services.insert(
            "service2".to_string(),
            HashSet::from(["provider2".to_string()]),
        );

        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [],
                "subscribedTopics": [],
                "advertisedServices": [{
                    "name": "service2",
                    "providerIds": ["provider2"]
                }],
                "removedTopics": [],
                "removedServices": ["service1"]
            }),
        );
    }

    #[test]
    fn test_complex_update() {
        let mut graph = ConnectionGraph::new();
        graph.published_topics.insert(
            "topic1".to_string(),
            HashSet::from(["publisher1".to_string()]),
        );
        graph.subscribed_topics.insert(
            "topic1".to_string(),
            HashSet::from(["subscriber1".to_string()]),
        );
        graph.advertised_services.insert(
            "service1".to_string(),
            HashSet::from(["provider1".to_string()]),
        );

        let mut updated = ConnectionGraph::new();
        updated.published_topics.insert(
            "topic2".to_string(),
            HashSet::from(["publisher2".to_string()]),
        );
        updated.subscribed_topics.insert(
            "topic2".to_string(),
            HashSet::from(["subscriber2".to_string()]),
        );
        updated.advertised_services.insert(
            "service2".to_string(),
            HashSet::from(["provider2".to_string()]),
        );

        let diff = graph.update(updated);

        assert_json_eq(
            diff,
            json!({
                "op": "connectionGraphUpdate",
                "publishedTopics": [{
                    "name": "topic2",
                    "publisherIds": ["publisher2"]
                }],
                "subscribedTopics": [{
                    "name": "topic2",
                    "subscriberIds": ["subscriber2"]
                }],
                "advertisedServices": [{
                    "name": "service2",
                    "providerIds": ["provider2"]
                }],
                "removedTopics": ["topic1"],
                "removedServices": ["service1"]
            }),
        );
    }
}
