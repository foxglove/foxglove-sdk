use std::ops::Deref;
use std::sync::{Arc, LazyLock};

use crate::{Encode, LazyChannel, LazyRawChannel};

use super::Context;

static DEFAULT_CONTEXT: LazyContext = LazyContext::new();

/// A context that is initialized lazily upon first use.
///
/// This is intended to be used with [`LazyChannel`][crate::LazyChannel] to create static channels
/// attached to static contexts.
///
/// Refer to the [`Context`] documentation for more information about contexts.
///
/// # Example
/// ```
/// use foxglove::schemas::Log;
/// use foxglove::{LazyChannel, LazyContext, LazyRawChannel};
///
/// // Create two channels for the same topic, in different contexts.
/// static TOPIC: &str = "/topic";
/// static CTX_A: LazyContext = LazyContext::new();
/// static CTX_B: LazyContext = LazyContext::new();
/// static LOG_A: LazyChannel<Log> = CTX_A.channel(TOPIC);
/// static LOG_B: LazyRawChannel = CTX_B.raw_channel(TOPIC, "json");
/// LOG_A.log(&Log {
///     message: "hello a".into(),
///     ..Log::default()
/// });
/// LOG_B.log(br#"{"message": "hello b"}"#);
/// ```
pub struct LazyContext(LazyLock<Arc<Context>>);

impl LazyContext {
    /// Creates a new lazily-initialized channel.
    #[allow(clippy::new_without_default)] // avoid confusion with LazyContext::get_default()
    pub const fn new() -> Self {
        Self(LazyLock::new(Context::new))
    }

    /// Returns a reference to the lazily-initialized default context.
    pub const fn get_default() -> &'static Self {
        &DEFAULT_CONTEXT
    }

    /// Creates a new lazily-initialized channel in this context.
    pub const fn channel<T: Encode>(&'static self, topic: &'static str) -> LazyChannel<T> {
        LazyChannel::new(topic).context(self)
    }

    /// Creates a new lazily-initialized raw channel in this context.
    pub const fn raw_channel(
        &'static self,
        topic: &'static str,
        message_encoding: &'static str,
    ) -> LazyRawChannel {
        LazyRawChannel::new(topic, message_encoding).context(self)
    }
}

impl Deref for LazyContext {
    type Target = Arc<Context>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
