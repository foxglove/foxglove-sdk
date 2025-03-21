use std::collections::BTreeMap;
use std::sync::Arc;

use crate::encode::TypedChannel;
use crate::{Channel, Context, Encode, FoxgloveError, Schema};

/// ChannelBuilder is a builder for creating a new [`Channel`] or [`TypedChannel`].
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
    /// This is required for Channel, but not for [`TypedChannel`] (it's provided by the [`Encode`]
    /// trait for [`TypedChannel`].) Foxglove supports several well-known message encodings:
    /// <https://docs.foxglove.dev/docs/visualization/message-schemas/introduction>
    pub fn message_encoding(mut self, encoding: &str) -> Self {
        self.message_encoding = Some(encoding.to_string());
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
    #[doc(hidden)]
    pub fn context(mut self, ctx: &Arc<Context>) -> Self {
        self.context = ctx.clone();
        self
    }

    /// Build the channel and return it in an [`Arc`] as a Result.
    /// Returns [`FoxgloveError::DuplicateChannel`] if a channel with the same topic already exists.
    pub fn build(self) -> Result<Arc<Channel>, FoxgloveError> {
        let channel = Channel::new(
            self.topic,
            self.message_encoding
                .ok_or_else(|| FoxgloveError::MessageEncodingRequired)?,
            self.schema,
            self.metadata,
        );
        self.context.add_channel(channel.clone())?;
        Ok(channel)
    }

    /// Build the channel and return it as a [`TypedChannel`] as a Result.
    /// `T` must implement [`Encode`].
    /// Returns [`FoxgloveError::DuplicateChannel`] if a channel with the same topic already exists.
    pub fn build_typed<T: Encode>(mut self) -> Result<TypedChannel<T>, FoxgloveError> {
        if self.message_encoding.is_none() {
            self.message_encoding = Some(<T as Encode>::get_message_encoding());
        }
        if self.schema.is_none() {
            self.schema = <T as Encode>::get_schema();
        }
        let channel = self.build()?;
        Ok(TypedChannel::from_channel(channel))
    }
}
