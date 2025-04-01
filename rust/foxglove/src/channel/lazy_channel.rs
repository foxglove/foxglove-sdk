//! Lazily-initialized channels

use std::sync::OnceLock;
use std::{ops::Deref, sync::Arc};

use crate::{ChannelBuilder, Encode, Schema};

use super::{Channel, RawChannel};

/// A channel that is initialized lazily upon first use.
///
/// A common pattern is to create the channels once as static variables, and then use them
/// throughout the application. But because channels do not have a const initializer, they must
/// be initialized lazily. [`LazyChannel`] provides a convenient way to do this.
///
/// Be careful when using this pattern. The channel will not be advertised to sinks until it is
/// initialized, which is guaranteed to happen when the channel is first used. If you need to
/// ensure the channel is initialized _before_ using it, you can use [`LazyChannel::init`].
///
/// # Example
/// ```
/// use foxglove::LazyChannel;
/// use foxglove::schemas::FrameTransform;
///
/// static TF: LazyChannel<FrameTransform> = LazyChannel::new("/tf");
/// ```
pub struct LazyChannel<T: Encode> {
    topic: &'static str,
    inner: OnceLock<Channel<T>>,
}

impl<T: Encode> LazyChannel<T> {
    /// Creates a new lazily-initialized channel.
    pub const fn new(topic: &'static str) -> Self {
        Self {
            topic,
            inner: OnceLock::new(),
        }
    }

    /// Ensures that the channel is initialized.
    ///
    /// If the channel is already initialized, this is a no-op.
    pub fn init(&self) {
        self.get_or_init();
    }

    /// Returns a reference to the channel, initializing it if necessary.
    fn get_or_init(&self) -> &Channel<T> {
        self.inner.get_or_init(|| {
            Channel::new(self.topic).unwrap_or_else(|e| {
                panic!(
                    "Failed to lazily initialize channel for {}: {e:?}",
                    self.topic
                )
            })
        })
    }
}

impl<T: Encode> Deref for LazyChannel<T> {
    type Target = Channel<T>;

    fn deref(&self) -> &Self::Target {
        self.get_or_init()
    }
}

/// A raw channel that is initialized lazily upon first use.
///
/// A common pattern is to create the channels once as static variables, and then use them
/// throughout the application. But because channels do not have a const initializer, they must
/// be initialized lazily. [`LazyRawChannel`] provides a convenient way to do this for raw
/// channels.
///
/// Be careful when using this pattern. The channel will not be advertised to sinks until it is
/// initialized, which is guaranteed to happen when the channel is first used. If you need to
/// ensure the channel is initialized _before_ using it, you can use [`LazyRawChannel::init`].
///
/// # Example
/// ```
/// use foxglove::LazyRawChannel;
///
/// static RAW: LazyRawChannel = LazyRawChannel::new("/raw", "message encoding")
///     .schema("schema name", "schema encoding", b"schema data");
/// ```
pub struct LazyRawChannel {
    topic: &'static str,
    message_encoding: &'static str,
    schema: Option<(&'static str, &'static str, &'static [u8])>,
    inner: OnceLock<Arc<RawChannel>>,
}

impl LazyRawChannel {
    /// Creates a new lazily-initialized raw channel.
    pub const fn new(topic: &'static str, message_encoding: &'static str) -> Self {
        Self {
            topic,
            message_encoding,
            schema: None,
            inner: OnceLock::new(),
        }
    }

    /// Sets the schema for the channel.
    #[must_use]
    pub const fn schema(
        mut self,
        name: &'static str,
        encoding: &'static str,
        data: &'static [u8],
    ) -> Self {
        self.schema = Some((name, encoding, data));
        self
    }

    /// Ensures that the channel is initialized.
    ///
    /// If the channel is already initialized, this is a no-op.
    pub fn init(&self) {
        self.get_or_init();
    }

    /// Returns a reference to the channel, initializing it if necessary.
    fn get_or_init(&self) -> &Arc<RawChannel> {
        self.inner.get_or_init(|| {
            let schema = self
                .schema
                .map(|(name, encoding, data)| Schema::new(name, encoding, data));
            ChannelBuilder::new(self.topic)
                .message_encoding(self.message_encoding)
                .schema(schema)
                .build_raw()
                .unwrap_or_else(|e| {
                    panic!(
                        "Failed to lazily initialize channel for {}: {e:?}",
                        self.topic
                    )
                })
        })
    }
}

impl Deref for LazyRawChannel {
    type Target = Arc<RawChannel>;

    fn deref(&self) -> &Self::Target {
        self.get_or_init()
    }
}
