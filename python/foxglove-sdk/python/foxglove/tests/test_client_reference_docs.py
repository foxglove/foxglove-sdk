"""Compare the hand-written foxglove.client reference page with the installed foxglove-client.

The page repeats method signatures from another package, so a foxglove-client release can
change a method without the page changing. These tests fail when that happens.
"""

import ast
import inspect
import re
from enum import Enum
from pathlib import Path
from typing import Any, NamedTuple

import pytest

client_module = pytest.importorskip("foxglove.client")

PAGE = Path(__file__).resolve().parents[2] / "docs" / "api" / "client.rst"

# The wheel test jobs copy this directory away from the docs sources.
pytestmark = pytest.mark.skipif(
    not PAGE.exists(), reason="docs sources are not next to the tests"
)

_CLASS = re.compile(r"^(?P<indent> *)\.\. py:class:: (?P<name>\w+)")
_METHOD = re.compile(
    r"^(?P<indent> *)\.\. py:method:: (?:(?P<owner>\w+)\.)?(?P<name>\w+)"
    r"\((?P<signature>.*)\)\s*$"
)
_PARAM = re.compile(r"^ *:param (?:\S+ )?(?P<name>\w+):\s*(?P<text>.*)$")
_STATED_DEFAULT = re.compile(r"^Optional, default ``(?P<value>[^`]+)``")

# (name, "positional" or "keyword", source text of the default or None)
Parameter = tuple[str, str, str | None]


class DocumentedMethod(NamedTuple):
    owner: str
    name: str
    signature: str
    param_fields: dict[str, str]


def _documented_methods() -> list[DocumentedMethod]:
    lines = PAGE.read_text(encoding="utf-8").split("\n")
    methods = []
    enclosing_class = ""
    for index, line in enumerate(lines):
        class_match = _CLASS.match(line)
        if class_match is not None:
            enclosing_class = class_match["name"]
        method_match = _METHOD.match(line)
        if method_match is None:
            continue
        indent = len(method_match["indent"])
        param_fields = {}
        for body_line in lines[index + 1 :]:
            if body_line.strip() and len(body_line) - len(body_line.lstrip()) <= indent:
                break
            param_match = _PARAM.match(body_line)
            if param_match is not None:
                param_fields[param_match["name"]] = param_match["text"]
        methods.append(
            DocumentedMethod(
                method_match["owner"] or enclosing_class,
                method_match["name"],
                method_match["signature"],
                param_fields,
            )
        )
    return methods


def _default_source(default: Any) -> str:
    if type(default) is object:
        # A private sentinel for "argument omitted". The page writes it as an ellipsis.
        return "..."
    if isinstance(default, Enum):
        return f"{type(default).__name__}.{default.name}"
    return repr(default)


def _actual_parameters(function: Any) -> list[Parameter]:
    parameters: list[Parameter] = []
    for parameter in list(inspect.signature(function).parameters.values())[1:]:
        kind = "keyword" if parameter.kind is parameter.KEYWORD_ONLY else "positional"
        default = (
            None
            if parameter.default is parameter.empty
            else _default_source(parameter.default)
        )
        parameters.append((parameter.name, kind, default))
    return parameters


def _documented_parameters(signature: str) -> list[Parameter]:
    function = ast.parse(f"def f({signature}): pass").body[0]
    assert isinstance(function, ast.FunctionDef)
    arguments = function.args
    positional = arguments.posonlyargs + arguments.args
    positional_defaults: list[ast.expr | None] = [None] * (
        len(positional) - len(arguments.defaults)
    )
    positional_defaults += arguments.defaults
    parameters: list[Parameter] = []
    for argument, default in zip(positional, positional_defaults):
        parameters.append(
            (
                argument.arg,
                "positional",
                None if default is None else ast.unparse(default),
            )
        )
    for argument, default in zip(arguments.kwonlyargs, arguments.kw_defaults):
        parameters.append(
            (argument.arg, "keyword", None if default is None else ast.unparse(default))
        )
    return parameters


METHODS = _documented_methods() if PAGE.exists() else []
METHOD_IDS = [f"{method.owner}.{method.name}" for method in METHODS]


def _client_parameters(method: DocumentedMethod) -> list[Parameter]:
    return _actual_parameters(
        getattr(getattr(client_module, method.owner), method.name)
    )


def test_page_parser_finds_the_client_methods() -> None:
    documented = {method.name for method in METHODS if method.owner == "Client"}
    assert {
        "create_dataset",
        "get_dataset_version_episodes",
        "update_episodes",
    } <= documented


@pytest.mark.parametrize("method", METHODS, ids=METHOD_IDS)
def test_signature_on_page_matches_client(method: DocumentedMethod) -> None:
    assert _documented_parameters(method.signature) == _client_parameters(method)


@pytest.mark.parametrize("method", METHODS, ids=METHOD_IDS)
def test_param_fields_match_client_signature(method: DocumentedMethod) -> None:
    actual = _client_parameters(method)
    assert list(method.param_fields) == [name for name, _, _ in actual]
    for name, _, default in actual:
        text = method.param_fields[name]
        if default is None:
            assert text.startswith("Required."), name
            continue
        assert text.startswith("Optional"), name
        stated = _STATED_DEFAULT.match(text)
        if stated is not None:
            stated_default = ast.unparse(ast.parse(stated["value"], mode="eval").body)
            assert stated_default == default, name
        else:
            # Only a default with no source form may be left unstated.
            assert default == "...", name
