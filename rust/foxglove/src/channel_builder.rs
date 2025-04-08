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
    /// Returns [`FoxgloveError::DuplicateChannel`] if a channel with the same topic already exists,
    /// or [`FoxgloveError::MessageEncodingRequired`] if no message encoding was specified.
    pub fn build_raw(self) -> Result<Arc<RawChannel>, FoxgloveError> {
        let channel = RawChannel::new(
            &self.context,
            self.topic,
            self.message_encoding
                .ok_or_else(|| FoxgloveError::MessageEncodingRequired)?,
            self.schema,
            self.metadata,
        );
        self.context.add_channel(channel.clone())?;
        Ok(channel)
    }

    /// Builds a [`Channel`].
    ///
    /// `T` must implement [`Encode`].
    ///
    /// Returns [`FoxgloveError::DuplicateChannel`] if a channel with the same topic already
    /// exists.
    pub fn build<T: Encode>(mut self) -> Result<Channel<T>, FoxgloveError> {
        if self.message_encoding.is_none() {
            self.message_encoding = Some(<T as Encode>::get_message_encoding());
        }
        if self.schema.is_none() {
            self.schema = <T as Encode>::get_schema();
        }
        let channel = self.build_raw()?;
        Ok(Channel::from_raw_channel(channel))
    }
}
