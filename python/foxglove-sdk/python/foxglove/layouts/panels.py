from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from typing import Any, Literal

from . import random_id


def to_camel_case(snake_str: str) -> str:
    return "".join(x.capitalize() for x in snake_str.lower().split("_"))


def to_lower_camel_case(snake_str: str) -> str:
    # We capitalize the first letter of each component except the first one
    # with the 'capitalize' method and join them together.
    camel_string = to_camel_case(snake_str)
    return snake_str[0].lower() + camel_string[1:]


class Panel:
    """
    A panel in a layout.

    :param id: Unique identifier for the panel
    :param type: Type of the panel (e.g., "Plot", "3D", "Raw Messages")
    :param config: Configuration dictionary for the panel
    """

    def __init__(self, type: str, id: str | None = None, **panel_config: Any) -> None:

        print(f"hello: {type}, {id}, {panel_config}")

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


class MarkdownPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        markdown: str | None = None,
        font_size: int | None = None,
        foxglove_panel_title: str | None = None,
    ):
        panel_config = {
            "markdown": markdown,
            "font_size": font_size,
            "foxglove_panel_title": foxglove_panel_title,
        }

        super().__init__(
            "Markdown",
            id,
            **panel_config,
        )


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
    ):
        panel_config = {
            "diff_enabled": diff_enabled,
            "diff_method": diff_method,
            "diff_topic_path": diff_topic_path,
            "expansion": expansion,
            "show_full_message_for_diff": show_full_message_for_diff,
            "topic_path": topic_path,
            "font_size": font_size,
        }

        super().__init__(
            "RawMessages",
            id,
            **panel_config,
        )


class AudioPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        color: str | None = None,
        muted: bool | None = False,
        topic: str | None = None,
        volume: float | None = None,
        sliding_view_width: float | None,
        foxglove_panel_title: str | None = None,
    ):
        panel_config = {
            "color": color,
            "muted": muted,
            "topic": topic,
            "volume": volume,
            "sliding_view_width": sliding_view_width,
            "foxglove_panel_title": foxglove_panel_title,
        }

        super().__init__(
            "Audio",
            id,
            **panel_config,
        )


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
    ):
        panel_config = {
            "selected_hardware_id": selected_hardware_id,
            "selected_name": selected_name,
            "split_fraction": split_fraction,
            "topic_to_render": topic_to_render,
            "numeric_precision": numeric_precision,
            "seconds_until_stale": seconds_until_stale,
        }

        super().__init__(
            "ROSDiagnosticDetailPanel",
            id,
            **panel_config,
        )


class ROSDiagnosticSummaryPanel(Panel):
    def __init__(
        self,
        *,
        id: str | None = None,
        min_level: int = 0,
        pinned_ids: list[str] = [],
        topic_to_render: str = "",
        hardware_id_filter: str = "",
        sort_by_level: bool | None = None,
        seconds_until_stale: int | None = None,
    ):
        panel_config = {
            "min_level": min_level,
            "pinned_ids": pinned_ids,
            "topic_to_render": topic_to_render,
            "hardware_id_filter": hardware_id_filter,
            "sort_by_level": sort_by_level,
            "seconds_until_stale": seconds_until_stale,
        }

        super().__init__(
            "ROSDiagnosticSummaryPanel",
            id,
            **panel_config,
        )


@dataclass
class IndicatorPanelRule:
    raw_value: str
    operator: Literal["=", "<", "<=", ">", ">="]
    color: str
    label: str

    def to_dict(self) -> dict[str, Any]:
        rule_dict = asdict(self)
        return {to_lower_camel_case(k): v for k, v in rule_dict.items()}


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
    ):
        panel_config = {
            "path": path,
            "style": style,
            "font_size": font_size,
            "fallback_color": fallback_color,
            "fallback_label": fallback_label,
            "foxglove_panel_title": foxglove_panel_title,
            "rules": list(rules),
        }

        super().__init__(
            "Indicator",
            id,
            **panel_config,
        )

    def config_to_dict(self) -> dict[str, Any]:
        # copy the config and convert rules to dict
        config = super().config_to_dict().copy()
        config["rules"] = [rule.to_dict() for rule in config.get("rules", [])]
        return config


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
    ):
        panel_config = {
            "path": path,
            "min_value": min_value,
            "max_value": max_value,
            "color_mode": color_mode,
            "color_map": color_map,
            "gradient": gradient,
            "reverse": reverse,
            "reverse_direction": reverse_direction,
            "foxglove_panel_title": foxglove_panel_title,
        }

        super().__init__(
            "Gauge",
            id,
            **panel_config,
        )


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
    ):
        panel_config = {
            "paths": paths,
            "min_x_value": min_x_value,
            "max_x_value": max_x_value,
            "min_y_value": min_y_value,
            "max_y_value": max_y_value,
            "show_legend": show_legend,
            "legend_display": legend_display,
            "show_plot_values_in_legend": show_plot_values_in_legend,
            "show_x_axis_labels": show_x_axis_labels,
            "show_y_axis_labels": show_y_axis_labels,
            "is_synced": is_synced,
            "x_axis_val": x_axis_val,
            "time_range": time_range,
            "x_axis_path": x_axis_path,
            "x_axis_label": x_axis_label,
            "time_window_mode": time_window_mode,
            "playback_bar_position": playback_bar_position,
            "y_axis_label": y_axis_label,
            "following_view_width": following_view_width,
            "sidebar_dimension": sidebar_dimension,
            "axis_scales_mode": axis_scales_mode,
            "foxglove_panel_title": foxglove_panel_title,
        }

        super().__init__(
            "Plot",
            id,
            **panel_config,
        )


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
