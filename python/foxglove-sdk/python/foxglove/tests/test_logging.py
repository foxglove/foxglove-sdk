import logging

import pytest
from foxglove import set_log_level


def test_set_log_level_accepts_string_or_int() -> None:
    set_log_level("DEBUG")
    set_log_level(logging.DEBUG)
    with pytest.raises(ValueError):
        set_log_level("debug")


def test_set_log_level_clamps_illegal_values() -> None:
    set_log_level(-1)
    set_log_level(2**64)
