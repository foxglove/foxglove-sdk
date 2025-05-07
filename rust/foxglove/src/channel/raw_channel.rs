//! A raw channel.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Release};
use std::sync::{Arc, Weak};
use std::time::Duration;

use parking_lot::Mutex;
use tracing::warn;

use super::ChannelId;
use crate::log_sink_set::LogSinkSet;
use crate::sink::SmallSinkVec;
use crate::throttler::Throttler;
use crate::{nanoseconds_since_epoch, Context, Metadata, PartialMetadata, Schema};

/// Interval for throttled warnings.
static WARN_THROTTLER_INTERVAL: Duration = Duration::from_secs(10);

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
///
/// You should choose a unique topic name per channel for compatibility with the Foxglove app.
pub struct RawChannel {
    id: ChannelId,
    context: Weak<Context>,
    topic: String,
    message_encoding: String,
    schema: Option<Schema>,
    metadata: BTreeMap<String, String>,
    sinks: LogSinkSet,
    closed: AtomicBool,
    warn_throttler: Mutex<Throttler>,
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
            sinks: LogSinkSet::new(),
            closed: AtomicBool::new(false),
            warn_throttler: Mutex::new(Throttler::new(WARN_THROTTLER_INTERVAL)),
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

    /// Returns true if one channel is substantially the same as the other.
    pub(crate) fn matches(&self, other: &Self) -> bool {
        self.topic == other.topic
            && self.message_encoding == other.message_encoding
            && self.schema == other.schema
            && self.metadata == other.metadata
    }

    /// Closes the channel, removing it from the context.
    ///
    /// You can use this to explicitly unadvertise the channel to sinks that subscribe to channels
    /// dynamically, such as the [`WebSocketServer`][crate::WebSocketServer].
    ///
    /// Attempts to log on a closed channel will elicit a throttled warning message.
    pub fn close(&self) {
        if !self.is_closed() {
            if let Some(ctx) = self.context.upgrade() {
                ctx.remove_channel(self.id);
            }
        }
    }

    /// Invoked when the channel is removed from its context.
    ///
    /// This can happen either in the context of an explicit call to [`RawChannel::close`], or due
    /// to the context being dropped.
    pub(crate) fn remove_from_context(&self) {
        self.closed.store(true, Release);
        self.sinks.clear();
    }

    /// Returns true if the channel is closed.
    ///
    /// A channel may be closed either by an explicit call to [`RawChannel::close`], or due to the
    /// context being dropped.
    fn is_closed(&self) -> bool {
        self.closed.load(Acquire)
    }

    /// Issues a throttled warning about attempting to log on a closed channel.
    pub(crate) fn log_warn_if_closed(&self) {
        if self.is_closed() && self.warn_throttler.lock().try_acquire() {
            warn!("Cannot log on closed channel for {}", self.topic());
        }
    }

    /// Updates the set of sinks that are subscribed to this channel.
    pub(crate) fn update_sinks(&self, sinks: SmallSinkVec) {
        self.sinks.store(sinks);
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
    ///
    /// The buffering behavior depends on the log sink; see [`McapWriter`][crate::McapWriter] and
    /// [`WebSocketServer`][crate::WebSocketServer] for details.
    pub fn log(&self, msg: &[u8]) {
        self.log_with_meta(msg, PartialMetadata::default());
    }

    /// Logs a message with additional metadata.
    ///
    /// The buffering behavior depends on the log sink; see [`McapWriter`][crate::McapWriter] and
    /// [`WebSocketServer`][crate::WebSocketServer] for details.
    pub fn log_with_meta(&self, msg: &[u8], opts: PartialMetadata) {
        if self.has_sinks() {
            self.log_to_sinks(msg, opts);
        } else {
            self.log_warn_if_closed();
        }
    }

    /// Logs a message with additional metadata.
    pub(crate) fn log_to_sinks(&self, msg: &[u8], opts: PartialMetadata) {
        let metadata = Metadata {
            log_time: opts.log_time.unwrap_or_else(nanoseconds_since_epoch),
        };

        self.sinks.for_each(|sink| sink.log(self, msg, &metadata));
    }
}

#[cfg(test)]
impl PartialEq for RawChannel {
    fn eq(&self, other: &Self) -> bool {
        self.matches(other)
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
