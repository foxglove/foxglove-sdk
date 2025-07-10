use std::{collections::BTreeMap, sync::Arc};

use crate::{channel::ChannelId, Schema};

/// Information about a Channel.
///
/// Cheaply cloned with references to the underlying data.
#[derive(Clone)]
pub struct ChannelDescriptor(Arc<Inner>);

struct Inner {
    id: ChannelId,
    topic: String,
    message_encoding: String,
    metadata: BTreeMap<String, String>,
    schema: Option<Schema>,
}

impl ChannelDescriptor {
    pub(crate) fn new(
        id: ChannelId,
        topic: String,
        message_encoding: String,
        metadata: BTreeMap<String, String>,
        schema: Option<Schema>,
    ) -> Self {
        Self(Arc::new(Inner {
            id,
            topic,
            message_encoding,
            metadata,
            schema,
        }))
    }

    /// Returns the channel ID.
    pub fn id(&self) -> ChannelId {
        self.0.id
    }

    /// Returns the channel topic.
    pub fn topic(&self) -> &str {
        &self.0.topic
    }

    /// Returns the message encoding for this channel.
    pub fn message_encoding(&self) -> &str {
        &self.0.message_encoding
    }

    /// Returns the metadata for this channel.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.0.metadata
    }

    /// Returns the schema for this channel.
    pub fn schema(&self) -> Option<&Schema> {
        self.0.schema.as_ref()
    }

    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.0.topic == other.0.topic
            && self.0.message_encoding == other.0.message_encoding
            && self.0.metadata == other.0.metadata
            && self.0.schema == other.0.schema
    }
}
