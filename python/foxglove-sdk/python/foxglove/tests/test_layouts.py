from __future__ import annotations

import json

from foxglove.layouts.panels import (
    AudioPanel,
    BasePlotPath,
    GaugePanel,
    IndicatorPanel,
    IndicatorPanelRule,
    MarkdownPanel,
    PlotPanel,
    PlotPath,
    RawMessagesPanel,
    ROSDiagnosticDetailPanel,
    ROSDiagnosticSummaryPanel,
)


class TestMarkdownPanel:
    def test_creation_with_defaults(self) -> None:
        panel = MarkdownPanel()
        result = panel.to_dict()
        assert result["type"] == "Markdown"
        assert result["id"].startswith("Markdown!")
        assert result["config"] == {}

    def test_creation_with_id(self) -> None:
        panel = MarkdownPanel(id="custom-id")
        result = panel.to_dict()
        assert result["type"] == "Markdown"
        assert result["id"] == "custom-id"
        assert result["config"] == {}

    def test_creation_with_config(self) -> None:
        panel = MarkdownPanel(
            id="test-id",
            markdown="# Hello",
            font_size=14,
            foxglove_panel_title="Test Panel",
        )
        result = panel.to_dict()
        assert result["type"] == "Markdown"
        assert result["id"] == "test-id"
        assert result["config"]["markdown"] == "# Hello"
        assert result["config"]["fontSize"] == 14
        assert result["config"]["foxglovePanelTitle"] == "Test Panel"

    def test_to_dict_filters_none_values(self) -> None:
        panel = MarkdownPanel(id="test", markdown="# Hello", font_size=None)
        result = panel.to_dict()
        assert result["id"] == "test"
        assert result["type"] == "Markdown"
        assert result["config"]["markdown"] == "# Hello"
        assert "fontSize" not in result["config"]

    def test_to_dict_converts_to_camel_case(self) -> None:
        panel = MarkdownPanel(id="test", foxglove_panel_title="Title")
        result = panel.to_dict()
        assert result["config"]["foxglovePanelTitle"] == "Title"

    def test_to_json(self) -> None:
        panel = MarkdownPanel(id="test", markdown="# Hello")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "test"
        assert parsed["type"] == "Markdown"
        assert parsed["config"]["markdown"] == "# Hello"


class TestRawMessagesPanel:
    def test_creation_with_defaults(self) -> None:
        panel = RawMessagesPanel()
        result = panel.to_dict()
        assert result["type"] == "RawMessages"
        assert result["config"]["diffEnabled"] is False
        assert result["config"]["diffMethod"] == "custom"

    def test_creation_with_all_params(self) -> None:
        panel = RawMessagesPanel(
            id="test-id",
            diff_enabled=True,
            diff_method="previous message",
            diff_topic_path="/topic",
            expansion="all",
            show_full_message_for_diff=True,
            topic_path="/messages",
            font_size=12,
        )
        result = panel.to_dict()
        assert result["config"]["diffEnabled"] is True
        assert result["config"]["diffMethod"] == "previous message"
        assert result["config"]["diffTopicPath"] == "/topic"
        assert result["config"]["expansion"] == "all"
        assert result["config"]["showFullMessageForDiff"] is True
        assert result["config"]["topicPath"] == "/messages"
        assert result["config"]["fontSize"] == 12


class TestAudioPanel:
    def test_creation_with_defaults(self) -> None:
        panel = AudioPanel()
        result = panel.to_dict()
        assert result["type"] == "Audio"
        assert result["config"]["muted"] is False

    def test_creation_with_config(self) -> None:
        panel = AudioPanel(
            id="audio-1",
            color="#ff0000",
            muted=True,
            topic="/audio",
            volume=0.8,
            sliding_view_width=10.0,
        )
        result = panel.to_dict()
        assert result["config"]["color"] == "#ff0000"
        assert result["config"]["muted"] is True
        assert result["config"]["topic"] == "/audio"
        assert result["config"]["volume"] == 0.8
        assert result["config"]["slidingViewWidth"] == 10.0


class TestROSDiagnosticDetailPanel:
    def test_creation_with_defaults(self) -> None:
        panel = ROSDiagnosticDetailPanel()
        result = panel.to_dict()
        assert result["type"] == "ROSDiagnosticDetailPanel"
        assert result["config"]["topicToRender"] == ""

    def test_creation_with_config(self) -> None:
        panel = ROSDiagnosticDetailPanel(
            id="diag-1",
            selected_hardware_id="hw1",
            selected_name="sensor",
            split_fraction=0.5,
            topic_to_render="/diagnostics",
            numeric_precision=3,
            seconds_until_stale=5,
        )
        result = panel.to_dict()
        assert result["config"]["selectedHardwareId"] == "hw1"
        assert result["config"]["selectedName"] == "sensor"
        assert result["config"]["splitFraction"] == 0.5
        assert result["config"]["topicToRender"] == "/diagnostics"
        assert result["config"]["numericPrecision"] == 3
        assert result["config"]["secondsUntilStale"] == 5


class TestROSDiagnosticSummaryPanel:
    def test_creation_with_defaults(self) -> None:
        panel = ROSDiagnosticSummaryPanel()
        result = panel.to_dict()
        assert result["type"] == "ROSDiagnosticSummaryPanel"
        assert result["config"]["minLevel"] == 0
        assert result["config"]["pinnedIds"] == []

    def test_creation_with_config(self) -> None:
        panel = ROSDiagnosticSummaryPanel(
            id="summary-1",
            min_level=1,
            pinned_ids=["id1", "id2"],
            topic_to_render="/diagnostics",
            hardware_id_filter="hw*",
            sort_by_level=True,
            seconds_until_stale=10,
        )
        result = panel.to_dict()
        assert result["config"]["minLevel"] == 1
        assert result["config"]["pinnedIds"] == ["id1", "id2"]
        assert result["config"]["topicToRender"] == "/diagnostics"
        assert result["config"]["hardwareIdFilter"] == "hw*"
        assert result["config"]["sortByLevel"] is True
        assert result["config"]["secondsUntilStale"] == 10


class TestIndicatorPanelRule:
    def test_to_dict_converts_to_camel_case(self) -> None:
        rule = IndicatorPanelRule(
            raw_value="10",
            operator=">",
            color="#00ff00",
            label="OK",
        )
        result = rule.to_dict()
        assert result["rawValue"] == "10"
        assert result["operator"] == ">"
        assert result["color"] == "#00ff00"
        assert result["label"] == "OK"


class TestIndicatorPanel:
    def test_creation_without_rules(self) -> None:
        panel = IndicatorPanel(id="ind-1", path="/topic/value")
        result = panel.to_dict()
        assert result["type"] == "Indicator"
        assert result["config"]["path"] == "/topic/value"
        assert result["config"]["rules"] == []

    def test_creation_with_rules(self) -> None:
        rule1 = IndicatorPanelRule(
            raw_value="10", operator=">", color="#00ff00", label="OK"
        )
        rule2 = IndicatorPanelRule(
            raw_value="5", operator="<", color="#ff0000", label="Error"
        )
        panel = IndicatorPanel(
            rule1, rule2, id="ind-1", path="/topic/value", style="background"
        )
        result = panel.to_dict()
        assert len(result["config"]["rules"]) == 2
        assert isinstance(result["config"]["rules"], list)
        assert result["config"]["rules"][0]["rawValue"] == "10"
        assert result["config"]["rules"][0]["operator"] == ">"
        assert result["config"]["rules"][1]["rawValue"] == "5"
        assert result["config"]["rules"][1]["operator"] == "<"
        assert result["config"]["style"] == "background"

    def test_to_dict_filters_none_and_converts_rules(self) -> None:
        rule = IndicatorPanelRule(
            raw_value="10", operator=">=", color="#00ff00", label="OK"
        )
        panel = IndicatorPanel(
            rule,
            id="ind-1",
            path="/topic/value",
            font_size=None,
            fallback_color="#ffffff",
        )
        result = panel.to_dict()
        assert "fontSize" not in result["config"]
        assert result["config"]["fallbackColor"] == "#ffffff"
        assert len(result["config"]["rules"]) == 1
        assert result["config"]["rules"][0]["rawValue"] == "10"
        assert result["config"]["rules"][0]["operator"] == ">="


class TestGaugePanel:
    def test_creation_with_defaults(self) -> None:
        panel = GaugePanel()
        result = panel.to_dict()
        assert result["type"] == "Gauge"
        assert result["config"]["path"] == ""
        assert result["config"]["minValue"] == 0
        assert result["config"]["maxValue"] == 1
        assert result["config"]["colorMode"] == "colormap"
        assert result["config"]["colorMap"] == "red-yellow-green"
        assert result["config"]["gradient"] == ("#0000ff", "#ff00ff")

    def test_creation_with_config(self) -> None:
        panel = GaugePanel(
            id="gauge-1",
            path="/topic/value",
            min_value=-10.0,
            max_value=10.0,
            color_mode="gradient",
            gradient=("#ff0000", "#00ff00"),
            reverse=True,
            reverse_direction=True,
        )
        result = panel.to_dict()
        assert result["config"]["path"] == "/topic/value"
        assert result["config"]["minValue"] == -10.0
        assert result["config"]["maxValue"] == 10.0
        assert result["config"]["colorMode"] == "gradient"
        assert result["config"]["gradient"] == ("#ff0000", "#00ff00")
        assert result["config"]["reverse"] is True
        assert result["config"]["reverseDirection"] is True


class TestBasePlotPath:
    def test_creation_with_defaults(self) -> None:
        path = BasePlotPath(value="/topic/field")
        assert path.value == "/topic/field"
        assert path.enabled is True

    def test_to_dict(self) -> None:
        path = BasePlotPath(value="/topic/field", enabled=False)
        result = path.to_dict()
        assert result["value"] == "/topic/field"
        assert result["enabled"] is False


class TestPlotPath:
    def test_creation_with_defaults(self) -> None:
        path = PlotPath(value="/topic/field")
        assert path.value == "/topic/field"
        assert path.enabled is True
        assert path.timestamp_method == "receiveTime"
        assert path.show_line is True

    def test_to_dict_converts_to_camel_case(self) -> None:
        path = PlotPath(
            value="/topic/field",
            id="path-1",
            color="#ff0000",
            label="Temperature",
            timestamp_method="headerStamp",
            timestamp_path="/header/stamp",
            show_line=False,
            line_size=2,
            x_value_path="/x",
        )
        result = path.to_dict()
        assert result["value"] == "/topic/field"
        assert result["id"] == "path-1"
        assert result["color"] == "#ff0000"
        assert result["label"] == "Temperature"
        assert result["timestampMethod"] == "headerStamp"
        assert result["timestampPath"] == "/header/stamp"
        assert result["showLine"] is False
        assert result["lineSize"] == 2
        assert result["xValuePath"] == "/x"


class TestPlotPanel:
    def test_creation_without_paths(self) -> None:
        panel = PlotPanel(id="plot-1")
        result = panel.to_dict()
        assert result["type"] == "Plot"
        assert result["config"]["paths"] == []

    def test_creation_with_paths(self) -> None:
        path1 = PlotPath(value="/topic/field1", label="Field 1")
        path2 = PlotPath(value="/topic/field2", label="Field 2")
        panel = PlotPanel(path1, path2, id="plot-1")
        result = panel.to_dict()
        assert len(result["config"]["paths"]) == 2
        assert isinstance(result["config"]["paths"], list)
        assert result["config"]["paths"][0]["value"] == "/topic/field1"
        assert result["config"]["paths"][0]["label"] == "Field 1"
        assert result["config"]["paths"][1]["value"] == "/topic/field2"
        assert result["config"]["paths"][1]["label"] == "Field 2"

    def test_creation_with_complex_config(self) -> None:
        path = PlotPath(value="/topic/field")
        x_axis_path = BasePlotPath(value="/topic/x", enabled=True)
        panel = PlotPanel(
            path,
            id="plot-1",
            min_x_value=-10.0,
            max_x_value=10.0,
            min_y_value=-5.0,
            max_y_value=5.0,
            show_legend=False,
            legend_display="top",
            show_plot_values_in_legend=True,
            show_x_axis_labels=False,
            show_y_axis_labels=False,
            is_synced=False,
            x_axis_val="custom",
            time_range="latest",
            x_axis_path=x_axis_path,
            x_axis_label="Time (s)",
            time_window_mode="sliding",
            playback_bar_position="right",
            y_axis_label="Value",
            following_view_width=30.0,
            sidebar_dimension=250,
            axis_scales_mode="lockedScales",
        )
        result = panel.to_dict()
        assert result["config"]["minXValue"] == -10.0
        assert result["config"]["maxXValue"] == 10.0
        assert result["config"]["showLegend"] is False
        assert result["config"]["legendDisplay"] == "top"
        assert result["config"]["xAxisVal"] == "custom"
        assert result["config"]["xAxisPath"]["value"] == "/topic/x"
        assert result["config"]["xAxisPath"]["enabled"] is True
        assert result["config"]["sidebarDimension"] == 250

    def test_to_dict_converts_paths_and_filters_none(self) -> None:
        path = PlotPath(value="/topic/field", label="Field")
        panel = PlotPanel(path, id="plot-1", min_x_value=None, max_x_value=10.0)
        result = panel.to_dict()
        assert result["type"] == "Plot"
        assert "minXValue" not in result["config"]
        assert result["config"]["maxXValue"] == 10.0
        assert len(result["config"]["paths"]) == 1
        assert result["config"]["paths"][0]["value"] == "/topic/field"
        assert result["config"]["paths"][0]["label"] == "Field"


class TestPanelSerialization:
    def test_all_panels_serialize_to_json(self) -> None:
        panels = [
            MarkdownPanel(id="md", markdown="# Test"),
            RawMessagesPanel(id="raw", topic_path="/messages"),
            AudioPanel(id="audio", topic="/audio"),
            ROSDiagnosticDetailPanel(id="diag", topic_to_render="/diagnostics"),
            ROSDiagnosticSummaryPanel(id="summary", topic_to_render="/diagnostics"),
            IndicatorPanel(id="ind", path="/value"),
            GaugePanel(id="gauge", path="/value"),
            PlotPanel(id="plot"),
        ]
        for panel in panels:
            json_str = panel.to_json()
            parsed = json.loads(json_str)
            assert parsed["id"] == panel.id
            assert parsed["type"] == panel.type
            assert isinstance(parsed["config"], dict)

    def test_panel_id_generation(self) -> None:
        panel1 = MarkdownPanel()
        panel2 = MarkdownPanel()
        assert panel1.id != panel2.id
        assert panel1.id.startswith("Markdown!")
        assert panel2.id.startswith("Markdown!")
