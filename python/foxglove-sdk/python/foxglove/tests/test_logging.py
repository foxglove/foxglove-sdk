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


def test_logging_config_with_env() -> None:
    # Run a script in a child process so logger can be re-initialized from env.
    test_script = """
import logging
import foxglove

server = foxglove.start_server(port=0)
server.stop()

print("test_init_with_env_complete")
"""

    # Default: logging is disabled unless enabled by the user or environment.
    env = os.environ.copy()
    env.pop("FOXGLOVE_LOG_LEVEL", None)

    result = subprocess.run(
        [sys.executable, "-c", test_script],
        env=env,
        capture_output=True,
        text=True,
        timeout=5,
    )
    assert "test_init_with_env_complete" in result.stdout
    assert "Started server" not in result.stderr

    test_script = """
import logging
import foxglove

foxglove.set_log_level("DEBUG")

server = foxglove.start_server(port=0)
server.stop()

print("test_init_with_env_complete")
"""

    # set_log_level enables Foxglove SDK logs by default.
    env = os.environ.copy()
    env.pop("FOXGLOVE_LOG_LEVEL", None)

    result = subprocess.run(
        [sys.executable, "-c", test_script],
        env=env,
        capture_output=True,
        text=True,
        timeout=5,
    )
    assert "test_init_with_env_complete" in result.stdout
    assert "Started server" in result.stderr

    # Environment filters take precedence over set_log_level.
    env = os.environ.copy()
    env["FOXGLOVE_LOG_LEVEL"] = "debug,foxglove::websocket::server=warn"

    result = subprocess.run(
        [sys.executable, "-c", test_script],
        env=env,
        capture_output=True,
        text=True,
        timeout=5,
    )
    assert "test_init_with_env_complete" in result.stdout
    assert "Started server" not in result.stderr
