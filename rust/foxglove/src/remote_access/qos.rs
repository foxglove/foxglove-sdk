use crate::channel::ChannelDescriptor;

/// The reliability policy for a channel's data delivery.
///
/// This determines whether data logged on a channel is sent over the reliable control bytestream
/// or over unreliable data tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reliability {
    /// Data is sent over lossy data tracks (unordered, no guaranteed delivery).
    ///
    /// This is the default and is suitable for high-frequency data streams.
    #[default]
    Lossy,
    /// Data is sent over the reliable control channel (ordered, guaranteed delivery).
    ///
    /// Use this for low-frequency, config-like data where every message must be delivered.
    Reliable,
}

/// Quality-of-service profile for a channel.
///
/// Controls how data for a channel is delivered to remote participants.
/// Construct with [`QosProfile::default()`] or [`QosProfile::builder()`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QosProfile {
    pub(crate) reliability: Reliability,
}

impl QosProfile {
    /// Returns a builder for constructing a [`QosProfile`].
    pub fn builder() -> QosProfileBuilder {
        QosProfileBuilder::default()
    }

    /// Returns the reliability policy.
    pub fn reliability(&self) -> Reliability {
        self.reliability
    }
}

/// Builder for [`QosProfile`].
#[derive(Debug, Clone, Default)]
pub struct QosProfileBuilder {
    reliability: Reliability,
}

impl QosProfileBuilder {
    /// Sets the reliability policy.
    #[must_use]
    pub fn reliability(mut self, reliability: Reliability) -> Self {
        self.reliability = reliability;
        self
    }

    /// Builds the [`QosProfile`].
    pub fn build(self) -> QosProfile {
        QosProfile {
            reliability: self.reliability,
        }
    }
}

/// Assigns a [`QosProfile`] to channels.
///
/// This callback is invoked during channel registration to determine the quality-of-service
/// profile for each channel.
pub trait QosClassifier: Sync + Send {
    /// Returns the QoS profile for the given channel.
    fn classify(&self, channel: &ChannelDescriptor) -> QosProfile;
}

pub(crate) struct QosClassifierFn<F>(pub F)
where
    F: Fn(&ChannelDescriptor) -> QosProfile + Sync + Send;

impl<F> QosClassifier for QosClassifierFn<F>
where
    F: Fn(&ChannelDescriptor) -> QosProfile + Sync + Send,
{
    fn classify(&self, channel: &ChannelDescriptor) -> QosProfile {
        self.0(channel)
    }
}
