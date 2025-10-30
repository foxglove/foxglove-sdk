from __future__ import annotations

from dataclasses import asdict, dataclass
from typing import Any, Literal

from . import Panel


class MarkdownPanel(Panel):
    def __init__(
        self,
        *,
        markdown: str | None = None,
        font_size: int | None = None,
        foxglove_panel_title: str | None = None,
    ):
        super().__init__(
            "Markdown",
            markdown=markdown,
            fontSize=font_size,
            foxglovePanelTitle=foxglove_panel_title,
        )


class RawMessagesPanel(Panel):
    def __init__(
        self,
        *,
        diff_enabled: bool = False,
        diff_method: Literal["custom", "previous message"] = "custom",
        diff_topic_path: str = "",
        expansion: Literal["all", "none"] | dict[str, Literal["c", "e"]] | None = None,
        show_full_message_for_diff: bool = False,
        topic_path: str = "",
        font_size: int | None = None,
    ):
        super().__init__(
            "RawMessages",
            diffEnabled=diff_enabled,
            diffMethod=diff_method,
            diffTopicPath=diff_topic_path,
            expansion=expansion,
            showFullMessageForDiff=show_full_message_for_diff,
            topicPath=topic_path,
            fontSize=font_size,
        )


class AudioPanel(Panel):
    def __init__(
        self,
        *,
        color: str | None = None,
        muted: bool | None = False,
        topic: str | None = None,
        volume: float | None = None,
        sliding_view_width: float | None,
        foxglove_panel_title: str | None = None,
    ):
        super().__init__(
            "Audio",
            color=color,
            muted=muted,
            topic=topic,
            volume=volume,
            slidingViewWidth=sliding_view_width,
            foxglovePanelTitle=foxglove_panel_title,
        )


class ROSDiagnosticDetailPanel(Panel):
    def __init__(
        self,
        *,
        selected_hardware_id: str | None = None,
        selected_name: str | None = None,
        split_fraction: float | None = None,
        topic_to_render: str = "",
        numeric_precision: int | None = None,
        seconds_until_stale: int | None = None,
    ):
        super().__init__(
            "ROSDiagnosticDetailPanel",
            selectedHardwareId=selected_hardware_id,
            selectedName=selected_name,
            splitFraction=split_fraction,
            topicToRender=topic_to_render,
            numericPrecision=numeric_precision,
            secondsUntilStale=seconds_until_stale,
        )


class ROSDiagnosticSummaryPanel(Panel):
    def __init__(
        self,
        *,
        min_level: int = 0,
        pinned_ids: list[str] = [],
        topic_to_render: str = "",
        hardware_id_filter: str = "",
        sort_by_level: bool | None = None,
        seconds_until_stale: int | None = None,
    ):
        super().__init__(
            "ROSDiagnosticSummaryPanel",
            minLevel=min_level,
            pinnedIds=pinned_ids,
            topicToRender=topic_to_render,
            hardwareIdFilter=hardware_id_filter,
            sortByLevel=sort_by_level,
            secondsUntilStale=seconds_until_stale,
        )


@dataclass
class IndicatorPanelRule:
    raw_value: str
    operator: Literal["=", "<", "<=", ">", ">="]
    color: str
    label: str

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class IndicatorPanel(Panel):
    def __init__(
        self,
        *rules: IndicatorPanelRule,
        path: str = "",
        style: Literal["bulb", "background"] = "bulb",
        font_size: int | None = None,
        fallback_color: str | None = None,
        fallback_label: str | None = None,
        foxglove_panel_title: str | None = None,
    ):
        super().__init__(
            "Indicator",
            path=path,
            style=style,
            fontSize=font_size,
            fallbackColor=fallback_color,
            fallbackLabel=fallback_label,
            rules=list(rules),
            foxglovePanelTitle=foxglove_panel_title,
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
        super().__init__(
            "Gauge",
            path=path,
            minValue=min_value,
            maxValue=max_value,
            colorMode=color_mode,
            colorMap=color_map,
            gradient=gradient,
            reverse=reverse,
            reverseDirection=reverse_direction,
            foxglovePanelTitle=foxglove_panel_title,
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
        return asdict(self)


class PlotPanel(Panel):
    def __init__(
        self,
        *paths: PlotPath,
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
        super().__init__(
            "Plot",
            paths=list(paths),
            minXValue=min_x_value,
            maxXValue=max_x_value,
            minYValue=min_y_value,
            maxYValue=max_y_value,
            showLegend=show_legend,
            legendDisplay=legend_display,
            showPlotValuesInLegend=show_plot_values_in_legend,
            showXAxisLabels=show_x_axis_labels,
            showYAxisLabels=show_y_axis_labels,
            isSynced=is_synced,
            xAxisVal=x_axis_val,
            timeRange=time_range,
            xAxisPath=x_axis_path,
            xAxisLabel=x_axis_label,
            timeWindowMode=time_window_mode,
            playbackBarPosition=playback_bar_position,
            yAxisLabel=y_axis_label,
            followingViewWidth=following_view_width,
            sidebarDimension=sidebar_dimension,
            axisScalesMode=axis_scales_mode,
            foxglovePanelTitle=foxglove_panel_title,
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
]
