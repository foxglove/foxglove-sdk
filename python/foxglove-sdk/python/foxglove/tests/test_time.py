import datetime

import pytest
from foxglove.schemas import Duration, Timestamp


def test_duration_normalization() -> None:
    assert Duration(sec=0, nsec=1_111_222_333) == Duration(sec=1, nsec=111_222_333)
    assert Duration(sec=0, nsec=2**32 - 1) == Duration(sec=4, nsec=294_967_295)
    assert Duration(sec=-2, nsec=1_000_000_001) == Duration(sec=-1, nsec=1)
    assert Duration(sec=-(2**31), nsec=1_000_000_001) == Duration(
        sec=-(2**31) + 1, nsec=1
    )

    # argument conversions
    d = Duration(sec=-(2**31))
    assert d.sec == -(2**31)
    assert d.nsec == 0

    d = Duration(sec=0, nsec=2**32 - 1)
    assert d.sec == 4
    assert d.nsec == 294_967_295

    d = Duration(sec=2**31 - 1, nsec=999_999_999)
    assert d.sec == 2**31 - 1
    assert d.nsec == 999_999_999

    with pytest.raises(OverflowError):
        Duration(sec=-(2**31) - 1)
    with pytest.raises(OverflowError):
        Duration(sec=2**31)
    with pytest.raises(OverflowError):
        Duration(sec=0, nsec=-1)
    with pytest.raises(OverflowError):
        Duration(sec=0, nsec=2**32)

    # overflow past upper bound
    with pytest.raises(OverflowError):
        Duration(sec=2**31 - 1, nsec=1_000_000_000)

    # we don't handle this corner case, where seconds is beyond the lower
    # bound, but nanoseconds overflow to bring the duration within range.
    with pytest.raises(OverflowError):
        Duration(sec=-(2**31) - 1, nsec=1_000_000_000)


def test_duration_from_secs() -> None:
    assert Duration.from_secs(1.123) == Duration(sec=1, nsec=123_000_000)
    assert Duration.from_secs(-0.123) == Duration(sec=-1, nsec=877_000_000)
    assert Duration.from_secs(-1.123) == Duration(sec=-2, nsec=877_000_000)

    with pytest.raises(OverflowError):
        Duration.from_secs(-1e42)

    with pytest.raises(OverflowError):
        Duration.from_secs(1e42)


def test_duration_from_timedelta() -> None:
    td = datetime.timedelta(seconds=1, milliseconds=123)
    assert Duration.from_timedelta(td) == Duration(sec=1, nsec=123_000_000)

    # no loss of precision
    td = datetime.timedelta(days=9876, microseconds=123_456)
    assert Duration.from_timedelta(td) == Duration(sec=853_286_400, nsec=123_456_000)

    # timedeltas are normalized
    td = datetime.timedelta(seconds=8 * 24 * 3600, milliseconds=99_111)
    assert Duration.from_timedelta(td) == Duration(sec=691_299, nsec=111_000_000)

    with pytest.raises(OverflowError):
        Duration.from_timedelta(datetime.timedelta.min)

    with pytest.raises(OverflowError):
        Duration.from_timedelta(datetime.timedelta.max)


def test_timestamp_normalization() -> None:
    assert Timestamp(sec=0, nsec=1_111_222_333) == Timestamp(sec=1, nsec=111_222_333)
    assert Timestamp(sec=0, nsec=2**32 - 1) == Timestamp(sec=4, nsec=294_967_295)

    # argument conversions
    t = Timestamp(sec=0)
    assert t.sec == 0
    assert t.nsec == 0

    t = Timestamp(sec=0, nsec=2**32 - 1)
    assert t.sec == 4
    assert t.nsec == 294_967_295

    t = Timestamp(sec=2**32 - 1, nsec=999_999_999)
    assert t.sec == 2**32 - 1
    assert t.nsec == 999_999_999

    with pytest.raises(OverflowError):
        Timestamp(sec=-1)
    with pytest.raises(OverflowError):
        Timestamp(sec=2**32)
    with pytest.raises(OverflowError):
        Timestamp(sec=0, nsec=-1)
    with pytest.raises(OverflowError):
        Timestamp(sec=0, nsec=2**32)

    # overflow past upper bound
    with pytest.raises(OverflowError):
        Timestamp(sec=2**32 - 1, nsec=1_000_000_000)


def test_timestamp_from_epoch_secs() -> None:
    assert Timestamp.from_epoch_secs(1.123) == Timestamp(sec=1, nsec=123_000_000)

    with pytest.raises(OverflowError):
        Timestamp.from_epoch_secs(-1.0)

    with pytest.raises(OverflowError):
        Timestamp.from_epoch_secs(1e42)


def test_timestamp_from_datetime() -> None:
    utc = datetime.timezone.utc
    dt = datetime.datetime(1970, 1, 1, tzinfo=utc)
    assert Timestamp.from_datetime(dt) == Timestamp(sec=0)

    # no loss of precision
    dt = datetime.datetime(2025, 1, 1, microsecond=42, tzinfo=utc)
    assert Timestamp.from_datetime(dt) == Timestamp(sec=1_735_689_600, nsec=42_000)

    # alternative timezone
    local_tz = datetime.timezone(datetime.timedelta(hours=-1))
    dt = datetime.datetime(1970, 1, 1, 0, 0, 1, 123_000, tzinfo=local_tz)
    assert Timestamp.from_datetime(dt) == Timestamp(sec=3601, nsec=123_000_000)

    with pytest.raises(OverflowError):
        Timestamp.from_datetime(datetime.datetime(1969, 12, 31, tzinfo=utc))

    with pytest.raises(OverflowError):
        Timestamp.from_datetime(datetime.datetime(2106, 2, 8, tzinfo=utc))
