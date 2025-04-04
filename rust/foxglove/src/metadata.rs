/// PartialMetadata is `Metadata` with all optional fields.
///
/// These timestamps are somewhat arbitrary, but should be used consistently within your app.
/// Foxglove currently only supports full playback in `log_time` order, but we are tracking
/// supporting playback in `publish_time` order as well.
///
/// See [MCAP Message](https://mcap.dev/spec#message-op0x05) for more information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartialMetadata {
    /// The sequence number is unique per channel,
    /// and allows for ordering of messages as well as detecting missing messages.
    /// If omitted, a monotonically increasing sequence number unique to the channel is used.
    pub sequence: Option<u32>,
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: Option<u64>,
    /// The publish_time is the time at which the message was published.
    /// e.g. the timestamp at which the sensor reading was taken.
    /// If omitted, log time is used.
    pub publish_time: Option<u64>,
}

/// Metadata is the metadata associated with a log message.
///
/// It includes the sequence number, log time, and publish time.
/// See the documentation for [`PartialMetadata`] for more information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// The sequence number is unique per channel,
    /// and allows for ordering of messages as well as detecting missing messages.
    pub sequence: u32,
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: u64,
    /// The publish_time is the time at which the message was published.
    /// e.g. the timestamp at which the sensor reading was taken.
    pub publish_time: u64,
}
