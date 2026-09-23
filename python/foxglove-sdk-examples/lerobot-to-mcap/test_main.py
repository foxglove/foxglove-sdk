from argparse import ArgumentTypeError
from datetime import datetime, timedelta, timezone

import pytest
from main import parse_episodes, parse_start_time


@pytest.mark.parametrize(
    ("spec", "expected"),
    [("3", {3}), ("0,2", {0, 2}), ("1-3", {1, 2, 3}), (" 0, 5-7 ,9", {0, 5, 6, 7, 9})],
)
def test_parses_episode_selections(spec: str, expected: set[int]) -> None:
    assert parse_episodes(spec) == expected


@pytest.mark.parametrize("spec", ["1-x", "5-3", "", " , "])
def test_rejects_malformed_or_empty_episode_selections(spec: str) -> None:
    with pytest.raises(ArgumentTypeError):
        parse_episodes(spec)


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("2021-06-01T12:00:00Z", datetime(2021, 6, 1, 12, tzinfo=timezone.utc)),
        ("2021-06-01T12:00:00", datetime(2021, 6, 1, 12)),
        (
            "2021-06-01T14:00:00+02:00",
            datetime(2021, 6, 1, 14, tzinfo=timezone(timedelta(hours=2))),
        ),
    ],
)
def test_parses_iso_8601_start_times(value: str, expected: datetime) -> None:
    assert parse_start_time(value) == expected


def test_rejects_start_times_that_are_not_iso_8601() -> None:
    with pytest.raises(ArgumentTypeError):
        parse_start_time("yesterday")
