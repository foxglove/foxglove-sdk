use std::time::{SystemTime, UNIX_EPOCH};

use crate::schemas::Timestamp;

/// A trait for converting a time value to a u64 nanoseconds since epoch.
pub trait ToUnixNanos {
    /// Returns the value as nanoseconds since epoch.
    fn to_unix_nanos(&self) -> u64;
}

impl ToUnixNanos for u64 {
    fn to_unix_nanos(&self) -> u64 {
        *self
    }
}

impl ToUnixNanos for Timestamp {
    fn to_unix_nanos(&self) -> u64 {
        self.total_nanos()
    }
}

impl ToUnixNanos for SystemTime {
    fn to_unix_nanos(&self) -> u64 {
        self.duration_since(UNIX_EPOCH)
            .expect("SystemTime out of range")
            .as_nanos() as u64
    }
}

#[cfg(feature = "chrono")]
impl ToUnixNanos for chrono::DateTime<chrono::Utc> {
    fn to_unix_nanos(&self) -> u64 {
        self.timestamp_nanos_opt().expect("timestamp out of range") as u64
    }
}

/// PartialMetadata is [`Metadata`] with all optional fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PartialMetadata {
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: Option<u64>,
}

impl PartialMetadata {
    /// Returns a new PartialMetadata with the given log time.
    ///
    /// `log_time` can be a u64 (nanoseconds since epoch), a foxglove [`Timestamp`][crate::schemas::Timestamp],
    /// a [`SystemTime`][std::time::SystemTime], or anything else that implements [`ToUnixNanos`][crate::ToUnixNanos].
    pub fn with_log_time(log_time: impl ToUnixNanos) -> Self {
        Self {
            log_time: Some(log_time.to_unix_nanos()),
        }
    }
}

/// Metadata is the metadata associated with a log message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// The log time is the time, as nanoseconds from the unix epoch, that the message was recorded.
    /// Usually this is the time log() is called. If omitted, the current time is used.
    pub log_time: u64,
}
