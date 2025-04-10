/// PartialMetadata is [`Metadata`] with all optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartialMetadata {
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: Option<u64>,
}

/// Metadata is the metadata associated with a log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: u64,
}
