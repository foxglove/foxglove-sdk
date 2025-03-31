//! Lazily-initialized channels

use std::ops::Deref;
use std::sync::OnceLock;

use crate::Encode;

use super::Channel;

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
