"""Keep hand-written client signatures in sync with the installed foxglove-client."""

import ast
import inspect
import re
from enum import Enum
from pathlib import Path
from typing import Any

import pytest

client = pytest.importorskip("foxglove.client")
PAGE = Path(__file__).resolve().parents[2] / "docs" / "api" / "client.rst"
pytestmark = pytest.mark.skipif(not PAGE.exists(), reason="docs sources unavailable")


def _methods() -> list[tuple[str, str, str]]:
    methods = []
    owner = ""
    for line in PAGE.read_text(encoding="utf-8").splitlines() if PAGE.exists() else []:
        cls = re.match(r" *\.\. py:class:: (\w+)", line)
        if cls:
            owner = cls[1]
        method = re.match(r" *\.\. py:method:: (?:(\w+)\.)?(\w+)\((.*)\)$", line)
        if method:
            methods.append((method[1] or owner, method[2], method[3]))
    return methods


def _default(value: Any) -> str:
    if type(value) is object:
        return "..."  # Private sentinel for an omitted argument.
    if isinstance(value, Enum):
        return f"{type(value).__name__}.{value.name}"
    return repr(value)


@pytest.mark.parametrize("owner,name,signature", _methods())
def test_client_signature(owner: str, name: str, signature: str) -> None:
    function = ast.parse(f"def f({signature}): pass").body[0]
    assert isinstance(function, ast.FunctionDef)
    args = function.args
    documented = [
        (arg.arg, inspect.Parameter.POSITIONAL_OR_KEYWORD, default)
        for arg, default in zip(
            args.args,
            [None] * (len(args.args) - len(args.defaults))
            + [ast.unparse(value) for value in args.defaults],
        )
    ] + [
        (
            arg.arg,
            inspect.Parameter.KEYWORD_ONLY,
            None if default is None else ast.unparse(default),
        )
        for arg, default in zip(args.kwonlyargs, args.kw_defaults)
    ]
    actual = [
        (
            param.name,
            param.kind,
            (
                None
                if param.default is inspect.Parameter.empty
                else _default(param.default)
            ),
        )
        for param in list(
            inspect.signature(getattr(getattr(client, owner), name)).parameters.values()
        )[1:]
    ]
    assert not (args.posonlyargs or args.vararg or args.kwarg)
    assert documented == actual
