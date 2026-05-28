import logging
import os
import subprocess
import sys

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


def _run_logging_script(
    test_script: str, env: dict[str, str]
) -> subprocess.CompletedProcess[str]:
    # Run a script in a child process so logger can be re-initialized from env.
    result = subprocess.run(
        [sys.executable, "-c", test_script],
        env=env,
        capture_output=True,
        text=True,
        timeout=5,
    )
    assert "test_init_with_env_complete" in result.stdout
    return result


START_SERVER_SCRIPT = """
import foxglove

server = foxglove.start_server(port=0)
server.stop()

print("test_init_with_env_complete")
"""


START_SERVER_WITH_SET_LOG_LEVEL_SCRIPT = """
import foxglove

foxglove.set_log_level("INFO")

server = foxglove.start_server(port=0)
server.stop()

print("test_init_with_env_complete")
"""


def test_logging_disabled_by_default() -> None:
    # Default: logging is disabled unless enabled by the user or environment.
    env = os.environ.copy()
    env.pop("FOXGLOVE_LOG_LEVEL", None)

    result = _run_logging_script(START_SERVER_SCRIPT, env)
    assert "Started server" not in result.stderr
    assert "Creating tokio runtime" not in result.stderr


def test_set_log_level_enables_logging() -> None:
    # Set explicitly to INFO in script
    env = os.environ.copy()
    env.pop("FOXGLOVE_LOG_LEVEL", None)

    result = _run_logging_script(START_SERVER_WITH_SET_LOG_LEVEL_SCRIPT, env)
    assert "Started server" in result.stderr
    assert "Creating tokio runtime" not in result.stderr


def test_foxglove_log_level_info_enables_logging() -> None:
    env = os.environ.copy()
    env["FOXGLOVE_LOG_LEVEL"] = "info"

    result = _run_logging_script(START_SERVER_SCRIPT, env)
    assert "Started server" in result.stderr
    assert "Creating tokio runtime" not in result.stderr


def test_foxglove_log_level_debug_enables_logging() -> None:
    env = os.environ.copy()
    env["FOXGLOVE_LOG_LEVEL"] = "warn,foxglove=debug"

    result = _run_logging_script(START_SERVER_SCRIPT, env)
    assert "Started server" in result.stderr
    assert "Creating tokio runtime" in result.stderr


def test_foxglove_log_level_takes_precedence_over_set_log_level() -> None:
    # Environment filters take precedence over set_log_level.
    env = os.environ.copy()
    env["FOXGLOVE_LOG_LEVEL"] = "debug,foxglove=warn"

    result = _run_logging_script(START_SERVER_WITH_SET_LOG_LEVEL_SCRIPT, env)
    assert "Started server" not in result.stderr
    assert "Creating tokio runtime" not in result.stderr
