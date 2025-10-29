"""
Layout types for Foxglove panels and layouts.

This module contains type definitions for panel configurations, panels,
tabs, items, item lists, and complete layouts used in Foxglove Studio.
"""

from __future__ import annotations

import json
import random
import string
from typing import Any, Literal

from typing_extensions import TypeAlias

Variables: TypeAlias = dict[str, Any]


def random_id() -> str:
    return "".join(random.choices(string.ascii_lowercase + string.digits, k=7))


class Panel:
    """
    A panel in a layout.

    :param id: Unique identifier for the panel
    :param type: Type of the panel (e.g., "Plot", "3D", "Raw Messages")
    :param config: Configuration dictionary for the panel
    """

    def __init__(self, type: str, id: str | None = None, **panel_config: Any) -> None:
        self.id = id if id is not None else f"{type}!{random_id()}"
        self.type = type
        self.config = panel_config

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "type": self.type,
            # filter out None values from the config
            "config": {k: v for k, v in self.config.items() if v is not None},
        }


class Tab:
    """
    A tab containing panels or other content.

    :param id: Unique identifier for the tab
    :param name: Display name for the tab
    :param items: List of items (panels, tabs, or item lists) in this tab
    """

    def __init__(self, name: str, items: ItemList, id: str | None = None) -> None:
        self.id = id if id is not None else f"Tab!{random_id()}"
        self.name = name
        self.items = items

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "items": self.items.to_dict(),
        }


class Tabs:
    """
    A collection of tabs with an optional selected tab.

    :param tabs: List of tabs
    :param selected_tab_id: Optional ID of the selected tab (defaults to first tab if not provided)
    """

    def __init__(self, *tabs: Tab, selected_tab_id: str | None = None) -> None:
        self.tabs = list(tabs)

        if len(self.tabs) == 0:
            raise ValueError("Tabs list is empty")

        if selected_tab_id is None:
            # use the first tab id as the selected tab id
            self.selected_tab_id = self.tabs[0].id
        else:
            # check if the selected tab id is in the list of tabs
            if selected_tab_id not in [tab.id for tab in self.tabs]:
                raise ValueError(f"Selected tab id {selected_tab_id} not found in tabs")
            self.selected_tab_id = selected_tab_id

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "selected_tab_id": self.selected_tab_id,
            "tabs": [tab.to_dict() for tab in self.tabs],
        }


class Item:
    """
    An item in a layout with a ratio for space allocation.

    The ratio determines the proportion of space this item takes up, similar to CSS flex ratio.
    For example, if a list has 3 items with ratios 1, 2, and 3, the first item takes 1/6 of the
    space, the second item takes 2/6, and the third item takes 3/6.

    :param ratio: Proportion of space this item occupies
    :param content: Content of the item (panel, tabs, or item list)
    """

    def __init__(self, ratio: float, content: Panel | Tabs | ItemList) -> None:
        self.ratio = ratio
        self.content = content

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "ratio": self.ratio,
            "content": self.content.to_dict(),
        }


class ItemList:
    """
    A list of items arranged in a specific direction.

    :param direction: Direction of the layout ("vertical" or "horizontal")
    :param items: List of items to arrange
    """

    def __init__(
        self, *items: Item, direction: Literal["vertical", "horizontal"] = "horizontal"
    ) -> None:
        if len(items) == 0:
            raise ValueError("Items list is empty")

        self.direction = direction
        self.items = list(items)

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "direction": self.direction,
            "items": [item.to_dict() for item in self.items],
        }


class Layout:
    """
    A complete layout containing panels, tabs, and item lists.

    :param version: Version of the layout format
    :param variables: Variables dictionary
    :param items: Root item list containing the layout structure
    """

    def __init__(self, variables: Variables, items: ItemList) -> None:
        self.version = "0.1.0"
        self.variables = variables
        self.items = items

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)

    def to_dict(self) -> dict[str, Any]:
        return {
            "version": self.version,
            "variables": self.variables,
            "items": self.items.to_dict(),
        }


__all__ = [
    "Variables",
    "Panel",
    "Tab",
    "Tabs",
    "Item",
    "ItemList",
    "Layout",
]
