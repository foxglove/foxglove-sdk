/// PartialMetadata is [`Metadata`] with all optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartialMetadata {
    /// The sequence number is unique per channel, and allows for ordering of messages as well as
    /// detecting missing messages. If omitted, a monotonically increasing sequence number unique to
    /// the channel is used.
    pub sequence: Option<u32>,
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: Option<u64>,
}

/// Metadata is the metadata associated with a log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// The sequence number is unique per channel, and allows for ordering of messages as well as
    /// detecting missing messages. If omitted, a monotonically increasing sequence number unique to
    /// the channel is used.
    pub sequence: u32,
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: u64,
}
