from __future__ import annotations

import inspect
import json
from dataclasses import asdict, dataclass
from typing import Any, Callable, Literal

from . import random_id


def to_camel_case(snake_str: str) -> str:
    return "".join(x.capitalize() for x in snake_str.lower().split("_"))


def to_lower_camel_case(snake_str: str) -> str:
    # We capitalize the first letter of each component except the first one
    # with the 'capitalize' method and join them together.
    camel_string = to_camel_case(snake_str)
    return snake_str[0].lower() + camel_string[1:]


def panel_type(panel_type_name: str) -> Callable[[type[Panel]], type[Panel]]:
    """
    Decorator for Panel subclasses that automatically handles the common __init__ pattern.

    The decorator extracts `id` from kwargs and collects all other parameters (including
    positional variadic args like *rules, *paths) into a panel_config dictionary that
    is passed to the base Panel.__init__.

    :param panel_type_name: The type name string to pass to Panel.__init__
    """

    def decorator(cls: type[Panel]) -> type[Panel]:
        original_init = cls.__init__

        def new_init(
            self: Panel, *args: Any, id: str | None = None, **kwargs: Any
        ) -> None:
            # Get the original function signature to handle variadic positional args
            sig = inspect.signature(original_init)
            bound = sig.bind(self, *args, **kwargs)
            bound.apply_defaults()

            # Identify variadic positional parameters (like *rules, *paths)
            variadic_params = {
                name
                for name, param in sig.parameters.items()
                if param.kind == inspect.Parameter.VAR_POSITIONAL
            }

            # Extract the panel config: all kwargs except 'id' and 'self'
            panel_config: dict[str, Any] = {}
            for param_name, param_value in bound.arguments.items():
                if param_name in ("self", "id"):
                    continue
                # Convert variadic positional args (tuples) to lists to match original behavior
                if param_name in variadic_params and isinstance(param_value, tuple):
                    panel_config[param_name] = list(param_value)
                else:
                    panel_config[param_name] = param_value

            # Call the base Panel.__init__ with the panel type and config
            Panel.__init__(self, panel_type_name, id=id, **panel_config)

        cls.__init__ = new_init  # type: ignore[method-assign]
        return cls

    return decorator


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

    def config_to_dict(self) -> dict[str, Any]:
        return self.config

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "type": self.type,
            # filter out None values from the config
            # and convert snake_case to camelCase
            "config": {
                to_lower_camel_case(k): v
                for k, v in self.config_to_dict().items()
                if v is not None
            },
        }

    def to_json(self) -> str:
        return json.dumps(self.to_dict(), indent=4)


@panel_type("Markdown")
class MarkdownPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        markdown: str | None = None,
        font_size: int | None = None,
        foxglove_panel_title: str | None = None,
    ) -> None:
        pass


@panel_type("RawMessages")
class RawMessagesPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        diff_enabled: bool = False,
        diff_method: Literal["custom", "previous message"] = "custom",
        diff_topic_path: str = "",
        expansion: Literal["all", "none"] | dict[str, Literal["c", "e"]] | None = None,
        show_full_message_for_diff: bool = False,
        topic_path: str = "",
        font_size: int | None = None,
    ) -> None:
        pass


@panel_type("Audio")
class AudioPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        color: str | None = None,
        muted: bool | None = False,
        topic: str | None = None,
        volume: float | None = None,
        sliding_view_width: float | None = None,
        foxglove_panel_title: str | None = None,
    ) -> None:
        pass


@panel_type("ROSDiagnosticDetailPanel")
class ROSDiagnosticDetailPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        selected_hardware_id: str | None = None,
        selected_name: str | None = None,
        split_fraction: float | None = None,
        topic_to_render: str = "",
        numeric_precision: int | None = None,
        seconds_until_stale: int | None = None,
    ) -> None:
        pass


@panel_type("ROSDiagnosticSummaryPanel")
class ROSDiagnosticSummaryPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        min_level: int = 0,
        pinned_ids: list[str] = [],  # noqa: B006
        topic_to_render: str = "",
        hardware_id_filter: str = "",
        sort_by_level: bool | None = None,
        seconds_until_stale: int | None = None,
    ) -> None:
        pass


@dataclass
class IndicatorPanelRule:
    raw_value: str
    operator: Literal["=", "<", "<=", ">", ">="]
    color: str
    label: str

    def to_dict(self) -> dict[str, Any]:
        rule_dict = asdict(self)
        return {to_lower_camel_case(k): v for k, v in rule_dict.items()}


@panel_type("Indicator")
class IndicatorPanel(Panel):
    def __init__(
        self,
        *rules: IndicatorPanelRule,
        id: str | None = None,
        path: str = "",
        style: Literal["bulb", "background"] = "bulb",
        font_size: int | None = None,
        fallback_color: str | None = None,
        fallback_label: str | None = None,
        foxglove_panel_title: str | None = None,
    ) -> None:
        pass

    def config_to_dict(self) -> dict[str, Any]:
        # copy the config and convert rules to dict
        config = super().config_to_dict().copy()
        # Convert rules list to list of dicts
        if "rules" in config:
            config["rules"] = [rule.to_dict() for rule in config["rules"]]
        return config


@panel_type("Gauge")
class GaugePanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        path: str = "",
        min_value: float = 0,
        max_value: float = 1,
        color_mode: Literal["colormap", "gradient"] = "colormap",
        color_map: Literal["red-yellow-green", "rainbow", "turbo"] = "red-yellow-green",
        gradient: tuple[str, str] = ("#0000ff", "#ff00ff"),
        reverse: bool = False,
        reverse_direction: bool = False,
        foxglove_panel_title: str | None = None,
    ) -> None:
        pass


@dataclass
class BasePlotPath:
    value: str
    enabled: bool = True

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


@dataclass
class PlotPath(BasePlotPath):
    id: str | None = None
    color: str | None = None
    label: str | None = None
    timestamp_method: Literal[
        "receiveTime", "publishTime", "headerStamp", "customField"
    ] = "receiveTime"
    timestamp_path: str | None = None
    show_line: bool = True
    line_size: int | None = None
    x_value_path: str | None = None

    def to_dict(self) -> dict[str, Any]:
        rule_dict = asdict(self)
        return {to_lower_camel_case(k): v for k, v in rule_dict.items()}


@panel_type("Plot")
class PlotPanel(Panel):
    def __init__(
        self,
        *paths: PlotPath,
        id: str | None = None,
        min_x_value: float | None = None,
        max_x_value: float | None = None,
        min_y_value: float | None = None,
        max_y_value: float | None = None,
        show_legend: bool = True,
        legend_display: Literal["floating", "top", "left"] = "floating",
        show_plot_values_in_legend: bool = False,
        show_x_axis_labels: bool = True,
        show_y_axis_labels: bool = True,
        is_synced: bool = True,
        x_axis_val: Literal[
            "custom", "timestamp", "index", "currentCustom"
        ] = "timestamp",
        time_range: Literal["all", "latest"] = "all",
        x_axis_path: BasePlotPath | None = None,
        x_axis_label: str | None = None,
        time_window_mode: Literal["automatic", "sliding", "fixed"] = "automatic",
        playback_bar_position: Literal["center", "right"] = "center",
        y_axis_label: str | None = None,
        following_view_width: float | None = None,
        sidebar_dimension: int = 200,
        axis_scales_mode: Literal["independent", "lockedScales"] = "independent",
        foxglove_panel_title: str | None = None,
    ) -> None:
        pass


__all__ = [
    "MarkdownPanel",
    "RawMessagesPanel",
    "AudioPanel",
    "ROSDiagnosticDetailPanel",
    "ROSDiagnosticSummaryPanel",
    "IndicatorPanel",
    "IndicatorPanelRule",
    "GaugePanel",
    "PlotPanel",
]
