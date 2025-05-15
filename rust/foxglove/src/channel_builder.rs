use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Channel, Context, Encode, FoxgloveError, RawChannel, Schema};

/// A builder for creating a [`Channel`] or [`RawChannel`].
#[must_use]
#[derive(Debug)]
pub struct ChannelBuilder {
    topic: String,
    message_encoding: Option<String>,
    schema: Option<Schema>,
    metadata: BTreeMap<String, String>,
    context: Arc<Context>,
}

impl ChannelBuilder {
    /// Creates a new channel builder for the specified topic.
    ///
    /// You should choose a unique topic name per channel for compatibility with the Foxglove app.
    pub fn new<T: Into<String>>(topic: T) -> Self {
        Self {
            topic: topic.into(),
            message_encoding: None,
            schema: None,
            metadata: BTreeMap::new(),
            context: Context::get_default(),
        }
    }

    /// Set the schema for the channel. It's good practice to set a schema for the channel
    /// and the ensure all messages logged on the channel conform to the schema.
    /// This helps you get the most out of Foxglove. But it's not required.
    pub fn schema(mut self, schema: impl Into<Option<Schema>>) -> Self {
        self.schema = schema.into();
        self
    }

    /// Set the message encoding for the channel.
    ///
    /// This is required for [`RawChannel`], but not for [`Channel`] (it's provided by the
    /// [`Encode`] trait for [`Channel`].) Foxglove supports several well-known message encodings:
    /// <https://docs.foxglove.dev/docs/visualization/message-schemas/introduction>
    pub fn message_encoding(mut self, encoding: impl Into<String>) -> Self {
        self.message_encoding = Some(encoding.into());
        self
    }

    /// Set the metadata for the channel.
    /// Metadata is an optional set of user-defined key-value pairs.
    pub fn metadata(mut self, metadata: BTreeMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a key-value pair to the metadata for the channel.
    pub fn add_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    /// Sets the context for this channel.
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Builds a [`RawChannel`].
    ///
    /// Returns [`FoxgloveError::MessageEncodingRequired`] if no message encoding was specified.
    pub fn build_raw(self) -> Result<Arc<RawChannel>, FoxgloveError> {
        let mut channel = RawChannel::new(
            &self.context,
            self.topic,
            self.message_encoding
                .ok_or_else(|| FoxgloveError::MessageEncodingRequired)?,
            self.schema,
            self.metadata,
        );
        channel = self.context.add_channel(channel);
        Ok(channel)
    }

    /// Builds a [`Channel`].
    ///
    /// `T` must implement [`Encode`].
    pub fn build<T: Encode>(mut self) -> Channel<T> {
        if self.message_encoding.is_none() {
            self.message_encoding = Some(T::get_message_encoding());
        }
        if self.schema.is_none() {
            self.schema = <T as Encode>::get_schema();
        }
        // We know that message_encoding is set and build_raw will succeed.
        let channel = self.build_raw().expect("Failed to build raw channel");
        Channel::from_raw_channel(channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_with_no_options() {
        let builder = ChannelBuilder::new("topic");
        let channel = builder.build::<String>();
        assert_eq!(channel.topic(), "topic");
    }
}
