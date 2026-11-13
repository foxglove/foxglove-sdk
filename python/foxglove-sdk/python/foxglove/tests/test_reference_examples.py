"""Check that the examples in the Datasets reference use APIs that exist.

The reference has hand-written examples in the docstrings of the ``foxglove.datasets`` modules,
on the Datasets page, and on the ``foxglove.client`` page. Each example is parsed, its
``foxglove`` imports are resolved, and its calls are bound to the signatures of the functions
they name. An example that uses a removed function or a renamed parameter fails.
"""

import ast
import importlib
import inspect
import re
import textwrap
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest

client_module = pytest.importorskip("foxglove.client")
pytest.importorskip("torch")
pytest.importorskip("ray")
pytest.importorskip("av")

import foxglove.datasets as datasets  # noqa: E402
import foxglove.datasets.ray as ray_datasets  # noqa: E402
import foxglove.datasets.torch as torch_datasets  # noqa: E402
import foxglove.datasets.video as video_datasets  # noqa: E402

DOCS = Path(__file__).resolve().parents[2] / "docs"
DATASETS_PAGE = DOCS / "datasets" / "index.rst"
CLIENT_PAGE = DOCS / "api" / "client.rst"

_DIRECTIVE = re.compile(r"^( *)\.\. py:(?:method|class):: (?:\w+\.)?(\w+)")

# Names that stand for an object that an example uses without creating it.
_RECEIVERS: dict[str, type] = {
    "client": client_module.Client,
    "ds": torch_datasets.TorchDataset,
    "episode": datasets.EpisodeReader,
}
# The class of the object that a call returns, for names that an example assigns.
_RETURNS: dict[Any, type] = {
    client_module.Client: client_module.Client,
    torch_datasets.read_dataset: torch_datasets.TorchDataset,
}


def _docstring_examples() -> Iterator[tuple[str, str]]:
    for module in (datasets, ray_datasets, torch_datasets, video_datasets):
        names = getattr(module, "__all__", None) or [
            name
            for name, value in vars(module).items()
            if getattr(value, "__module__", None) == module.__name__
            and not name.startswith("_")
        ]
        for name in names:
            owner = getattr(module, name)
            documented = [(f"{module.__name__}.{name}", owner)]
            if inspect.isclass(owner):
                documented += [
                    (f"{module.__name__}.{name}.{attribute}", member)
                    for attribute, member in vars(owner).items()
                    if not attribute.startswith("_")
                ]
            for label, value in documented:
                _, marker, example = (inspect.getdoc(value) or "").partition(
                    "Example::"
                )
                if marker:
                    yield label, textwrap.dedent(example).strip("\n")


def _page_examples(page: Path) -> list[tuple[str, str]]:
    """The Python code blocks of a page, each with the name of the entry that contains it."""
    lines = page.read_text(encoding="utf-8").split("\n")
    examples = []
    owner, owner_indent = "", -1
    for index, line in enumerate(lines):
        indent = len(line) - len(line.lstrip())
        if line.strip() and indent <= owner_indent:
            owner, owner_indent = "", -1
        directive = _DIRECTIVE.match(line)
        if directive:
            owner, owner_indent = directive.group(2), len(directive.group(1))
        elif line.strip() == ".. code-block:: python":
            block = []
            for following in lines[index + 1 :]:
                if (
                    following.strip()
                    and len(following) - len(following.lstrip()) <= indent
                ):
                    break
                block.append(following)
            examples.append((owner, textwrap.dedent("\n".join(block)).strip("\n")))
    return examples


def _examples() -> list[tuple[str, str]]:
    examples = list(_docstring_examples())
    for page in (DATASETS_PAGE, CLIENT_PAGE):
        # The wheel test jobs copy this directory away from the docs sources.
        if page.exists():
            examples += [
                (f"{page.parent.name}/{page.name}:{owner or 'page'}", code)
                for owner, code in _page_examples(page)
            ]
    return examples


EXAMPLES = _examples()


def _api_calls(code: str) -> Iterator[tuple[ast.Call, Any, bool]]:
    """Yield each call to a foxglove function or method, with the callable it names.

    The last item is true for a method that is called on an object.
    """
    tree = ast.parse(code)
    imported: dict[str, Any] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom) and (node.module or "").startswith(
            "foxglove"
        ):
            module = importlib.import_module(node.module or "")
            for alias in node.names:
                imported[alias.asname or alias.name] = getattr(module, alias.name)

    receivers = dict(_RECEIVERS)
    for node in ast.walk(tree):
        if not isinstance(node, ast.Assign) or len(node.targets) != 1:
            continue
        target, value = node.targets[0], node.value
        if not isinstance(target, ast.Name):
            continue
        factory = None
        if isinstance(value, ast.Call) and isinstance(value.func, ast.Name):
            factory = imported.get(value.func.id)
        if factory in _RETURNS:
            receivers[target.id] = _RETURNS[factory]
        else:
            receivers.pop(target.id, None)

    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        function = node.func
        if isinstance(function, ast.Name) and function.id in imported:
            yield node, imported[function.id], False
        elif (
            isinstance(function, ast.Attribute)
            and isinstance(function.value, ast.Name)
            and function.value.id in receivers
        ):
            yield node, getattr(receivers[function.value.id], function.attr), True


@pytest.mark.parametrize(
    "code", [code for _, code in EXAMPLES], ids=[label for label, _ in EXAMPLES]
)
def test_example_calls_match_signatures(code: str) -> None:
    for call, target, is_method in _api_calls(code):
        arguments = [None] * (len(call.args) + int(is_method))
        keywords = {keyword.arg: None for keyword in call.keywords if keyword.arg}
        try:
            inspect.signature(target).bind(*arguments, **keywords)
        except TypeError as error:
            pytest.fail(
                f"{ast.unparse(call)} does not match {target.__qualname__}: {error}"
            )


def test_main_dataset_entries_have_an_example() -> None:
    labels = {label for label, _ in EXAMPLES}
    assert {
        "foxglove.datasets.DatasetSplit",
        "foxglove.datasets.inspect_splits",
        "foxglove.datasets.ray.read_dataset",
        "foxglove.datasets.torch.TorchDataset.set_epoch",
        "foxglove.datasets.torch.read_dataset",
        "foxglove.datasets.torch.to_image_tensor",
    } <= labels


@pytest.mark.skipif(
    not CLIENT_PAGE.exists(), reason="docs sources are not next to the tests"
)
def test_each_client_method_has_an_example_that_calls_it() -> None:
    page = CLIENT_PAGE.read_text(encoding="utf-8")
    examples = _page_examples(CLIENT_PAGE)
    methods = re.findall(r"^\.\. py:method:: Client\.(\w+)\(", page, flags=re.MULTILINE)
    assert methods
    for method in methods:
        calls = [
            call
            for owner, code in examples
            if owner == method
            for call, _, _ in _api_calls(code)
            if isinstance(call.func, ast.Attribute) and call.func.attr == method
        ]
        assert calls, f"the example of Client.{method} does not call it"
