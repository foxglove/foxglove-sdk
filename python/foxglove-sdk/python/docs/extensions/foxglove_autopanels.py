"""
Sphinx extension for generating documentation pages for panel classes.

Automatically discovers classes ending with "-Panel", and groups them with other classes matching
the same prefix. A separate page is generated for each group, and a toctree is placed where the
``foxglove_autopanels`` directive is used.

Usage in RST::

    .. foxglove_autopanels::
"""

import shutil
from dataclasses import dataclass, field
from pathlib import Path

import foxglove.layouts
from docutils import nodes
from sphinx.application import Sphinx
from sphinx.config import Config
from sphinx.util import logging
from sphinx.util.docutils import SphinxDirective

logger = logging.getLogger(__name__)


@dataclass
class PanelGroup:
    """A panel class and its related configuration and helper classes."""

    panel: str
    config: str | None = None
    related_types: set[str] = field(default_factory=set)


# Classes to exclude from panel documentation (either internal or documented elsewhere)
EXCLUDED_CLASSES = {
    # Python builtins that show up in dir()
    "ABC",
    "Any",
    # Internal base classes
    "BaseRendererConfig",
    "_BaseModel",
    # Documented manually in notebook/index.rst
    "Layout",
    "BasePanel",
    "Panel",
    "SplitContainer",
    "SplitItem",
    "StackContainer",
    "StackItem",
    "TabContainer",
    "TabItem",
    "UserScript",
}
# Map of panel base name -> prefix for related classes that don't follow the standard naming
RELATED_PREFIXES_BY_PANEL_BASENAME = {"ThreeDee": ["BaseRenderer"]}


def _get_panel_groups() -> dict[str, PanelGroup]:
    """Discover panel classes and group related classes."""

    all_class_names = {
        name
        for name, cls in foxglove.layouts.__dict__.items()
        if isinstance(cls, type) and name not in EXCLUDED_CLASSES
    }
    documented_names: set[str] = set()

    panel_groups: dict[str, PanelGroup] = {}

    for panel_name, panel_cls in foxglove.layouts.__dict__.items():
        if panel_name in EXCLUDED_CLASSES:
            continue
        if not panel_name.endswith("Panel"):
            continue
        if not isinstance(panel_cls, type):
            raise RuntimeError(
                f"[foxglove_autopanels] Expected foxglove.layouts.{panel_name} to be a class"
            )

        panel_basename = panel_name.removesuffix("Panel")
        config_name = panel_basename + "Config"
        config_cls = getattr(foxglove.layouts, config_name, None)
        if config_cls is not None and not isinstance(config_cls, type):
            raise RuntimeError(
                f"[foxglove_autopanels] Expected foxglove.layouts.{config_name} to be a class"
            )
        related_types = {
            name
            for name, cls in foxglove.layouts.__dict__.items()
            if name not in EXCLUDED_CLASSES
            and isinstance(cls, type)
            and name != panel_name
            and name != config_name
            and (
                name.startswith(panel_basename)
                or any(
                    name.startswith(prefix)
                    for prefix in RELATED_PREFIXES_BY_PANEL_BASENAME.get(
                        panel_basename, []
                    )
                )
            )
        }

        documented_names.add(panel_name)
        if config_cls is not None:
            documented_names.add(config_name)
        documented_names |= related_types

        panel_groups[panel_basename] = PanelGroup(
            panel=panel_name,
            config=config_name if config_cls is not None else None,
            related_types=related_types,
        )

    # Warn about any classes that are neither excluded nor documented
    undocumented_names = all_class_names - documented_names
    if undocumented_names:
        raise RuntimeError(
            f"[foxglove_autopanels] Documentation was not generated for the following classes: "
            f"{sorted(undocumented_names)}. This indiates either an error in the "
            f"foxglove_autopanels extension or missing association between panel names "
            f"and related type names."
        )

    return panel_groups


PANEL_GROUPS = _get_panel_groups()


def _make_autoclass_rst(class_name: str) -> str:
    return f"""\
.. autoclass:: foxglove.layouts.{class_name}
   :members:
   :undoc-members:
   :inherited-members:
   :exclude-members: __init__, to_json
"""


def _make_panel_group_rst(base_name: str, group: PanelGroup) -> str:
    """Generate RST content for the types in a panel group."""
    content = f"""\
:tocdepth: 2

{base_name}
{"=" * len(base_name)}

{_make_autoclass_rst(group.panel)}
"""

    if group.config:
        content += _make_autoclass_rst(group.config)

    for helper in sorted(group.related_types):
        content += _make_autoclass_rst(helper)

    return content


def generate_panel_pages(app: Sphinx, _config: Config) -> None:
    """Generate a RST file documenting the classes in each group."""
    output_dir = Path(app.srcdir) / "notebook" / "panels"

    shutil.rmtree(output_dir, ignore_errors=True)
    output_dir.mkdir(parents=True, exist_ok=True)

    for basename, group in PANEL_GROUPS.items():
        filename = output_dir / f"{basename}.rst"
        filename.write_text(_make_panel_group_rst(basename, group))
        logger.info(f"[foxglove_autopanels] Generated {filename}")


class AutoPanelsDirective(SphinxDirective):
    """
    Directive to create a toctree for auto-generated panel documentation.

    Usage: ``.. foxglove_autopanels::``
    """

    has_content = False
    required_arguments = 0
    optional_arguments = 0

    def run(self) -> list[nodes.Node]:
        toctree_rst = ".. toctree::\n   :maxdepth: 1\n\n"
        for basename in sorted(PANEL_GROUPS.keys()):
            toctree_rst += f"   panels/{basename}\n"

        return self.parse_text_to_nodes(toctree_rst)
