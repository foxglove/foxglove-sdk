//! Conversions from chrono types.

use super::{Duration, Timestamp};
use crate::convert::RangeError;

#[cfg(test)]
mod tests;

impl TryFrom<chrono::TimeDelta> for Duration {
    type Error = RangeError;

    fn try_from(delta: chrono::TimeDelta) -> Result<Self, Self::Error> {
        let num_seconds = delta.num_seconds();
        let Ok(mut sec) = i32::try_from(num_seconds) else {
            return Err(if num_seconds > 0 {
                RangeError::UpperBound
            } else {
                RangeError::LowerBound
            });
        };
        let subsec_nanos = delta.subsec_nanos();
        let nsec = if subsec_nanos >= 0 {
            u32::try_from(subsec_nanos).expect("positive")
        } else if sec == i32::MIN {
            return Err(RangeError::LowerBound);
        } else {
            sec -= 1;
            u32::try_from(subsec_nanos + 1_000_000_000).expect("positive")
        };
        Ok(Self::new(sec, nsec))
    }
}

impl TryFrom<chrono::DateTime<chrono::Utc>> for Timestamp {
    type Error = RangeError;

    fn try_from(time: chrono::DateTime<chrono::Utc>) -> Result<Self, Self::Error> {
        let timestamp = time.timestamp();
        let Ok(sec) = u32::try_from(timestamp) else {
            return Err(if timestamp > 0 {
                RangeError::UpperBound
            } else {
                RangeError::LowerBound
            });
        };
        let nsec = time.timestamp_subsec_nanos();
        Ok(Self::new(sec, nsec))
    }
}

impl TryFrom<chrono::NaiveDateTime> for Timestamp {
    type Error = RangeError;

    fn try_from(time: chrono::NaiveDateTime) -> Result<Self, Self::Error> {
        Self::try_from(time.and_utc())
    }
}
