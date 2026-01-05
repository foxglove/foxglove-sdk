//! Connection graph update message types.

use serde::{Deserialize, Serialize};

use crate::websocket::ws_protocol::JsonMessage;

/// Connection graph update message.
///
/// Spec: <https://github.com/foxglove/ws-protocol/blob/main/docs/spec.md#connection-graph-update>
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename = "connectionGraphUpdate", rename_all = "camelCase")]
pub struct ConnectionGraphUpdate {
    /// Published topics.
    pub published_topics: Vec<PublishedTopic>,
    /// Subscribed topics.
    pub subscribed_topics: Vec<SubscribedTopic>,
    /// Advertised services.
    pub advertised_services: Vec<AdvertisedService>,
    /// Removed tpoics.
    pub removed_topics: Vec<String>,
    /// Removed services.
    pub removed_services: Vec<String>,
}

impl JsonMessage for ConnectionGraphUpdate {}

/// A published topic in a [`ConnectionGraphUpdate`] message.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedTopic {
    /// Topic name.
    pub name: String,
    /// Topic subscriber IDs.
    pub publisher_ids: Vec<String>,
}

impl PublishedTopic {
    /// Creates a new published topic.
    pub fn new(
        name: impl Into<String>,
        publisher_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            publisher_ids: publisher_ids.into_iter().map(|id| id.into()).collect(),
        }
    }
}

/// A subscribed topic in a [`ConnectionGraphUpdate`] message.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribedTopic {
    /// Topic name.
    pub name: String,
    /// Topic subscriber IDs.
    pub subscriber_ids: Vec<String>,
}

impl SubscribedTopic {
    /// Creates a new subscribed topic.
    pub fn new(
        name: impl Into<String>,
        subscriber_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            subscriber_ids: subscriber_ids.into_iter().map(|id| id.into()).collect(),
        }
    }
}

/// An advertised service in a [`ConnectionGraphUpdate`] message.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvertisedService {
    /// Service name.
    pub name: String,
    /// Service provider IDs.
    pub provider_ids: Vec<String>,
}

impl AdvertisedService {
    /// Creates a new advertised service.
    pub fn new(
        name: impl Into<String>,
        provider_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            name: name.into(),
            provider_ids: provider_ids.into_iter().map(|id| id.into()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::websocket::ws_protocol::server::ServerMessage;

    use super::*;

    fn message() -> ConnectionGraphUpdate {
        ConnectionGraphUpdate {
            published_topics: vec![PublishedTopic::new("/t1", ["p1", "p2"])],
            subscribed_topics: vec![SubscribedTopic::new("/t2", ["s1", "s2"])],
            advertised_services: vec![AdvertisedService::new("/s1", ["pr1", "pr2"])],
            removed_topics: ["/t3", "/t4"].into_iter().map(String::from).collect(),
            removed_services: ["/s2", "/s3"].into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn test_encode() {
        insta::assert_json_snapshot!(message());
    }

    #[test]
    fn test_roundtrip() {
        let orig = message();
        let buf = orig.to_string();
        let msg = ServerMessage::parse_json(&buf).unwrap();
        assert_eq!(msg, ServerMessage::ConnectionGraphUpdate(orig));
    }
}
