//! Wrappers for protobuf well-known types
//!
//! For some reason, foxglove uses google's well-known types for representing Duration and
//! Timestamp in protobuf, even though we schematize those types differently. This module provides
//! an infallible translation from the foxglove schema to the underlying protobuf representation.
//!
//! This module lives outside crate::schemas, because everything under the schemas/ direcory is
//! generated.

use crate::convert::{RangeError, SaturatingFrom};

#[cfg(feature = "chrono")]
mod chrono;
#[cfg(test)]
mod tests;

/// Converts time integer types and normalizes excessive nanoseconds into seconds.
fn normalize(sec: impl Into<i64>, mut nsec: u32) -> (i64, i32) {
    if nsec < 1_000_000_000 {
        (sec.into(), i32::try_from(nsec).unwrap())
    } else {
        // We're upconverting seconds from u32/i32, so there's no risk of overflow here.
        let div = nsec / 1_000_000_000;
        nsec %= 1_000_000_000;
        (sec.into() + i64::from(div), i32::try_from(nsec).unwrap())
    }
}

/// A signed, fixed-length span of time.
///
/// The duration is represented by a count of seconds (which may be negative), and a count of
/// fractional seconds at nanosecond resolution (which are always positive).
///
/// # Example
///
/// ```
/// use foxglove::schemas::Duration;
///
/// // A duration of 2.718... seconds.
/// let duration = Duration {
///     sec: 2,
///     nsec: 718_281_828,
/// };
///
/// // A duration of -3.14... seconds. Note that nanoseconds are always in the positive
/// // direction.
/// let duration = Duration {
///     sec: -4,
///     nsec: 858_407_346,
/// };
/// ```
///
/// Various conversions are implemented. These conversions may fail with [`RangeError`], because
/// [`Duration`] represents a more restrictive range of values.
///
/// ```
/// # use foxglove::schemas::Duration;
/// let duration: Duration = std::time::Duration::from_micros(577_215).try_into().unwrap();
/// assert_eq!(
///     duration,
///     Duration {
///         sec: 0,
///         nsec: 577_215_000
///     }
/// );
///
/// #[cfg(feature = "chrono")]
/// {
///     let duration: Duration = chrono::TimeDelta::microseconds(1_414_213).try_into().unwrap();
///     assert_eq!(
///         duration,
///         Duration {
///             sec: 1,
///             nsec: 414_213_000
///         }
///     );
/// }
/// ```
///
/// The [`SaturatingFrom`] and [`SaturatingInto`][crate::convert::SaturatingInto] traits may be
/// used to saturate when the range is exceeded.
///
/// ```
/// # use foxglove::schemas::Duration;
/// use foxglove::convert::SaturatingInto;
///
/// let duration: Duration = std::time::Duration::from_secs(u64::MAX).saturating_into();
/// assert_eq!(duration, Duration::MAX);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Duration {
    /// Seconds offset.
    pub sec: i32,
    /// Nanoseconds offset in the positive direction.
    pub nsec: u32,
}

impl Duration {
    /// Maximum representable duration.
    pub const MAX: Self = Self {
        sec: i32::MAX,
        nsec: 999_999_999,
    };

    /// Minimum representable duration.
    pub const MIN: Self = Self {
        sec: i32::MIN,
        nsec: 0,
    };

    fn into_prost(self) -> prost_types::Duration {
        self.into()
    }

    /// Creates a `Duration` from `f64` seconds, or fails if the value is unrepresentable.
    pub fn try_from_secs_f64(secs: f64) -> Result<Self, RangeError> {
        if secs < i32::MIN as f64 {
            Err(RangeError::LowerBound)
        } else if secs > i32::MAX as f64 {
            if secs < (i32::MAX as f64 + 1.0) {
                let sec = i32::MAX;
                let nsec = ((secs - sec as f64) * 1e9) as u32;
                Ok(Self { sec, nsec })
            } else {
                Err(RangeError::UpperBound)
            }
        } else {
            let mut sec = secs as i32;
            let mut nsec = ((secs - sec as f64) * 1e9) as i32;
            if nsec < 0 {
                sec -= 1;
                nsec += 1_000_000_000;
            }
            Ok(Self {
                sec,
                nsec: u32::try_from(nsec).unwrap_or_else(|e| {
                    panic!("expected {nsec} to be within [0, 1_000_000_000): {e}")
                }),
            })
        }
    }

    /// Saturating `Duration` from `f64` seconds.
    pub fn saturating_from_secs_f64(secs: f64) -> Self {
        match Self::try_from_secs_f64(secs) {
            Ok(d) => d,
            Err(RangeError::LowerBound) => Duration::MIN,
            Err(RangeError::UpperBound) => Duration::MAX,
        }
    }
}

impl From<Duration> for prost_types::Duration {
    fn from(v: Duration) -> Self {
        let (seconds, nanos) = normalize(v.sec, v.nsec);
        Self { seconds, nanos }
    }
}

impl prost::Message for Duration {
    fn encode_raw(&self, buf: &mut impl bytes::BufMut)
    where
        Self: Sized,
    {
        self.into_prost().encode_raw(buf);
    }

    fn merge_field(
        &mut self,
        _tag: u32,
        _wire_type: prost::encoding::wire_type::WireType,
        _buf: &mut impl bytes::Buf,
        _ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError>
    where
        Self: Sized,
    {
        // We only support encoding for now.
        unimplemented!("not implemeneted");
    }

    fn encoded_len(&self) -> usize {
        self.into_prost().encoded_len()
    }

    fn clear(&mut self) {
        self.sec = 0;
        self.nsec = 0;
    }
}

impl TryFrom<std::time::Duration> for Duration {
    type Error = RangeError;

    fn try_from(duration: std::time::Duration) -> Result<Self, Self::Error> {
        let Ok(sec) = i32::try_from(duration.as_secs()) else {
            return Err(RangeError::UpperBound);
        };
        let nsec = duration.subsec_nanos();
        Ok(Self { sec, nsec })
    }
}

impl<T> SaturatingFrom<T> for Duration
where
    Self: TryFrom<T, Error = RangeError>,
{
    fn saturating_from(value: T) -> Self {
        match Self::try_from(value) {
            Ok(d) => d,
            Err(RangeError::LowerBound) => Duration::MIN,
            Err(RangeError::UpperBound) => Duration::MAX,
        }
    }
}

/// A timestamp, represented as an offset from a user-defined epoch.
///
/// # Example
///
/// ```
/// use foxglove::schemas::Timestamp;
///
/// let timestamp = Timestamp {
///     sec: 1_548_054_420,
///     nsec: 76_657_283,
/// };
/// ```
///
/// Various conversions are implemented, which presume the choice of the unix epoch as the
/// reference time. These conversions may fail with [`RangeError`], because [`Timestamp`]
/// represents a more restrictive range of values.
///
/// ```
/// # use foxglove::schemas::Timestamp;
/// let timestamp = Timestamp::try_from(std::time::SystemTime::UNIX_EPOCH).unwrap();
/// assert_eq!(timestamp, Timestamp::MIN);
///
/// #[cfg(feature = "chrono")]
/// {
///     let timestamp = Timestamp::try_from(chrono::DateTime::UNIX_EPOCH).unwrap();
///     assert_eq!(timestamp, Timestamp::MIN);
///     let timestamp = Timestamp::try_from(chrono::NaiveDateTime::UNIX_EPOCH).unwrap();
///     assert_eq!(timestamp, Timestamp::MIN);
/// }
/// ```
///
/// The [`SaturatingFrom`] and [`SaturatingInto`][crate::convert::SaturatingInto] traits may be
/// used to saturate when the range is exceeded.
///
/// ```
/// # use foxglove::schemas::Timestamp;
/// use foxglove::convert::SaturatingInto;
///
/// let timestamp: Timestamp = std::time::SystemTime::UNIX_EPOCH
///     .checked_sub(std::time::Duration::from_secs(1))
///     .unwrap()
///     .saturating_into();
/// assert_eq!(timestamp, Timestamp::MIN);
/// ```
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Timestamp {
    /// Seconds since epoch.
    pub sec: u32,
    /// Additional nanoseconds since epoch.
    pub nsec: u32,
}

impl Timestamp {
    /// Maximum representable timestamp.
    pub const MAX: Self = Self {
        sec: u32::MAX,
        nsec: 999_999_999,
    };

    /// Minimum representable timestamp.
    pub const MIN: Self = Self { sec: 0, nsec: 0 };

    fn into_prost(self) -> prost_types::Timestamp {
        self.into()
    }

    /// Creates a `Timestamp` from seconds since epoch as an `f64`, or fails if the value is
    /// unrepresentable.
    pub fn try_from_epoch_secs_f64(secs: f64) -> Result<Self, RangeError> {
        if secs < 0.0 {
            Err(RangeError::LowerBound)
        } else if secs > u32::MAX as f64 {
            if secs < (u32::MAX as f64 + 1.0) {
                let sec = u32::MAX;
                let nsec = ((secs - sec as f64) * 1e9) as u32;
                Ok(Self { sec, nsec })
            } else {
                Err(RangeError::UpperBound)
            }
        } else {
            let sec = secs as u32;
            let nsec = ((secs - sec as f64) * 1e9) as u32;
            Ok(Self { sec, nsec })
        }
    }

    /// Saturating `Timestamp` from seconds since epoch as an `f64`.
    pub fn saturating_from_epoch_secs_f64(secs: f64) -> Self {
        match Self::try_from_epoch_secs_f64(secs) {
            Ok(d) => d,
            Err(RangeError::LowerBound) => Timestamp::MIN,
            Err(RangeError::UpperBound) => Timestamp::MAX,
        }
    }
}

impl From<Timestamp> for prost_types::Timestamp {
    fn from(v: Timestamp) -> Self {
        let (seconds, nanos) = normalize(v.sec, v.nsec);
        Self { seconds, nanos }
    }
}

impl prost::Message for Timestamp {
    fn encode_raw(&self, buf: &mut impl bytes::BufMut)
    where
        Self: Sized,
    {
        self.into_prost().encode_raw(buf);
    }

    fn merge_field(
        &mut self,
        _tag: u32,
        _wire_type: prost::encoding::wire_type::WireType,
        _buf: &mut impl bytes::Buf,
        _ctx: prost::encoding::DecodeContext,
    ) -> Result<(), prost::DecodeError>
    where
        Self: Sized,
    {
        // We only support encoding for now.
        unimplemented!("not implemeneted");
    }

    fn encoded_len(&self) -> usize {
        self.into_prost().encoded_len()
    }

    fn clear(&mut self) {
        self.sec = 0;
        self.nsec = 0;
    }
}

impl TryFrom<std::time::SystemTime> for Timestamp {
    type Error = RangeError;

    fn try_from(time: std::time::SystemTime) -> Result<Self, Self::Error> {
        let Ok(duration) = time.duration_since(std::time::UNIX_EPOCH) else {
            return Err(RangeError::LowerBound);
        };
        let Ok(sec) = u32::try_from(duration.as_secs()) else {
            return Err(RangeError::UpperBound);
        };
        let nsec = duration.subsec_nanos();
        Ok(Self { sec, nsec })
    }
}

impl<T> SaturatingFrom<T> for Timestamp
where
    Self: TryFrom<T, Error = RangeError>,
{
    fn saturating_from(value: T) -> Self {
        match Self::try_from(value) {
            Ok(d) => d,
            Err(RangeError::LowerBound) => Timestamp::MIN,
            Err(RangeError::UpperBound) => Timestamp::MAX,
        }
    }
}
