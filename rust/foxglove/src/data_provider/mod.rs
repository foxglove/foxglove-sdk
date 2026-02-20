//! Types and utilities for building [remote data loader] manifests.
//!
//! Use [`ChannelSet`] to declare channels, then construct a [`StreamedSource`] with the
//! resulting topics and schemas.
//!
//! # Example
//!
//! ```
//! use chrono::{DateTime, Utc};
//! use foxglove::data_provider::{ChannelSet, Manifest, StreamedSource, UpstreamSource};
//!
//! #[derive(foxglove::Encode)]
//! struct MyMessage {
//!     value: i32,
//! }
//!
//! let mut channels = ChannelSet::new();
//! channels.insert::<MyMessage>("/topic1");
//! channels.insert::<MyMessage>("/topic2");
//!
//! let (topics, schemas) = channels.into_topics_and_schemas();
//! let source = StreamedSource {
//!     url: "/v1/data?flightId=ABC123".into(),
//!     id: Some("flight-v1-ABC123".into()),
//!     topics,
//!     schemas,
//!     start_time: DateTime::<Utc>::MIN_UTC,
//!     end_time: DateTime::<Utc>::MAX_UTC,
//! };
//!
//! let manifest = Manifest {
//!     name: Some("Flight ABC123".into()),
//!     sources: vec![UpstreamSource::Streamed(source)],
//! };
//! ```
//!
//! [remote data loader]: https://docs.foxglove.dev/docs/visualization/connecting/remote-data-loader

mod manifest;

pub use manifest::*;

use std::num::NonZeroU16;

use crate::Encode;

/// A set of channel declarations for a [`StreamedSource`].
///
/// Handles schema extraction from [`Encode`] types, schema ID assignment, and deduplication.
///
/// See the [module-level documentation](self) for a complete example.
pub struct ChannelSet {
    topics: Vec<Topic>,
    schemas: Vec<Schema>,
    next_schema_id: Option<NonZeroU16>,
}

impl ChannelSet {
    /// Create a new empty channel set.
    pub fn new() -> Self {
        Self {
            topics: Vec::new(),
            schemas: Vec::new(),
            next_schema_id: Some(NonZeroU16::MIN),
        }
    }

    /// Insert a channel for message type `T`.
    ///
    /// Extracts the schema from `T: Encode`, assigns a schema ID, and deduplicates schemas
    /// automatically. If multiple channels share the same schema, only one schema entry will be
    /// created.
    pub fn insert<T: Encode>(&mut self, topic: impl Into<String>) {
        let schema_id = T::get_schema().map(|s| self.add_schema(s));
        self.topics.push(Topic {
            name: topic.into(),
            message_encoding: T::get_message_encoding(),
            schema_id,
        });
    }

    /// Consume the set and return the topics and schemas.
    ///
    /// Use the returned values to construct a [`StreamedSource`] directly.
    pub fn into_topics_and_schemas(self) -> (Vec<Topic>, Vec<Schema>) {
        (self.topics, self.schemas)
    }

    fn add_schema(&mut self, schema: crate::Schema) -> NonZeroU16 {
        // Do not add duplicate schemas.
        let existing = self.schemas.iter().find(|existing| {
            existing.name == schema.name
                && existing.encoding == schema.encoding
                && existing.data.as_ref() == schema.data.as_ref()
        });

        if let Some(existing) = existing {
            existing.id
        } else {
            let id = self
                .next_schema_id
                .expect("should not add more than 65535 schemas");
            self.next_schema_id = id.checked_add(1);
            self.schemas.push(Schema {
                id,
                name: schema.name,
                encoding: schema.encoding,
                data: schema.data.into(),
            });
            id
        }
    }
}

impl Default for ChannelSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_streamed_source_builder_snapshot() {
        let mut channels = ChannelSet::new();
        channels.insert::<crate::messages::Vector3>("/topic1");
        channels.insert::<crate::messages::Vector3>("/topic2");

        let (topics, schemas) = channels.into_topics_and_schemas();
        assert_eq!(topics.len(), 2);
        assert_eq!(schemas.len(), 1);

        let source = StreamedSource {
            url: "/v1/data".into(),
            id: Some("test-id".into()),
            topics,
            schemas,
            start_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2024, 1, 2, 0, 0, 0).unwrap(),
        };

        let manifest = Manifest {
            name: Some("Test Source".into()),
            sources: vec![UpstreamSource::Streamed(source)],
        };

        insta::assert_json_snapshot!(manifest);
    }
}
