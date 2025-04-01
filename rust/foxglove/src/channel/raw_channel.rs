//! A raw channel.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Weak};

use super::ChannelId;
use crate::log_sink_set::LogSinkSet;
use crate::sink::SmallSinkVec;
use crate::{nanoseconds_since_epoch, Context, Metadata, PartialMetadata, Schema};

/// A log channel that can be used to log binary messages.
///
/// A "channel" is conceptually the same as a [MCAP channel]: it is a stream of messages which all
/// have the same type, or schema. Each channel is instantiated with a unique "topic", or name,
/// which is typically prefixed by a `/`.
///
/// [MCAP channel]: https://mcap.dev/guides/concepts#channel
///
/// If a schema was provided, all messages must be encoded according to the schema.
/// This is not checked. See [`Channel`](crate::Channel) for type-safe channels. Channels are
/// immutable, returned as `Arc<Channel>` and can be shared between threads.
///
/// Channels are created using [`ChannelBuilder`](crate::ChannelBuilder).
pub struct RawChannel {
    id: ChannelId,
    context: Weak<Context>,
    topic: String,
    message_encoding: String,
    schema: Option<Schema>,
    metadata: BTreeMap<String, String>,
    message_sequence: AtomicU32,
    sinks: LogSinkSet,
}

impl RawChannel {
    pub(crate) fn new(
        context: &Arc<Context>,
        topic: String,
        message_encoding: String,
        schema: Option<Schema>,
        metadata: BTreeMap<String, String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: ChannelId::next(),
            context: Arc::downgrade(context),
            topic,
            message_encoding,
            schema,
            metadata,
            message_sequence: AtomicU32::new(1),
            sinks: LogSinkSet::new(),
        })
    }

    /// Returns the channel ID.
    pub fn id(&self) -> ChannelId {
        self.id
    }

    /// Returns the channel topic.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Returns the channel schema.
    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// Returns the message encoding for this channel.
    pub fn message_encoding(&self) -> &str {
        &self.message_encoding
    }

    /// Returns the metadata for this channel.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    /// Atomically increments and returns the next message sequence number.
    pub fn next_sequence(&self) -> u32 {
        self.message_sequence.fetch_add(1, Relaxed)
    }

    /// Closes the channel, removing it from the context.
    ///
    /// Future messages logged to the channel will not be received by any sink.
    pub fn close(&self) {
        if let Some(ctx) = self.context.upgrade() {
            ctx.remove_channel(self.id);
        }
    }

    /// Updates the set of sinks that are subscribed to this channel.
    pub(crate) fn update_sinks(&self, sinks: SmallSinkVec) {
        self.sinks.store(sinks);
    }

    /// Clears the set of subscribed sinks.
    pub(crate) fn clear_sinks(&self) {
        self.sinks.clear();
    }

    /// Returns true if at least one sink is subscribed to this channel.
    pub fn has_sinks(&self) -> bool {
        !self.sinks.is_empty()
    }

    /// Returns the count of sinks subscribed to this channel.
    #[cfg(test)]
    pub(crate) fn num_sinks(&self) -> usize {
        self.sinks.len()
    }

    /// Logs a message.
    pub fn log(&self, msg: &[u8]) {
        if self.has_sinks() {
            self.log_to_sinks(msg, PartialMetadata::default());
        }
    }

    /// Logs a message with additional metadata.
    pub fn log_with_meta(&self, msg: &[u8], opts: PartialMetadata) {
        if self.has_sinks() {
            self.log_to_sinks(msg, opts);
        }
    }

    /// Logs a message with additional metadata.
    pub(crate) fn log_to_sinks(&self, msg: &[u8], opts: PartialMetadata) {
        let mut metadata = Metadata {
            sequence: opts.sequence.unwrap_or_else(|| self.next_sequence()),
            log_time: opts.log_time.unwrap_or_else(nanoseconds_since_epoch),
            publish_time: opts.publish_time.unwrap_or_default(),
        };
        // If publish_time is not set, use log_time.
        if opts.publish_time.is_none() {
            metadata.publish_time = metadata.log_time
        }

        self.sinks.for_each(|sink| sink.log(self, msg, &metadata));
    }
}

#[cfg(test)]
impl PartialEq for RawChannel {
    fn eq(&self, other: &Self) -> bool {
        self.topic == other.topic
            && self.message_encoding == other.message_encoding
            && self.schema == other.schema
            && self.metadata == other.metadata
            && self.message_sequence.load(Relaxed) == other.message_sequence.load(Relaxed)
    }
}

#[cfg(test)]
impl Eq for RawChannel {}

impl std::fmt::Debug for RawChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("id", &self.id)
            .field("topic", &self.topic)
            .field("message_encoding", &self.message_encoding)
            .field("schema", &self.schema)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}
