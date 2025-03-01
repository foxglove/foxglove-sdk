import datetime
import unittest

from foxglove.schemas import Duration, Timestamp


class TestTime(unittest.TestCase):
    def test_duration_from_secs(self) -> None:
        self.assertEqual(Duration.from_secs(1.123), Duration(sec=1, nsec=123_000_000))
        self.assertEqual(Duration.from_secs(-0.123), Duration(sec=-1, nsec=877_000_000))
        self.assertEqual(Duration.from_secs(-1.123), Duration(sec=-2, nsec=877_000_000))

        with self.assertRaises(OverflowError):
            Duration.from_secs(-1e42)

        with self.assertRaises(OverflowError):
            Duration.from_secs(1e42)

    def test_duration_from_timedelta(self) -> None:
        td = datetime.timedelta(seconds=1, milliseconds=123)
        self.assertEqual(Duration.from_timedelta(td), Duration(sec=1, nsec=123_000_000))

        # no loss of precision
        td = datetime.timedelta(days=9876, microseconds=123_456)
        self.assertEqual(
            Duration.from_timedelta(td), Duration(sec=853_286_400, nsec=123_456_000)
        )

        # timedeltas are normalized
        td = datetime.timedelta(seconds=8 * 24 * 3600, milliseconds=99_111)
        self.assertEqual(
            Duration.from_timedelta(td), Duration(sec=691_299, nsec=111_000_000)
        )

        with self.assertRaises(OverflowError):
            Duration.from_timedelta(datetime.timedelta.min)

        with self.assertRaises(OverflowError):
            Duration.from_timedelta(datetime.timedelta.max)

    def test_timestamp_from_epoch_secs(self) -> None:
        self.assertEqual(
            Timestamp.from_epoch_secs(1.123), Timestamp(sec=1, nsec=123_000_000)
        )

        with self.assertRaises(OverflowError):
            Timestamp.from_epoch_secs(-1.0)

        with self.assertRaises(OverflowError):
            Timestamp.from_epoch_secs(1e42)

    def test_timestamp_from_datetime(self) -> None:
        utc = datetime.timezone.utc
        dt = datetime.datetime(1970, 1, 1, tzinfo=utc)
        self.assertEqual(Timestamp.from_datetime(dt), Timestamp(sec=0))

        # no loss of precision
        dt = datetime.datetime(2025, 1, 1, microsecond=42, tzinfo=utc)
        self.assertEqual(
            Timestamp.from_datetime(dt), Timestamp(sec=1_735_689_600, nsec=42_000)
        )

        # alternative timezone
        local_tz = datetime.timezone(datetime.timedelta(hours=-1))
        dt = datetime.datetime(1970, 1, 1, 0, 0, 1, 123_000, tzinfo=local_tz)
        self.assertEqual(
            Timestamp.from_datetime(dt), Timestamp(sec=3601, nsec=123_000_000)
        )

        with self.assertRaises(OverflowError):
            Timestamp.from_datetime(datetime.datetime(1969, 12, 31, tzinfo=utc))

        with self.assertRaises(OverflowError):
            Timestamp.from_datetime(datetime.datetime(2106, 2, 8, tzinfo=utc))
