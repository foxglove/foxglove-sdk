# Configure pytest to skip tests marked with `benchmark` unless a `--with-benchmarks` flag is
# provided. The `benchmark` marker is defined by `pytest_benchmark`.
# - https://docs.pytest.org/en/stable/example/simple.html#control-skipping-of-tests-according-to-command-line-option # noqa: E501
#
# In order to define the option flag, this file must be in the root of the project.

from typing import List

import pytest

option_flag = "--with-benchmarks"


def pytest_addoption(parser: pytest.Parser) -> None:
    parser.addoption(
        option_flag, action="store_true", default=False, help="run benchmarks"
    )


def pytest_collection_modifyitems(
    config: pytest.Config, items: List[pytest.Item]
) -> None:
    if config.getoption(option_flag):
        return
    if config.getoption("-m") == "benchmark":  # running only benchmark tests
        return
    skip_marker = pytest.mark.skip(reason=f"need {option_flag} option to run")
    for item in items:
        if "benchmark" in item.keywords:
            item.add_marker(skip_marker)
