use assert_matches::assert_matches;

use super::{normalize_nsec, Duration, NormalizeResult, Timestamp};
use crate::convert::{RangeError, SaturatingFrom};

#[test]
fn test_normalize_nsec() {
    // no overflow
    assert_eq!(normalize_nsec(0), NormalizeResult::Ok(0));
    assert_eq!(
        normalize_nsec(999_999_999),
        NormalizeResult::Ok(999_999_999)
    );

    // overflow
    assert_eq!(
        normalize_nsec(1_000_000_000),
        NormalizeResult::Overflow(1, 0)
    );
    assert_eq!(
        normalize_nsec(2_999_999_999),
        NormalizeResult::Overflow(2, 999_999_999)
    );
    assert_eq!(
        normalize_nsec(u32::MAX),
        NormalizeResult::Overflow(4, 294_967_295)
    );

    // no carry
    assert_eq!(normalize_nsec(1).carry_u32(1), Some((1, 1)));
    assert_eq!(normalize_nsec(1).carry_i32(-1), Some((-1, 1)));

    // carry
    assert_eq!(normalize_nsec(1_000_000_001).carry_u32(1), Some((2, 1)));
    assert_eq!(normalize_nsec(1_000_000_001).carry_i32(-1), Some((0, 1)));

    // out of range
    assert_eq!(normalize_nsec(1_000_000_001).carry_i32(i32::MAX), None);
    assert_eq!(normalize_nsec(1_000_000_001).carry_u32(u32::MAX), None);
}

#[test]
fn test_duration_normalization() {
    assert_eq!(
        Duration::new(0, 1_111_222_333),
        Duration {
            sec: 1,
            nsec: 111_222_333,
        }
    );
    assert_eq!(
        Duration::new(0, u32::MAX),
        Duration {
            sec: 4,
            nsec: 294_967_295,
        }
    );
    assert_eq!(
        Duration::new(-2, 1_000_000_001),
        Duration { sec: -1, nsec: 1 }
    );
    assert_eq!(
        Duration::new(i32::MIN, 1_000_000_001),
        Duration {
            sec: i32::MIN + 1,
            nsec: 1
        }
    );

    // overflow
    assert!(Duration::new_checked(i32::MAX, 1_000_000_000).is_none());
}

#[test]
fn test_duration_from_secs_f64() {
    // positive
    assert_eq!(
        Duration::try_from_secs_f64(1.618_033_989).unwrap(),
        Duration {
            sec: 1,
            nsec: 618_033_989
        }
    );

    // negative
    assert_eq!(
        Duration::try_from_secs_f64(-0.1).unwrap(),
        Duration {
            sec: -1,
            nsec: 900_000_000
        }
    );
    assert_eq!(
        Duration::try_from_secs_f64(-1.618_033_989).unwrap(),
        Duration {
            sec: -2,
            nsec: 381_966_011
        }
    );

    // min
    assert_eq!(
        Duration::try_from_secs_f64(i32::MIN.into()).unwrap(),
        Duration::MIN,
    );

    // nearly max
    assert_eq!(
        Duration::try_from_secs_f64(i32::MAX.into()).unwrap(),
        Duration {
            sec: i32::MAX,
            nsec: 0
        }
    );

    // fractional seconds beyond i32::MAX seconds are supported, but precision is limited.
    assert_matches!(
        Duration::try_from_secs_f64(f64::from(i32::MAX) + 0.1),
        Ok(_)
    );

    // out of range negative
    assert_matches!(
        Duration::try_from_secs_f64(f64::from(i32::MIN) - 0.1),
        Err(RangeError::LowerBound)
    );
    assert_eq!(
        Duration::saturating_from_secs_f64(f64::from(i32::MIN) - 0.1),
        Duration::MIN
    );

    // out of range positive
    assert_matches!(
        Duration::try_from_secs_f64(f64::from(i32::MAX) + 1.),
        Err(RangeError::UpperBound)
    );
    assert_eq!(
        Duration::saturating_from_secs_f64(f64::from(i32::MAX) + 1.),
        Duration::MAX
    );
}

#[test]
fn test_duration_from_std_duration() {
    let orig = std::time::Duration::from_millis(1234);
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: 1,
            nsec: 234_000_000,
        }
    );

    // min
    let orig = std::time::Duration::default();
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(dur, Duration::default());

    // max
    let orig = std::time::Duration::from_nanos(i32::MAX as u64 * 1_000_000_000 + 999_999_999);
    let dur = Duration::try_from(orig).unwrap();
    assert_eq!(
        dur,
        Duration {
            sec: i32::MAX,
            nsec: 999_999_999,
        }
    );

    // seconds out of range
    let orig = std::time::Duration::from_secs(i32::MAX as u64 + 1);
    assert_matches!(Duration::try_from(orig), Err(RangeError::UpperBound));
    assert_eq!(Duration::saturating_from(orig), Duration::MAX);
}

#[test]
fn test_timestamp_normalization() {
    assert_eq!(
        Timestamp::new(0, 1_111_222_333),
        Timestamp {
            sec: 1,
            nsec: 111_222_333
        }
    );
    assert_eq!(
        Timestamp::new(0, u32::MAX),
        Timestamp {
            sec: 4,
            nsec: 294_967_295
        }
    );
    assert!(Timestamp::new_checked(u32::MAX, 1_000_000_000).is_none());
}

#[test]
fn test_timestamp_from_epoch_secs_f64() {
    assert_eq!(
        Timestamp::try_from_epoch_secs_f64(1.618_033_989).unwrap(),
        Timestamp {
            sec: 1,
            nsec: 618_033_989
        }
    );

    // min
    assert_eq!(
        Timestamp::try_from_epoch_secs_f64(0.0).unwrap(),
        Timestamp::MIN,
    );

    // nearly max
    assert_eq!(
        Timestamp::try_from_epoch_secs_f64(u32::MAX.into()).unwrap(),
        Timestamp {
            sec: u32::MAX,
            nsec: 0
        }
    );

    // fractional seconds beyond u32::MAX seconds are supported, but precision is limited.
    assert_matches!(
        Timestamp::try_from_epoch_secs_f64(f64::from(u32::MAX) + 0.1),
        Ok(_)
    );

    // too past
    assert_matches!(
        Timestamp::try_from_epoch_secs_f64(-0.1),
        Err(RangeError::LowerBound)
    );
    assert_eq!(
        Timestamp::saturating_from_epoch_secs_f64(-0.1),
        Timestamp::MIN
    );

    // too future
    assert_matches!(
        Timestamp::try_from_epoch_secs_f64(f64::from(u32::MAX) + 1.),
        Err(RangeError::UpperBound)
    );
    assert_eq!(
        Timestamp::saturating_from_epoch_secs_f64(f64::from(u32::MAX) + 1.),
        Timestamp::MAX
    );
}

#[test]
fn test_timestamp_from_system_time() {
    // min
    let orig = std::time::UNIX_EPOCH;
    let ts = Timestamp::try_from(orig).unwrap();
    assert_eq!(ts, Timestamp::default());

    // max
    let orig = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_nanos(
            u64::from(u32::MAX) * 1_000_000_000 + 999_999_999,
        ))
        .unwrap();
    let ts = Timestamp::try_from(orig).unwrap();
    assert_eq!(
        ts,
        Timestamp {
            sec: u32::MAX,
            nsec: 999_999_999,
        }
    );

    // too past
    let orig = std::time::UNIX_EPOCH
        .checked_sub(std::time::Duration::from_nanos(1))
        .unwrap();
    assert_matches!(Timestamp::try_from(orig), Err(RangeError::LowerBound));
    assert_eq!(Timestamp::saturating_from(orig), Timestamp::MIN);

    // too future
    let orig = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(u64::from(u32::MAX) + 1))
        .unwrap();
    assert_matches!(Timestamp::try_from(orig), Err(RangeError::UpperBound));
    assert_eq!(Timestamp::saturating_from(orig), Timestamp::MAX);
}

#[test]
fn test_timestamp_now() {
    let now = std::time::SystemTime::now();
    let before = Timestamp::try_from(now).unwrap();

    let now = Timestamp::now();

    assert!(now >= before);
    assert!(now.nsec() < 1_000_000_000);
}
