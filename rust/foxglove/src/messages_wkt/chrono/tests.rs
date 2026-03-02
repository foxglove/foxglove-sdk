use assert_matches::assert_matches;

use super::{Duration, Timestamp};
use crate::convert::{RangeError, SaturatingFrom};

#[test]
fn test_duration_from_chrono_time_delta() {
    // positive
    let orig = chrono::TimeDelta::new(1, 234_000_000).unwrap();
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: 1,
            nsec: 234_000_000
        }
    );

    // negative
    let orig = chrono::TimeDelta::new(-2, 345_000_000).unwrap();
    assert_eq!(orig.num_nanoseconds().unwrap(), -1_655_000_000);
    assert_eq!(orig.num_seconds(), -1);
    assert_eq!(orig.subsec_nanos(), -655_000_000);
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: -2,
            nsec: 345_000_000
        }
    );

    // max
    let orig = chrono::TimeDelta::new(i32::MAX as i64, 999_999_999).unwrap();
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: i32::MAX,
            nsec: 999_999_999,
        }
    );

    // min
    let orig = chrono::TimeDelta::new(i32::MIN as i64, 0).unwrap();
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: i32::MIN,
            nsec: 0,
        }
    );

    // can't construct timedelta with more than 999_999_999 nanos.
    assert_matches!(chrono::TimeDelta::new(0, 1_000_000_000), None);

    // seconds out of range, high
    let orig = chrono::TimeDelta::new(i32::MAX as i64 + 1, 0).unwrap();
    assert_eq!(orig.num_seconds(), i32::MAX as i64 + 1);
    assert_matches!(Duration::try_from(orig), Err(RangeError::UpperBound));
    assert_eq!(Duration::saturating_from(orig), Duration::MAX);

    // seconds out of range, low
    let orig = chrono::TimeDelta::new(i32::MIN as i64 - 1, 0).unwrap();
    assert_eq!(orig.num_seconds(), i32::MIN as i64 - 1);
    assert_matches!(Duration::try_from(orig), Err(RangeError::LowerBound));
    assert_eq!(Duration::saturating_from(orig), Duration::MIN);

    // rounded seconds within range, but knocked out of range by nanos
    let orig = chrono::TimeDelta::new(i32::MIN as i64 - 1, 999_999_999).unwrap();
    assert_eq!(orig.num_seconds(), i32::MIN as i64);
    assert_eq!(orig.subsec_nanos(), -1);
    assert_matches!(Duration::try_from(orig), Err(RangeError::LowerBound));
    assert_eq!(Duration::saturating_from(orig), Duration::MIN);
}

#[test]
fn test_timestamp_from_datetime_utc() {
    let orig = chrono::DateTime::from_timestamp_nanos(123_456_789_000);
    let ts = Timestamp::try_from(orig).unwrap();
    assert_eq!(
        ts,
        Timestamp {
            sec: 123,
            nsec: 456_789_000
        }
    );

    // min
    let orig = chrono::DateTime::from_timestamp_nanos(0);
    let ts = Timestamp::try_from(orig).unwrap();
    assert_eq!(ts, Timestamp::default());

    // max
    let orig =
        chrono::DateTime::from_timestamp_nanos(u32::MAX as i64 * 1_000_000_000 + 999_999_999);
    let ts = Timestamp::try_from(orig).unwrap();
    assert_eq!(
        ts,
        Timestamp {
            sec: u32::MAX,
            nsec: 999_999_999,
        }
    );

    // too future
    let orig = chrono::DateTime::from_timestamp_nanos((u32::MAX as i64 + 1) * 1_000_000_000);
    assert_matches!(Timestamp::try_from(orig), Err(RangeError::UpperBound));
    assert_eq!(Timestamp::saturating_from(orig), Timestamp::MAX);

    // too past
    let orig = chrono::DateTime::from_timestamp_nanos(-1);
    assert_matches!(Timestamp::try_from(orig), Err(RangeError::LowerBound));
    assert_eq!(Timestamp::saturating_from(orig), Timestamp::MIN);
}
