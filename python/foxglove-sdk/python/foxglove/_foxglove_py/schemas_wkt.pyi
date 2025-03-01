import datetime
from typing import Optional

class Duration:
    """
    A duration in seconds and nanoseconds
    """

    def __new__(
        cls,
        sec: int,
        nsec: Optional[int] = None,
    ) -> "Duration": ...
    @staticmethod
    def from_secs(secs: float) -> "Duration":
        """
        Creates a :py:class:`Duration` from seconds.

        Raises `OverflowError` if the duration cannot be represented.

        :param secs: Seconds
        :type secs: float
        :rtype: :py:class:`Duration`
        """
        ...

    @staticmethod
    def from_timedelta(td: datetime.timedelta) -> "Duration":
        """
        Creates a :py:class:`Duration` from a timedelta.

        Raises `OverflowError` if the duration cannot be represented.

        :param td: Timedelta
        :type td: :py:class:`datetime.timedelta`
        :rtype: :py:class:`Duration`
        """
        ...

class Timestamp:
    """
    A timestamp in seconds and nanoseconds
    """

    def __new__(
        cls,
        sec: int,
        nsec: Optional[int] = None,
    ) -> "Timestamp": ...
    @staticmethod
    def from_epoch_secs(timestamp: float) -> "Timestamp":
        """
        Creates a :py:class:`Timestamp` from an epoch timestamp, such as is
        returned by :py:func:`time.time` or
        :py:func:`datetime.datetime.timestamp`.

        Raises `OverflowError` if the timestamp cannot be represented.

        :param timestamp: Seconds since epoch
        :type timestamp: float
        :rtype: :py:class:`Timestamp`
        """
        ...

    @staticmethod
    def from_datetime(dt: datetime.datetime) -> "Timestamp":
        """
        Creates a UNIX epoch :py:class:`Timestamp` from a datetime object.

        Naive datetime objects are presumed to be in the local timezone.

        Raises `OverflowError` if the timestamp cannot be represented.

        :param dt: Datetime
        :type dt: :py:class:`datetime.datetime`
        :rtype: :py:class:`Timestamp`
        """
        ...
