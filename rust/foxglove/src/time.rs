use std::time::{SystemTime, UNIX_EPOCH};

/// nanoseconds_since_epoch returns the current time in nanoseconds since the Unix epoch.
/// This is useful for setting timestamps in log messages.
pub(crate) fn nanoseconds_since_epoch() -> u64 {
    let now = SystemTime::now();
    if let Ok(duration) = now.duration_since(UNIX_EPOCH) {
        return duration.as_secs() * 1_000_000_000 + duration.subsec_nanos() as u64;
    }
    0
}

pub(crate) fn millis_since_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
