//! Wrappers for protobuf well-known types
//!
//! For some reason, foxglove uses google's well-known types for representing Duration and
//! Timestamp in protobuf, even though we schematize those types differently. This module provides
//! an infallible translation from the foxglove schema to the underlying protobuf representation.
//!
//! This module lives outside `crate::schemas`, because everything under the schemas/ direcory is
//! generated.

use crate::convert::{RangeError, SaturatingFrom};

#[cfg(feature = "chrono")]
mod chrono;
#[cfg(test)]
mod tests;

/// The result type for [`normalize_nsec`].
#[derive(Debug, PartialEq, Eq)]
enum NormalizeResult {
    /// Nanoseconds already within range.
    Ok(u32),
    /// Nanoseconds overflowed into seconds. Result is `(sec, nsec)`.
    Overflow(u32, u32),
}

/// Normalizes nsec to be on within range `[0, 1_000_000_000)`.
fn normalize_nsec(nsec: u32) -> NormalizeResult {
    if nsec < 1_000_000_000 {
        NormalizeResult::Ok(nsec)
    } else {
        let sec = nsec / 1_000_000_000;
        NormalizeResult::Overflow(sec, nsec % 1_000_000_000)
    }
}

impl NormalizeResult {
    /// Carries the result into an i32 representation of seconds.
    ///
    /// Returns None if the result overflows seconds.
    fn carry_i32(self, sec: i32) -> Option<(i32, u32)> {
        match self {
            Self::Ok(nsec) => Some((sec, nsec)),
            Self::Overflow(extra_sec, nsec) => {
                let Ok(extra_sec) = i32::try_from(extra_sec) else {
                    unreachable!("expected {extra_sec} to be within [0, 4]")
                };
                sec.checked_add(extra_sec).map(|sec| (sec, nsec))
            }
        }
    }

    /// Carries the result into a u32 representation of seconds.
    ///
    /// Returns None if the result overflows seconds.
    fn carry_u32(self, sec: u32) -> Option<(u32, u32)> {
        match self {
            Self::Ok(nsec) => Some((sec, nsec)),
            Self::Overflow(extra_sec, nsec) => sec.checked_add(extra_sec).map(|sec| (sec, nsec)),
        }
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
/// let duration = Duration::new(2, 718_281_828);
///
/// // A duration of -3.14... seconds. Note that nanoseconds are always in the positive
/// // direction.
/// let duration = Duration::new(-4, 858_407_346);
/// ```
///
/// Various conversions are implemented. These conversions may fail with [`RangeError`], because
/// [`Duration`] represents a more restrictive range of values.
///
/// ```
/// # use foxglove::schemas::Duration;
/// let duration: Duration = std::time::Duration::from_micros(577_215).try_into().unwrap();
/// assert_eq!(duration, Duration::new(0, 577_215_000));
///
/// #[cfg(feature = "chrono")]
/// {
///     let duration: Duration = chrono::TimeDelta::microseconds(1_414_213).try_into().unwrap();
///     assert_eq!(duration, Duration::new(1, 414_213_000));
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration {
    /// Seconds offset.
    sec: i32,
    /// Nanoseconds offset in the positive direction.
    nsec: u32,
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

    /// Creates a new normalized duration.
    ///
    /// This constructor normalizes the duration by converting excess nanoseconds into seconds.
    ///
    /// Returns `None` if the attempt to convert excess nanoseconds causes `sec` to overflow.
    pub fn new_checked(sec: i32, nsec: u32) -> Option<Self> {
        normalize_nsec(nsec)
            .carry_i32(sec)
            .map(|(sec, nsec)| Self { sec, nsec })
    }

    /// Creates a new normalized duration.
    ///
    /// This constructor normalizes the duration by converting excess nanoseconds into seconds.
    ///
    /// Panics if the attempt to convert excess nanoseconds causes `sec` to overflow.
    pub fn new(sec: i32, nsec: u32) -> Self {
        Self::new_checked(sec, nsec).unwrap()
    }

    /// Returns the number of seconds in the duration.
    pub fn sec(&self) -> i32 {
        self.sec
    }

    /// Returns the number of fractional seconds in the duration, as nanoseconds.
    pub fn nsec(&self) -> u32 {
        self.nsec
    }

    /// Creates a `Duration` from `f64` seconds, or fails if the value is unrepresentable.
    pub fn try_from_secs_f64(secs: f64) -> Result<Self, RangeError> {
        if secs < f64::from(i32::MIN) {
            Err(RangeError::LowerBound)
        } else if secs >= f64::from(i32::MAX) + 1.0 {
            Err(RangeError::UpperBound)
        } else {
            let mut sec = secs as i32;
            let mut nsec = ((secs - f64::from(sec)) * 1e9) as i32;
            if nsec < 0 {
                sec -= 1;
                nsec += 1_000_000_000;
            }
            Ok(Self::new(
                sec,
                u32::try_from(nsec).unwrap_or_else(|e| {
                    unreachable!("expected {nsec} to be within [0, 1_000_000_000): {e}")
                }),
            ))
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
        Self {
            seconds: i64::from(v.sec),
            nanos: i32::try_from(v.nsec).unwrap_or_else(|e| {
                unreachable!("expected {} to be within [0, 1_000_000_000): {e}", v.nsec)
            }),
        }
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
/// let timestamp = Timestamp::new(1_548_054_420, 76_657_283);
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    /// Seconds since epoch.
    sec: u32,
    /// Additional nanoseconds since epoch.
    nsec: u32,
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

    /// Creates a new normalized timestamp.
    ///
    /// This constructor normalizes the timestamp by converting excess nanoseconds into seconds.
    ///
    /// Returns `None` if the attempt to convert excess nanoseconds causes `sec` to overflow.
    pub fn new_checked(sec: u32, nsec: u32) -> Option<Self> {
        normalize_nsec(nsec)
            .carry_u32(sec)
            .map(|(sec, nsec)| Self { sec, nsec })
    }

    /// Creates a new normalized timestamp.
    ///
    /// This constructor normalizes the timestamp by converting excess nanoseconds into seconds.
    ///
    /// Panics if the attempt to convert excess nanoseconds causes `sec` to overflow.
    pub fn new(sec: u32, nsec: u32) -> Self {
        Self::new_checked(sec, nsec).unwrap()
    }

    /// Returns the current timestamp using [`SystemTime::now`][std::time::SystemTime::now].
    pub fn now() -> Self {
        let now = std::time::SystemTime::now();
        Self::try_from(now).expect("timestamp out of range")
    }

    /// Returns the number of seconds in the timestamp.
    pub fn sec(&self) -> u32 {
        self.sec
    }

    /// Returns the number of fractional seconds in the timestamp, as nanoseconds.
    pub fn nsec(&self) -> u32 {
        self.nsec
    }

    /// Creates a `Timestamp` from seconds since epoch as an `f64`, or fails if the value is
    /// unrepresentable.
    pub fn try_from_epoch_secs_f64(secs: f64) -> Result<Self, RangeError> {
        if secs < 0.0 {
            Err(RangeError::LowerBound)
        } else if secs >= f64::from(u32::MAX) + 1.0 {
            Err(RangeError::UpperBound)
        } else {
            let sec = secs as u32;
            let nsec = ((secs - f64::from(sec)) * 1e9) as u32;
            Ok(Self::new(sec, nsec))
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
        Self {
            seconds: i64::from(v.sec),
            nanos: i32::try_from(v.nsec).unwrap_or_else(|e| {
                unreachable!("expected {} to be within [0, 1_000_000_000): {e}", v.nsec)
            }),
        }
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
        Ok(Self::new(sec, nsec))
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
