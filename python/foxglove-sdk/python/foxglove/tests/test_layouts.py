from __future__ import annotations

import json
from typing import Literal

from foxglove.layouts.panels import (
    AudioPanel,
    BaseCustomState,
    BasePlotPath,
    ButtonConfig,
    CameraState,
    DataSourceInfoPanel,
    GaugePanel,
    GridLayerConfig,
    ImageAnnotationSettings,
    ImageModeConfig,
    ImagePanel,
    IndicatorPanel,
    IndicatorPanelRule,
    LayersConfig,
    LinkSettings,
    LogPanel,
    MapCoordinates,
    MapPanel,
    MapTopicConfig,
    MarkdownPanel,
    NameFilter,
    ParametersPanel,
    PlotPanel,
    PlotPath,
    PublishPanel,
    RawMessagesPanel,
    ROSDiagnosticDetailPanel,
    ROSDiagnosticSummaryPanel,
    SceneConfig,
    ServiceCallPanel,
    StateTransitionsDiscreteCustomState,
    StateTransitionsDiscreteCustomStates,
    StateTransitionsPanel,
    StateTransitionsPath,
    StateTransitionsRangeCustomState,
    StateTransitionsRangeCustomStates,
    TablePanel,
    TeleopPanel,
    ThreeDeePanel,
    TiledMapLayerConfig,
    TopicGraphPanel,
    TopicsConfig,
    VariableSliderPanel,
    VariableSliderConfig,
    TransformConfig,
    TransformsConfig,
    TransformTreePanel,
    UrdfLayerConfig,
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
        assert result["type"] == "DiagnosticStatusPanel"
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
        assert result["type"] == "DiagnosticSummary"
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


class TestImageAnnotationSettings:
    def test_to_dict(self) -> None:
        settings = ImageAnnotationSettings(visible=True)
        result = settings.to_dict()
        assert result["visible"] is True

    def test_to_dict_with_false(self) -> None:
        settings = ImageAnnotationSettings(visible=False)
        result = settings.to_dict()
        assert result["visible"] is False


class TestImageModeConfig:
    def test_creation_with_defaults(self) -> None:
        config = ImageModeConfig()
        result = config.to_dict()
        assert result == {}

    def test_creation_with_all_params(self) -> None:
        annotations: dict[str, ImageAnnotationSettings | None] = {
            "keypoint": ImageAnnotationSettings(visible=True),
            "polygon": ImageAnnotationSettings(visible=False),
        }
        config = ImageModeConfig(
            image_topic="/camera/image",
            image_schema_name="foxglove.CompressedImage",
            calibration_topic="/camera/calibration",
            annotations=annotations,
            synchronize=True,
            rotation=90,
            flip_horizontal=True,
            flip_vertical=False,
        )
        result = config.to_dict()
        assert result["imageTopic"] == "/camera/image"
        assert result["imageSchemaName"] == "foxglove.CompressedImage"
        assert result["calibrationTopic"] == "/camera/calibration"
        assert result["synchronize"] is True
        assert result["rotation"] == 90
        assert result["flipHorizontal"] is True
        assert result["flipVertical"] is False
        assert isinstance(result["annotations"], dict)
        assert result["annotations"]["keypoint"]["visible"] is True
        assert result["annotations"]["polygon"]["visible"] is False

    def test_to_dict_filters_none_values(self) -> None:
        config = ImageModeConfig(image_topic="/camera/image", rotation=None)
        result = config.to_dict()
        assert result["imageTopic"] == "/camera/image"
        assert "rotation" not in result

    def test_to_dict_filters_none_annotations(self) -> None:
        annotations = {
            "keypoint": ImageAnnotationSettings(visible=True),
            "polygon": None,
        }
        config = ImageModeConfig(annotations=annotations)
        result = config.to_dict()
        assert "keypoint" in result["annotations"]
        assert "polygon" in result["annotations"]
        assert result["annotations"]["polygon"] is None


class TestImagePanel:
    def test_creation_with_minimal_config(self) -> None:
        image_mode = ImageModeConfig(image_topic="/camera/image")
        panel = ImagePanel(image_mode=image_mode)
        result = panel.to_dict()
        assert result["type"] == "Image"
        assert result["id"].startswith("Image!")
        assert result["config"]["imageMode"]["imageTopic"] == "/camera/image"

    def test_creation_with_id(self) -> None:
        image_mode = ImageModeConfig(image_topic="/camera/image")
        panel = ImagePanel(id="custom-id", image_mode=image_mode)
        result = panel.to_dict()
        assert result["type"] == "Image"
        assert result["id"] == "custom-id"
        assert result["config"]["imageMode"]["imageTopic"] == "/camera/image"

    def test_creation_with_full_config(self) -> None:
        annotations: dict[str, ImageAnnotationSettings | None] = {
            "keypoint": ImageAnnotationSettings(visible=True),
            "polygon": ImageAnnotationSettings(visible=False),
        }
        image_mode = ImageModeConfig(
            image_topic="/camera/image",
            image_schema_name="foxglove.CompressedImage",
            calibration_topic="/camera/calibration",
            annotations=annotations,
            synchronize=True,
            rotation=180,
            flip_horizontal=True,
            flip_vertical=True,
        )
        panel = ImagePanel(id="image-1", image_mode=image_mode)
        result = panel.to_dict()
        assert result["type"] == "Image"
        assert result["id"] == "image-1"
        config = result["config"]["imageMode"]
        assert config["imageTopic"] == "/camera/image"
        assert config["imageSchemaName"] == "foxglove.CompressedImage"
        assert config["calibrationTopic"] == "/camera/calibration"
        assert config["synchronize"] is True
        assert config["rotation"] == 180
        assert config["flipHorizontal"] is True
        assert config["flipVertical"] is True
        assert config["annotations"]["keypoint"]["visible"] is True
        assert config["annotations"]["polygon"]["visible"] is False

    def test_to_dict_converts_to_camel_case(self) -> None:
        image_mode = ImageModeConfig(
            image_topic="/camera/image",
            rotation=90,
            flip_horizontal=False,
        )
        panel = ImagePanel(id="test", image_mode=image_mode)
        result = panel.to_dict()
        config = result["config"]["imageMode"]
        assert config["imageTopic"] == "/camera/image"
        assert config["rotation"] == 90
        assert config["flipHorizontal"] is False

    def test_to_json(self) -> None:
        image_mode = ImageModeConfig(image_topic="/camera/image", rotation=270)
        panel = ImagePanel(id="test", image_mode=image_mode)
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "test"
        assert parsed["type"] == "Image"
        assert parsed["config"]["imageMode"]["imageTopic"] == "/camera/image"
        assert parsed["config"]["imageMode"]["rotation"] == 270

    def test_to_dict_with_all_rotation_values(self) -> None:
        rotations: list[Literal[0, 90, 180, 270]] = [0, 90, 180, 270]
        for rotation in rotations:
            image_mode = ImageModeConfig(image_topic="/camera/image", rotation=rotation)
            panel = ImagePanel(image_mode=image_mode)
            result = panel.to_dict()
            assert result["config"]["imageMode"]["rotation"] == rotation


class TestTransformsConfig:
    def test_creation_with_defaults(self) -> None:
        config = TransformsConfig()
        result = config.to_dict()
        assert result["visible"] is True
        assert result["editable"] is True
        assert result["showLabel"] is True
        assert result["enablePreloading"] is False
        assert result["drawBehind"] is False

    def test_creation_with_all_params(self) -> None:
        config = TransformsConfig(
            visible=False,
            editable=False,
            show_label=False,
            label_size=12.0,
            axis_size=10.0,
            line_width=2.0,
            line_color="#ff0000",
            enable_preloading=True,
            draw_behind=True,
        )
        result = config.to_dict()
        assert result["visible"] is False
        assert result["editable"] is False
        assert result["showLabel"] is False
        assert result["labelSize"] == 12.0
        assert result["axisSize"] == 10.0
        assert result["lineWidth"] == 2.0
        assert result["lineColor"] == "#ff0000"
        assert result["enablePreloading"] is True
        assert result["drawBehind"] is True

    def test_to_dict_filters_none_values(self) -> None:
        config = TransformsConfig(label_size=None, axis_size=10.0)
        result = config.to_dict()
        assert "labelSize" not in result
        assert result["axisSize"] == 10.0


class TestSceneConfig:
    def test_creation_with_defaults(self) -> None:
        config = SceneConfig()
        result = config.to_dict()
        assert result["enableStats"] is False
        assert result["ignoreColladaUpAxis"] is False
        assert result["meshUpAxis"] == "z_up"
        assert result["syncCamera"] is False

    def test_creation_with_all_params(self) -> None:
        transforms = TransformsConfig(visible=True, label_size=15.0)
        config = SceneConfig(
            enable_stats=True,
            background_color="#000000",
            label_scale_factor=1.5,
            ignore_collada_up_axis=True,
            mesh_up_axis="y_up",
            transforms=transforms,
            sync_camera=True,
        )
        result = config.to_dict()
        assert result["enableStats"] is True
        assert result["backgroundColor"] == "#000000"
        assert result["labelScaleFactor"] == 1.5
        assert result["ignoreColladaUpAxis"] is True
        assert result["meshUpAxis"] == "y_up"
        assert result["syncCamera"] is True
        assert isinstance(result["transforms"], dict)
        assert result["transforms"]["visible"] is True
        assert result["transforms"]["labelSize"] == 15.0

    def test_to_dict_filters_none_values(self) -> None:
        config = SceneConfig(background_color=None, label_scale_factor=2.0)
        result = config.to_dict()
        assert "backgroundColor" not in result
        assert result["labelScaleFactor"] == 2.0


class TestCameraState:
    def test_creation_with_defaults(self) -> None:
        config = CameraState()
        result = config.to_dict()
        assert result["distance"] == 20
        assert result["perspective"] is True
        assert result["phi"] == 60
        assert result["target"] == (0, 0, 0)
        assert result["targetOffset"] == (0, 0, 0)
        assert result["targetOrientation"] == (0, 0, 0, 1)
        assert result["thetaOffset"] == 45
        assert result["fovy"] == 45
        assert result["near"] == 0.5
        assert result["far"] == 5000
        assert result["logDepth"] is False

    def test_creation_with_custom_values(self) -> None:
        config = CameraState(
            distance=50.0,
            perspective=False,
            phi=90.0,
            target=(1.0, 2.0, 3.0),
            target_offset=(0.5, 0.5, 0.5),
            target_orientation=(1.0, 0.0, 0.0, 0.0),
            theta_offset=90.0,
            fovy=60.0,
            near=1.0,
            far=10000.0,
            log_depth=True,
        )
        result = config.to_dict()
        assert result["distance"] == 50.0
        assert result["perspective"] is False
        assert result["phi"] == 90.0
        assert result["target"] == (1.0, 2.0, 3.0)
        assert result["targetOffset"] == (0.5, 0.5, 0.5)
        assert result["targetOrientation"] == (1.0, 0.0, 0.0, 0.0)
        assert result["thetaOffset"] == 90.0
        assert result["fovy"] == 60.0
        assert result["near"] == 1.0
        assert result["far"] == 10000.0
        assert result["logDepth"] is True


class TestTransformConfig:
    def test_creation_with_defaults(self) -> None:
        config = TransformConfig()
        result = config.to_dict()
        assert result["visible"] is False

    def test_creation_with_all_params(self) -> None:
        config = TransformConfig(
            visible=True,
            draw_behind=True,
            frame_locked=False,
            xyz_offset=(1.0, 2.0, 3.0),
            rpy_coefficient=(0.1, 0.2, 0.3),
        )
        result = config.to_dict()
        assert result["visible"] is True
        assert result["drawBehind"] is True
        assert result["frameLocked"] is False
        assert result["xyzOffset"] == (1.0, 2.0, 3.0)
        assert result["rpyCoefficient"] == (0.1, 0.2, 0.3)

    def test_to_dict_filters_none_values(self) -> None:
        config = TransformConfig(visible=True, draw_behind=None)
        result = config.to_dict()
        assert result["visible"] is True
        assert "drawBehind" not in result


class TestTopicsConfig:
    def test_creation_with_defaults(self) -> None:
        config = TopicsConfig()
        result = config.to_dict()
        assert result["visible"] is False

    def test_creation_with_all_params(self) -> None:
        config = TopicsConfig(
            visible=True,
            draw_behind=True,
            frame_locked=False,
        )
        result = config.to_dict()
        assert result["visible"] is True
        assert result["drawBehind"] is True
        assert result["frameLocked"] is False

    def test_to_dict_filters_none_values(self) -> None:
        config = TopicsConfig(visible=True, draw_behind=None)
        result = config.to_dict()
        assert result["visible"] is True
        assert "drawBehind" not in result


class TestLayersConfig:
    def test_creation_with_required_params(self) -> None:
        config = LayersConfig(
            instance_id="instance1",
            layer_id="layer1",
            label="Test Layer",
        )
        result = config.to_dict()
        assert result["instanceId"] == "instance1"
        assert result["layerId"] == "layer1"
        assert result["label"] == "Test Layer"
        assert result["visible"] is False

    def test_creation_with_all_params(self) -> None:
        config = LayersConfig(
            instance_id="instance1",
            layer_id="layer1",
            label="Test Layer",
            visible=True,
            draw_behind=True,
            frame_locked=False,
            order=5,
        )
        result = config.to_dict()
        assert result["instanceId"] == "instance1"
        assert result["layerId"] == "layer1"
        assert result["label"] == "Test Layer"
        assert result["visible"] is True
        assert result["drawBehind"] is True
        assert result["frameLocked"] is False
        assert result["order"] == 5

    def test_to_dict_filters_none_values(self) -> None:
        config = LayersConfig(
            instance_id="instance1",
            layer_id="layer1",
            label="Test",
            order=None,
        )
        result = config.to_dict()
        assert "order" not in result


class TestGridLayerConfig:
    def test_creation_with_defaults(self) -> None:
        config = GridLayerConfig(instance_id="grid1")
        result = config.to_dict()
        assert result["instanceId"] == "grid1"
        assert result["layerId"] == "foxglove.Grid"
        assert result["label"] == "Grid"
        assert result["visible"] is True
        assert result["size"] == 10
        assert result["divisions"] == 10
        assert result["lineWidth"] == 1
        assert result["color"] == "#248eff"
        assert result["position"] == (0, 0, 0)
        assert result["rotation"] == (0, 0, 0)

    def test_creation_with_all_params(self) -> None:
        config = GridLayerConfig(
            instance_id="grid1",
            visible=False,
            draw_behind=True,
            frame_locked=True,
            order=1,
            frame_id="map",
            size=20.0,
            divisions=20,
            line_width=2.0,
            color="#ff0000",
            position=(1.0, 2.0, 3.0),
            rotation=(90.0, 0.0, 0.0),
        )
        result = config.to_dict()
        assert result["instanceId"] == "grid1"
        assert result["layerId"] == "foxglove.Grid"
        assert result["visible"] is False
        assert result["drawBehind"] is True
        assert result["frameLocked"] is True
        assert result["order"] == 1
        assert result["frameId"] == "map"
        assert result["size"] == 20.0
        assert result["divisions"] == 20
        assert result["lineWidth"] == 2.0
        assert result["color"] == "#ff0000"
        assert result["position"] == (1.0, 2.0, 3.0)
        assert result["rotation"] == (90.0, 0.0, 0.0)

    def test_to_dict_filters_none_values(self) -> None:
        config = GridLayerConfig(instance_id="grid1", frame_id=None)
        result = config.to_dict()
        assert "frameId" not in result


class TestTiledMapLayerConfig:
    def test_creation_with_defaults(self) -> None:
        config = TiledMapLayerConfig(instance_id="map1")
        result = config.to_dict()
        assert result["instanceId"] == "map1"
        assert result["layerId"] == "foxglove.TiledMap"
        assert result["label"] == "Map"
        assert result["visible"] is True
        assert result["serverConfig"] == "map"
        assert result["mapSizeM"] == 500
        assert result["opacity"] == 1
        assert result["zPosition"] == 0

    def test_creation_with_all_params(self) -> None:
        config = TiledMapLayerConfig(
            instance_id="map1",
            visible=False,
            draw_behind=True,
            frame_locked=False,
            order=2,
            server_config="satellite",
            custom_map_tile_server="https://example.com/{z}/{x}/{y}",
            map_size_m=1000.0,
            opacity=0.8,
            z_position=-1.0,
        )
        result = config.to_dict()
        assert result["instanceId"] == "map1"
        assert result["layerId"] == "foxglove.TiledMap"
        assert result["visible"] is False
        assert result["drawBehind"] is True
        assert result["frameLocked"] is False
        assert result["order"] == 2
        assert result["serverConfig"] == "satellite"
        assert result["customMapTileServer"] == "https://example.com/{z}/{x}/{y}"
        assert result["mapSizeM"] == 1000.0
        assert result["opacity"] == 0.8
        assert result["zPosition"] == -1.0

    def test_to_dict_filters_none_values(self) -> None:
        config = TiledMapLayerConfig(
            instance_id="map1",
            custom_map_tile_server=None,
            map_size_m=None,
        )
        result = config.to_dict()
        assert "customMapTileServer" not in result
        assert "mapSizeM" not in result


class TestLinkSettings:
    def test_creation_with_defaults(self) -> None:
        settings = LinkSettings()
        result = settings.to_dict()
        assert result["visible"] is True

    def test_creation_with_false(self) -> None:
        settings = LinkSettings(visible=False)
        result = settings.to_dict()
        assert result["visible"] is False

    def test_to_dict_with_none(self) -> None:
        settings = LinkSettings(visible=None)
        result = settings.to_dict()
        assert result["visible"] is None


class TestUrdfLayerConfig:
    def test_creation_with_defaults(self) -> None:
        config = UrdfLayerConfig(instance_id="urdf1")
        result = config.to_dict()
        assert result["instanceId"] == "urdf1"
        assert result["layerId"] == "foxglove.Urdf"
        assert result["label"] == "URDF"
        assert result["visible"] is False
        assert result["displayMode"] == "auto"
        assert result["showAxis"] is False
        assert result["axisScale"] == 1.0
        assert result["showOutlines"] is True
        assert result["opacity"] == 1.0
        assert result["sourceType"] == "url"
        assert result["framePrefix"] == ""

    def test_creation_with_all_params(self) -> None:
        links = {
            "link1": LinkSettings(visible=True),
            "link2": LinkSettings(visible=False),
        }
        config = UrdfLayerConfig(
            instance_id="urdf1",
            visible=True,
            draw_behind=False,
            frame_locked=True,
            order=3,
            display_mode="visual",
            fallback_color="#00ff00",
            show_axis=True,
            axis_scale=2.0,
            show_outlines=False,
            opacity=0.9,
            source_type="topic",
            url="https://example.com/robot.urdf",
            file_path="/path/to/robot.urdf",
            parameter="/robot_description",
            topic="/urdf",
            frame_prefix="robot_",
            links=links,
        )
        result = config.to_dict()
        assert result["instanceId"] == "urdf1"
        assert result["layerId"] == "foxglove.Urdf"
        assert result["visible"] is True
        assert result["drawBehind"] is False
        assert result["frameLocked"] is True
        assert result["order"] == 3
        assert result["displayMode"] == "visual"
        assert result["fallbackColor"] == "#00ff00"
        assert result["showAxis"] is True
        assert result["axisScale"] == 2.0
        assert result["showOutlines"] is False
        assert result["opacity"] == 0.9
        assert result["sourceType"] == "topic"
        assert result["url"] == "https://example.com/robot.urdf"
        assert result["filePath"] == "/path/to/robot.urdf"
        assert result["parameter"] == "/robot_description"
        assert result["topic"] == "/urdf"
        assert result["framePrefix"] == "robot_"
        assert isinstance(result["links"], dict)
        assert result["links"]["link1"]["visible"] is True
        assert result["links"]["link2"]["visible"] is False

    def test_to_dict_filters_none_values(self) -> None:
        config = UrdfLayerConfig(
            instance_id="urdf1",
            fallback_color=None,
            show_axis=None,
            url=None,
        )
        result = config.to_dict()
        assert "fallbackColor" not in result
        assert "showAxis" not in result
        assert "url" not in result


class TestThreeDeePanel:
    def test_creation_with_defaults(self) -> None:
        panel = ThreeDeePanel()
        result = panel.to_dict()
        assert result["type"] == "3D"
        assert result["id"].startswith("3D!")
        assert result["config"]["followMode"] == "follow-pose"

    def test_creation_with_id(self) -> None:
        panel = ThreeDeePanel(id="custom-id")
        result = panel.to_dict()
        assert result["type"] == "3D"
        assert result["id"] == "custom-id"

    def test_creation_with_follow_config(self) -> None:
        panel = ThreeDeePanel(
            id="3d-1",
            follow_tf="base_link",
            follow_mode="follow-none",
        )
        result = panel.to_dict()
        assert result["id"] == "3d-1"
        assert result["config"]["followTf"] == "base_link"
        assert result["config"]["followMode"] == "follow-none"

    def test_creation_with_location_fix_topic(self) -> None:
        panel = ThreeDeePanel(
            id="3d-1",
            location_fix_topic="/gps/fix",
        )
        result = panel.to_dict()
        assert result["id"] == "3d-1"
        assert result["config"]["locationFixTopic"] == "/gps/fix"

    def test_creation_with_enu_frame_id(self) -> None:
        panel = ThreeDeePanel(
            id="3d-1",
            enu_frame_id="enu",
        )
        result = panel.to_dict()
        assert result["id"] == "3d-1"
        assert result["config"]["enuFrameId"] == "enu"

    def test_creation_with_location_and_enu_config(self) -> None:
        panel = ThreeDeePanel(
            id="3d-1",
            location_fix_topic="/gps/fix",
            enu_frame_id="enu",
        )
        result = panel.to_dict()
        assert result["id"] == "3d-1"
        assert result["config"]["locationFixTopic"] == "/gps/fix"
        assert result["config"]["enuFrameId"] == "enu"

    def test_creation_with_scene_config(self) -> None:
        scene = SceneConfig(
            enable_stats=True,
            background_color="#ffffff",
            mesh_up_axis="y_up",
        )
        panel = ThreeDeePanel(id="3d-1", scene=scene)
        result = panel.to_dict()
        assert result["config"]["scene"]["enableStats"] is True
        assert result["config"]["scene"]["backgroundColor"] == "#ffffff"
        assert result["config"]["scene"]["meshUpAxis"] == "y_up"

    def test_creation_with_camera_state(self) -> None:
        camera_state = CameraState(
            distance=100.0,
            perspective=False,
            target=(10.0, 20.0, 30.0),
        )
        panel = ThreeDeePanel(id="3d-1", camera_state=camera_state)
        result = panel.to_dict()
        assert result["config"]["cameraState"]["distance"] == 100.0
        assert result["config"]["cameraState"]["perspective"] is False
        assert result["config"]["cameraState"]["target"] == (10.0, 20.0, 30.0)

    def test_creation_with_transforms(self) -> None:
        transforms: dict[str, TransformConfig | None] = {
            "frame1": TransformConfig(visible=True, draw_behind=True),
            "frame2": TransformConfig(visible=False),
        }
        panel = ThreeDeePanel(id="3d-1", transforms=transforms)
        result = panel.to_dict()
        assert "frame1" in result["config"]["transforms"]
        assert "frame2" in result["config"]["transforms"]
        assert result["config"]["transforms"]["frame1"]["visible"] is True
        assert result["config"]["transforms"]["frame1"]["drawBehind"] is True
        assert result["config"]["transforms"]["frame2"]["visible"] is False

    def test_creation_with_topics(self) -> None:
        topics: dict[str, TopicsConfig | None] = {
            "/topic1": TopicsConfig(visible=True),
            "/topic2": TopicsConfig(visible=False, frame_locked=True),
        }
        panel = ThreeDeePanel(id="3d-1", topics=topics)
        result = panel.to_dict()
        assert "/topic1" in result["config"]["topics"]
        assert "/topic2" in result["config"]["topics"]
        assert result["config"]["topics"]["/topic1"]["visible"] is True
        assert result["config"]["topics"]["/topic2"]["frameLocked"] is True

    def test_creation_with_layers(self) -> None:
        layers: dict[
            str,
            LayersConfig
            | GridLayerConfig
            | TiledMapLayerConfig
            | UrdfLayerConfig
            | None,
        ] = {
            "grid": GridLayerConfig(
                instance_id="grid1",
                visible=True,
                size=15.0,
                color="#00ff00",
            ),
            "map": TiledMapLayerConfig(
                instance_id="map1",
                visible=True,
                server_config="satellite",
                opacity=0.8,
            ),
            "urdf": UrdfLayerConfig(
                instance_id="urdf1",
                visible=True,
                display_mode="visual",
                source_type="url",
                url="https://example.com/robot.urdf",
            ),
            "generic": LayersConfig(
                instance_id="inst1",
                layer_id="layer1",
                label="Generic Layer",
                visible=True,
                order=1,
            ),
        }
        panel = ThreeDeePanel(id="3d-1", layers=layers)
        result = panel.to_dict()
        assert "grid" in result["config"]["layers"]
        assert "map" in result["config"]["layers"]
        assert "urdf" in result["config"]["layers"]
        assert "generic" in result["config"]["layers"]
        assert result["config"]["layers"]["grid"]["layerId"] == "foxglove.Grid"
        assert result["config"]["layers"]["grid"]["size"] == 15.0
        assert result["config"]["layers"]["map"]["layerId"] == "foxglove.TiledMap"
        assert result["config"]["layers"]["map"]["serverConfig"] == "satellite"
        assert result["config"]["layers"]["urdf"]["layerId"] == "foxglove.Urdf"
        assert result["config"]["layers"]["urdf"]["displayMode"] == "visual"
        assert result["config"]["layers"]["generic"]["label"] == "Generic Layer"

    def test_creation_with_full_config(self) -> None:
        scene = SceneConfig(
            enable_stats=True,
            background_color="#000000",
            transforms=TransformsConfig(visible=True, label_size=12.0),
        )
        camera_state = CameraState(distance=50.0, target=(1.0, 2.0, 3.0))
        transforms: dict[str, TransformConfig | None] = {
            "base_link": TransformConfig(visible=True),
        }
        topics: dict[str, TopicsConfig | None] = {
            "/points": TopicsConfig(visible=True),
        }
        layers: dict[
            str,
            LayersConfig
            | GridLayerConfig
            | TiledMapLayerConfig
            | UrdfLayerConfig
            | None,
        ] = {
            "grid": GridLayerConfig(
                instance_id="grid1",
                size=20.0,
                divisions=20,
                color="#248eff",
            ),
            "map": TiledMapLayerConfig(
                instance_id="map1",
                server_config="map",
                map_size_m=500.0,
            ),
            "urdf": UrdfLayerConfig(
                instance_id="urdf1",
                display_mode="auto",
                source_type="topic",
                topic="/robot_description",
            ),
        }
        panel = ThreeDeePanel(
            id="3d-full",
            follow_tf="base_link",
            follow_mode="follow-position",
            location_fix_topic="/gps/fix",
            enu_frame_id="enu",
            scene=scene,
            camera_state=camera_state,
            transforms=transforms,
            topics=topics,
            layers=layers,
            foxglove_panel_title="3D View",
        )
        result = panel.to_dict()
        assert result["id"] == "3d-full"
        assert result["config"]["followTf"] == "base_link"
        assert result["config"]["followMode"] == "follow-position"
        assert result["config"]["locationFixTopic"] == "/gps/fix"
        assert result["config"]["enuFrameId"] == "enu"
        assert result["config"]["foxglovePanelTitle"] == "3D View"
        assert result["config"]["scene"]["enableStats"] is True
        assert result["config"]["scene"]["transforms"]["labelSize"] == 12.0
        assert result["config"]["cameraState"]["distance"] == 50.0
        assert "base_link" in result["config"]["transforms"]
        assert "/points" in result["config"]["topics"]
        assert "grid" in result["config"]["layers"]
        assert "map" in result["config"]["layers"]
        assert "urdf" in result["config"]["layers"]
        assert result["config"]["layers"]["grid"]["layerId"] == "foxglove.Grid"
        assert result["config"]["layers"]["map"]["layerId"] == "foxglove.TiledMap"
        assert result["config"]["layers"]["urdf"]["layerId"] == "foxglove.Urdf"

    def test_to_dict_filters_none_transforms_topics_layers(self) -> None:
        transforms: dict[str, TransformConfig | None] = {
            "frame1": TransformConfig(visible=True),
            "frame2": None,
        }
        topics: dict[str, TopicsConfig | None] = {
            "/topic1": TopicsConfig(visible=True),
            "/topic2": None,
        }
        layers: dict[
            str,
            LayersConfig
            | GridLayerConfig
            | TiledMapLayerConfig
            | UrdfLayerConfig
            | None,
        ] = {
            "layer1": LayersConfig(
                instance_id="inst1",
                layer_id="layer1",
                label="Layer 1",
            ),
            "layer2": None,
        }
        panel = ThreeDeePanel(
            id="3d-1",
            transforms=transforms,
            topics=topics,
            layers=layers,
        )
        result = panel.to_dict()
        assert "frame1" in result["config"]["transforms"]
        assert "frame2" not in result["config"]["transforms"]
        assert "/topic1" in result["config"]["topics"]
        assert "/topic2" not in result["config"]["topics"]
        assert "layer1" in result["config"]["layers"]
        assert "layer2" not in result["config"]["layers"]

    def test_to_json(self) -> None:
        scene = SceneConfig(enable_stats=True, background_color="#ffffff")
        panel = ThreeDeePanel(id="3d-json", scene=scene)
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "3d-json"
        assert parsed["type"] == "3D"
        assert parsed["config"]["scene"]["enableStats"] is True
        assert parsed["config"]["scene"]["backgroundColor"] == "#ffffff"

    def test_all_follow_modes(self) -> None:
        modes: list[Literal["follow-none", "follow-pose", "follow-position"]] = [
            "follow-none",
            "follow-pose",
            "follow-position",
        ]
        for mode in modes:
            panel = ThreeDeePanel(id=f"3d-{mode}", follow_mode=mode)
            result = panel.to_dict()
            assert result["config"]["followMode"] == mode


class TestBaseCustomState:
    def test_creation_with_defaults(self) -> None:
        state = BaseCustomState()
        result = state.to_dict()
        assert result == {}

    def test_creation_with_all_params(self) -> None:
        state = BaseCustomState(label="Active", color="#00ff00")
        result = state.to_dict()
        assert result["label"] == "Active"
        assert result["color"] == "#00ff00"

    def test_to_dict_filters_none_values(self) -> None:
        state = BaseCustomState(label="Test", color=None)
        result = state.to_dict()
        assert result["label"] == "Test"
        assert "color" not in result


class TestStateTransitionsDiscreteCustomState:
    def test_creation_with_defaults(self) -> None:
        state = StateTransitionsDiscreteCustomState()
        result = state.to_dict()
        assert result["value"] == ""

    def test_creation_with_all_params(self) -> None:
        state = StateTransitionsDiscreteCustomState(
            value="active",
            label="Active State",
            color="#00ff00",
        )
        result = state.to_dict()
        assert result["value"] == "active"
        assert result["label"] == "Active State"
        assert result["color"] == "#00ff00"

    def test_to_dict_filters_none_values(self) -> None:
        state = StateTransitionsDiscreteCustomState(
            value="test", label="Test", color=None
        )
        result = state.to_dict()
        assert result["value"] == "test"
        assert result["label"] == "Test"
        assert "color" not in result


class TestStateTransitionsRangeCustomState:
    def test_creation_with_defaults(self) -> None:
        state = StateTransitionsRangeCustomState()
        result = state.to_dict()
        assert "value" not in result
        assert result["operator"] == "<"

    def test_creation_with_all_params(self) -> None:
        state = StateTransitionsRangeCustomState(
            value=10.5,
            operator=">=",
            label="Threshold",
            color="#ff0000",
        )
        result = state.to_dict()
        assert result["value"] == 10.5
        assert result["operator"] == ">="
        assert result["label"] == "Threshold"
        assert result["color"] == "#ff0000"

    def test_all_operators(self) -> None:
        operators: list[Literal["=", "<", "<=", ">", ">="]] = [
            "=",
            "<",
            "<=",
            ">",
            ">=",
        ]
        for op in operators:
            state = StateTransitionsRangeCustomState(value=5.0, operator=op)
            result = state.to_dict()
            assert result["operator"] == op


class TestStateTransitionsRangeCustomStates:
    def test_creation_with_defaults(self) -> None:
        config = StateTransitionsRangeCustomStates()
        result = config.to_dict()
        assert result["type"] == "range"
        assert result["states"] == []

    def test_creation_with_states(self) -> None:
        states = [
            StateTransitionsRangeCustomState(
                value=0.0, operator="<", label="Low", color="#ff0000"
            ),
            StateTransitionsRangeCustomState(
                value=10.0, operator=">=", label="High", color="#00ff00"
            ),
        ]
        otherwise = BaseCustomState(label="Normal", color="#0000ff")
        config = StateTransitionsRangeCustomStates(states=states, otherwise=otherwise)
        result = config.to_dict()
        assert result["type"] == "range"
        assert len(result["states"]) == 2
        assert result["states"][0]["value"] == 0.0
        assert result["states"][0]["operator"] == "<"
        assert result["states"][1]["value"] == 10.0
        assert result["states"][1]["operator"] == ">="
        assert result["otherwise"]["label"] == "Normal"
        assert result["otherwise"]["color"] == "#0000ff"

    def test_creation_without_otherwise(self) -> None:
        states = [
            StateTransitionsRangeCustomState(value=5.0, operator=">", label="High")
        ]
        config = StateTransitionsRangeCustomStates(states=states, otherwise=None)
        result = config.to_dict()
        assert result["type"] == "range"
        assert len(result["states"]) == 1
        assert "otherwise" not in result or result["otherwise"] == {}


class TestStateTransitionsDiscreteCustomStates:
    def test_creation_with_defaults(self) -> None:
        config = StateTransitionsDiscreteCustomStates()
        result = config.to_dict()
        assert result["type"] == "discrete"
        assert result["states"] == []

    def test_creation_with_states(self) -> None:
        states = [
            StateTransitionsDiscreteCustomState(
                value="idle", label="Idle", color="#ffff00"
            ),
            StateTransitionsDiscreteCustomState(
                value="running", label="Running", color="#00ff00"
            ),
            StateTransitionsDiscreteCustomState(
                value="error", label="Error", color="#ff0000"
            ),
        ]
        config = StateTransitionsDiscreteCustomStates(states=states)
        result = config.to_dict()
        assert result["type"] == "discrete"
        assert len(result["states"]) == 3
        assert result["states"][0]["value"] == "idle"
        assert result["states"][0]["label"] == "Idle"
        assert result["states"][1]["value"] == "running"
        assert result["states"][2]["value"] == "error"


class TestStateTransitionsPath:
    def test_creation_with_required_params(self) -> None:
        path = StateTransitionsPath(value="/topic/state")
        result = path.to_dict()
        assert result["value"] == "/topic/state"
        assert result["enabled"] is True
        assert result["timestampMethod"] == "receiveTime"

    def test_creation_with_all_params(self) -> None:
        discrete_states = StateTransitionsDiscreteCustomStates(
            states=[
                StateTransitionsDiscreteCustomState(value="on", label="On"),
                StateTransitionsDiscreteCustomState(value="off", label="Off"),
            ]
        )
        path = StateTransitionsPath(
            value="/topic/state",
            label="Device State",
            enabled=True,
            timestamp_method="headerStamp",
            timestamp_path="/header/stamp",
            custom_states=discrete_states,
        )
        result = path.to_dict()
        assert result["value"] == "/topic/state"
        assert result["label"] == "Device State"
        assert result["enabled"] is True
        assert result["timestampMethod"] == "headerStamp"
        assert result["timestampPath"] == "/header/stamp"
        assert result["customStates"]["type"] == "discrete"
        assert len(result["customStates"]["states"]) == 2

    def test_creation_with_range_custom_states(self) -> None:
        range_states = StateTransitionsRangeCustomStates(
            states=[
                StateTransitionsRangeCustomState(value=0.0, operator="<", label="Low"),
                StateTransitionsRangeCustomState(
                    value=10.0, operator=">=", label="High"
                ),
            ],
            otherwise=BaseCustomState(label="Normal", color="#00ff00"),
        )
        path = StateTransitionsPath(
            value="/topic/value",
            custom_states=range_states,
        )
        result = path.to_dict()
        assert result["customStates"]["type"] == "range"
        assert len(result["customStates"]["states"]) == 2
        assert result["customStates"]["otherwise"]["label"] == "Normal"

    def test_all_timestamp_methods(self) -> None:
        methods: list[
            Literal["receiveTime", "publishTime", "headerStamp", "customField"]
        ] = [
            "receiveTime",
            "publishTime",
            "headerStamp",
            "customField",
        ]
        for method in methods:
            path = StateTransitionsPath(value="/topic/state", timestamp_method=method)
            result = path.to_dict()
            assert result["timestampMethod"] == method

    def test_to_dict_filters_none_values(self) -> None:
        path = StateTransitionsPath(
            value="/topic/state", label=None, timestamp_path=None
        )
        result = path.to_dict()
        assert "label" not in result
        assert "timestampPath" not in result


class TestStateTransitionsPanel:
    def test_creation_with_defaults(self) -> None:
        panel = StateTransitionsPanel()
        result = panel.to_dict()
        assert result["type"] == "StateTransitions"
        assert result["id"].startswith("StateTransitions!")
        assert result["config"]["paths"] == []
        assert result["config"]["isSynced"] is True

    def test_creation_with_id(self) -> None:
        panel = StateTransitionsPanel(id="custom-id")
        result = panel.to_dict()
        assert result["type"] == "StateTransitions"
        assert result["id"] == "custom-id"

    def test_creation_with_paths(self) -> None:
        path1 = StateTransitionsPath(value="/topic/state1", label="State 1")
        path2 = StateTransitionsPath(value="/topic/state2", label="State 2")
        panel = StateTransitionsPanel(path1, path2, id="state-1")
        result = panel.to_dict()
        assert len(result["config"]["paths"]) == 2
        assert result["config"]["paths"][0]["value"] == "/topic/state1"
        assert result["config"]["paths"][0]["label"] == "State 1"
        assert result["config"]["paths"][1]["value"] == "/topic/state2"
        assert result["config"]["paths"][1]["label"] == "State 2"

    def test_creation_with_config(self) -> None:
        path = StateTransitionsPath(value="/topic/state")
        panel = StateTransitionsPanel(
            path,
            id="state-1",
            is_synced=False,
            x_axis_max_value=100.0,
            x_axis_min_value=0.0,
            x_axis_range=50.0,
            x_axis_label="Time (s)",
            time_window_mode="sliding",
            playback_bar_position="right",
            show_points=True,
            foxglove_panel_title="State Panel",
        )
        result = panel.to_dict()
        assert result["config"]["isSynced"] is False
        assert result["config"]["xAxisMaxValue"] == 100.0
        assert result["config"]["xAxisMinValue"] == 0.0
        assert result["config"]["xAxisRange"] == 50.0
        assert result["config"]["xAxisLabel"] == "Time (s)"
        assert result["config"]["timeWindowMode"] == "sliding"
        assert result["config"]["playbackBarPosition"] == "right"
        assert result["config"]["showPoints"] is True
        assert result["config"]["foxglovePanelTitle"] == "State Panel"

    def test_creation_with_path_with_custom_states(self) -> None:
        discrete_states = StateTransitionsDiscreteCustomStates(
            states=[
                StateTransitionsDiscreteCustomState(value="idle", label="Idle"),
                StateTransitionsDiscreteCustomState(value="running", label="Running"),
            ]
        )
        path = StateTransitionsPath(
            value="/topic/state",
            label="Device State",
            custom_states=discrete_states,
        )
        panel = StateTransitionsPanel(path, id="state-1")
        result = panel.to_dict()
        assert len(result["config"]["paths"]) == 1
        assert result["config"]["paths"][0]["customStates"]["type"] == "discrete"
        assert len(result["config"]["paths"][0]["customStates"]["states"]) == 2

    def test_creation_with_range_custom_states(self) -> None:
        range_states = StateTransitionsRangeCustomStates(
            states=[
                StateTransitionsRangeCustomState(value=0.0, operator="<", label="Low"),
            ],
            otherwise=BaseCustomState(label="Normal"),
        )
        path = StateTransitionsPath(value="/topic/value", custom_states=range_states)
        panel = StateTransitionsPanel(path, id="state-1")
        result = panel.to_dict()
        assert result["config"]["paths"][0]["customStates"]["type"] == "range"
        assert len(result["config"]["paths"][0]["customStates"]["states"]) == 1
        assert "otherwise" in result["config"]["paths"][0]["customStates"]

    def test_all_time_window_modes(self) -> None:
        modes: list[Literal["automatic", "sliding", "fixed"]] = [
            "automatic",
            "sliding",
            "fixed",
        ]
        for mode in modes:
            panel = StateTransitionsPanel(id=f"state-{mode}", time_window_mode=mode)
            result = panel.to_dict()
            assert result["config"]["timeWindowMode"] == mode

    def test_all_playback_bar_positions(self) -> None:
        positions: list[Literal["center", "right"]] = ["center", "right"]
        for position in positions:
            panel = StateTransitionsPanel(
                id=f"state-{position}", playback_bar_position=position
            )
            result = panel.to_dict()
            assert result["config"]["playbackBarPosition"] == position

    def test_to_dict_filters_none_values(self) -> None:
        path = StateTransitionsPath(value="/topic/state")
        panel = StateTransitionsPanel(
            path,
            id="state-1",
            x_axis_max_value=None,
            x_axis_min_value=0.0,
            x_axis_label=None,
        )
        result = panel.to_dict()
        assert "xAxisMaxValue" not in result["config"]
        assert result["config"]["xAxisMinValue"] == 0.0
        assert "xAxisLabel" not in result["config"]

    def test_to_json(self) -> None:
        path = StateTransitionsPath(value="/topic/state", label="State")
        panel = StateTransitionsPanel(path, id="state-json", show_points=True)
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "state-json"
        assert parsed["type"] == "StateTransitions"
        assert parsed["config"]["paths"][0]["value"] == "/topic/state"
        assert parsed["config"]["paths"][0]["label"] == "State"
        assert parsed["config"]["showPoints"] is True


class TestButtonConfig:
    def test_creation_with_all_params(self) -> None:
        config = ButtonConfig(field="linear-x", value=1.5)
        result = config.to_dict()
        assert result["field"] == "linear-x"
        assert result["value"] == 1.5

    def test_all_field_values(self) -> None:
        fields: list[
            Literal[
                "linear-x",
                "linear-y",
                "linear-z",
                "angular-x",
                "angular-y",
                "angular-z",
            ]
        ] = [
            "linear-x",
            "linear-y",
            "linear-z",
            "angular-x",
            "angular-y",
            "angular-z",
        ]
        for field in fields:
            config = ButtonConfig(field=field, value=1.0)
            result = config.to_dict()
            assert result["field"] == field
            assert result["value"] == 1.0


class TestTeleopPanel:
    def test_creation_with_defaults(self) -> None:
        panel = TeleopPanel()
        result = panel.to_dict()
        assert result["type"] == "Teleop"
        assert result["id"].startswith("Teleop!")
        assert result["config"]["publishRate"] == 1
        assert result["config"]["upButton"]["field"] == "linear-x"
        assert result["config"]["upButton"]["value"] == 1
        assert result["config"]["downButton"]["field"] == "linear-x"
        assert result["config"]["downButton"]["value"] == -1
        assert result["config"]["leftButton"]["field"] == "angular-z"
        assert result["config"]["leftButton"]["value"] == 1
        assert result["config"]["rightButton"]["field"] == "angular-z"
        assert result["config"]["rightButton"]["value"] == -1

    def test_creation_with_id(self) -> None:
        panel = TeleopPanel(id="custom-id")
        result = panel.to_dict()
        assert result["type"] == "Teleop"
        assert result["id"] == "custom-id"

    def test_creation_with_config(self) -> None:
        up_button = ButtonConfig(field="linear-y", value=2.0)
        down_button = ButtonConfig(field="linear-y", value=-2.0)
        left_button = ButtonConfig(field="angular-x", value=1.5)
        right_button = ButtonConfig(field="angular-x", value=-1.5)
        panel = TeleopPanel(
            id="teleop-1",
            topic="/cmd_vel",
            publish_rate=5.0,
            up_button=up_button,
            down_button=down_button,
            left_button=left_button,
            right_button=right_button,
            foxglove_panel_title="Robot Control",
        )
        result = panel.to_dict()
        assert result["id"] == "teleop-1"
        assert result["config"]["topic"] == "/cmd_vel"
        assert result["config"]["publishRate"] == 5.0
        assert result["config"]["foxglovePanelTitle"] == "Robot Control"
        assert result["config"]["upButton"]["field"] == "linear-y"
        assert result["config"]["upButton"]["value"] == 2.0
        assert result["config"]["downButton"]["field"] == "linear-y"
        assert result["config"]["downButton"]["value"] == -2.0
        assert result["config"]["leftButton"]["field"] == "angular-x"
        assert result["config"]["leftButton"]["value"] == 1.5
        assert result["config"]["rightButton"]["field"] == "angular-x"
        assert result["config"]["rightButton"]["value"] == -1.5

    def test_to_dict_converts_to_camel_case(self) -> None:
        panel = TeleopPanel(
            id="test",
            topic="/test",
            publish_rate=10.0,
            foxglove_panel_title="Test Panel",
        )
        result = panel.to_dict()
        assert result["config"]["topic"] == "/test"
        assert result["config"]["publishRate"] == 10.0
        assert result["config"]["foxglovePanelTitle"] == "Test Panel"
        assert result["config"]["upButton"]["field"] == "linear-x"
        assert result["config"]["downButton"]["field"] == "linear-x"

    def test_to_dict_filters_none_values(self) -> None:
        panel = TeleopPanel(
            id="test", topic="/test", publish_rate=1.0, foxglove_panel_title=None
        )
        result = panel.to_dict()
        assert result["config"]["topic"] == "/test"
        assert result["config"]["publishRate"] == 1.0
        assert "foxglovePanelTitle" not in result["config"]

    def test_to_dict_with_none_topic(self) -> None:
        panel = TeleopPanel(id="test", topic=None, publish_rate=1.0)
        result = panel.to_dict()
        assert "topic" not in result["config"]
        assert result["config"]["publishRate"] == 1.0

    def test_to_json(self) -> None:
        panel = TeleopPanel(id="teleop-json", topic="/cmd_vel", publish_rate=2.0)
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "teleop-json"
        assert parsed["type"] == "Teleop"
        assert parsed["config"]["topic"] == "/cmd_vel"
        assert parsed["config"]["publishRate"] == 2.0
        assert parsed["config"]["upButton"]["field"] == "linear-x"
        assert parsed["config"]["upButton"]["value"] == 1

    def test_all_button_field_values(self) -> None:
        fields: list[
            Literal[
                "linear-x",
                "linear-y",
                "linear-z",
                "angular-x",
                "angular-y",
                "angular-z",
            ]
        ] = [
            "linear-x",
            "linear-y",
            "linear-z",
            "angular-x",
            "angular-y",
            "angular-z",
        ]
        for field in fields:
            button = ButtonConfig(field=field, value=1.0)
            panel = TeleopPanel(id=f"teleop-{field}", up_button=button)
            result = panel.to_dict()
            assert result["config"]["upButton"]["field"] == field
            assert result["config"]["upButton"]["value"] == 1.0

    def test_custom_button_configs(self) -> None:
        up_button = ButtonConfig(field="linear-z", value=0.5)
        down_button = ButtonConfig(field="linear-z", value=-0.5)
        left_button = ButtonConfig(field="angular-y", value=2.0)
        right_button = ButtonConfig(field="angular-y", value=-2.0)
        panel = TeleopPanel(
            up_button=up_button,
            down_button=down_button,
            left_button=left_button,
            right_button=right_button,
        )
        result = panel.to_dict()
        assert result["config"]["upButton"]["field"] == "linear-z"
        assert result["config"]["upButton"]["value"] == 0.5
        assert result["config"]["downButton"]["field"] == "linear-z"
        assert result["config"]["downButton"]["value"] == -0.5
        assert result["config"]["leftButton"]["field"] == "angular-y"
        assert result["config"]["leftButton"]["value"] == 2.0
        assert result["config"]["rightButton"]["field"] == "angular-y"
        assert result["config"]["rightButton"]["value"] == -2.0


class TestMapCoordinates:
    def test_creation_with_all_params(self) -> None:
        coords = MapCoordinates(lat=37.7749, lon=-122.4194)
        result = coords.to_dict()
        assert result["lat"] == 37.7749
        assert result["lon"] == -122.4194


class TestMapTopicConfig:
    def test_creation_with_defaults(self) -> None:
        config = MapTopicConfig()
        result = config.to_dict()
        assert result["historyMode"] == "all"
        assert result["pointDisplayMode"] == "dot"
        assert result["pointSize"] == 6
        assert "color" not in result

    def test_creation_with_all_params(self) -> None:
        config = MapTopicConfig(
            history_mode="previous",
            point_display_mode="pin",
            point_size=10.0,
            color="#ff0000",
        )
        result = config.to_dict()
        assert result["historyMode"] == "previous"
        assert result["pointDisplayMode"] == "pin"
        assert result["pointSize"] == 10.0
        assert result["color"] == "#ff0000"

    def test_all_history_modes(self) -> None:
        modes: list[Literal["all", "previous", "none"]] = ["all", "previous", "none"]
        for mode in modes:
            config = MapTopicConfig(history_mode=mode)
            result = config.to_dict()
            assert result["historyMode"] == mode

    def test_all_point_display_modes(self) -> None:
        modes: list[Literal["dot", "pin"]] = ["dot", "pin"]
        for mode in modes:
            config = MapTopicConfig(point_display_mode=mode)
            result = config.to_dict()
            assert result["pointDisplayMode"] == mode

    def test_to_dict_filters_none_color(self) -> None:
        config = MapTopicConfig(color=None, point_size=8.0)
        result = config.to_dict()
        assert "color" not in result
        assert result["pointSize"] == 8.0


class TestMapPanel:
    def test_creation_with_defaults(self) -> None:
        panel = MapPanel()
        result = panel.to_dict()
        assert result["type"] == "map"
        assert result["id"].startswith("map!")
        assert result["config"]["layer"] == "map"
        assert result["config"]["zoomLevel"] == 10
        assert result["config"]["maxNativeZoom"] == 18
        assert result["config"]["disabledTopics"] == []
        assert "center" not in result["config"] or result["config"]["center"] is None

    def test_creation_with_id(self) -> None:
        panel = MapPanel(id="custom-id")
        result = panel.to_dict()
        assert result["type"] == "map"
        assert result["id"] == "custom-id"

    def test_creation_with_center(self) -> None:
        center = MapCoordinates(lat=40.7128, lon=-74.0060)
        panel = MapPanel(id="map-1", center=center)
        result = panel.to_dict()
        assert result["id"] == "map-1"
        assert result["config"]["center"]["lat"] == 40.7128
        assert result["config"]["center"]["lon"] == -74.0060

    def test_creation_with_all_params(self) -> None:
        center = MapCoordinates(lat=37.7749, lon=-122.4194)
        topic_config = {
            "/gps": MapTopicConfig(
                history_mode="all",
                point_display_mode="pin",
                point_size=8.0,
                color="#00ff00",
            ),
            "/navsat": MapTopicConfig(
                history_mode="previous",
                point_display_mode="dot",
                point_size=6.0,
            ),
        }
        panel = MapPanel(
            id="map-full",
            center=center,
            custom_tile_url="https://example.com/{z}/{x}/{y}.png",
            disabled_topics=["/old_topic"],
            follow_topic="/gps",
            follow_frame="base_link",
            layer="satellite",
            zoom_level=15.0,
            max_native_zoom=20,
            topic_config=topic_config,
            topic_colors={"/gps": "#ff0000", "/navsat": "#0000ff"},
            foxglove_panel_title="Map View",
        )
        result = panel.to_dict()
        assert result["id"] == "map-full"
        assert result["config"]["center"]["lat"] == 37.7749
        assert result["config"]["center"]["lon"] == -122.4194
        assert (
            result["config"]["customTileUrl"] == "https://example.com/{z}/{x}/{y}.png"
        )
        assert result["config"]["disabledTopics"] == ["/old_topic"]
        assert result["config"]["followTopic"] == "/gps"
        assert result["config"]["followFrame"] == "base_link"
        assert result["config"]["layer"] == "satellite"
        assert result["config"]["zoomLevel"] == 15.0
        assert result["config"]["maxNativeZoom"] == 20
        assert result["config"]["foxglovePanelTitle"] == "Map View"
        assert "/gps" in result["config"]["topicConfig"]
        assert result["config"]["topicConfig"]["/gps"]["historyMode"] == "all"
        assert result["config"]["topicConfig"]["/gps"]["pointDisplayMode"] == "pin"
        assert result["config"]["topicConfig"]["/gps"]["pointSize"] == 8.0
        assert result["config"]["topicConfig"]["/gps"]["color"] == "#00ff00"
        assert result["config"]["topicConfig"]["/navsat"]["historyMode"] == "previous"
        assert result["config"]["topicConfig"]["/navsat"]["pointDisplayMode"] == "dot"
        assert result["config"]["topicColors"]["/gps"] == "#ff0000"
        assert result["config"]["topicColors"]["/navsat"] == "#0000ff"

    def test_creation_without_center(self) -> None:
        panel = MapPanel(
            id="map-no-center",
            layer="custom",
            custom_tile_url="https://tiles.example.com/{z}/{x}/{y}",
        )
        result = panel.to_dict()
        assert result["id"] == "map-no-center"
        assert result["config"]["layer"] == "custom"
        assert (
            result["config"]["customTileUrl"] == "https://tiles.example.com/{z}/{x}/{y}"
        )
        # center should not be in config when None
        assert "center" not in result["config"]

    def test_to_dict_converts_to_camel_case(self) -> None:
        center = MapCoordinates(lat=0.0, lon=0.0)
        panel = MapPanel(
            id="test",
            center=center,
            custom_tile_url="/tiles",
            follow_topic="/topic",
            follow_frame="frame",
            zoom_level=12.0,
            max_native_zoom=19,
            foxglove_panel_title="Test Map",
        )
        result = panel.to_dict()
        assert result["config"]["customTileUrl"] == "/tiles"
        assert result["config"]["followTopic"] == "/topic"
        assert result["config"]["followFrame"] == "frame"
        assert result["config"]["zoomLevel"] == 12.0
        assert result["config"]["maxNativeZoom"] == 19
        assert result["config"]["foxglovePanelTitle"] == "Test Map"

    def test_to_dict_filters_none_values(self) -> None:
        panel = MapPanel(
            id="test",
            follow_topic="/gps",
            custom_tile_url=None,
            follow_frame=None,
            foxglove_panel_title=None,
        )
        result = panel.to_dict()
        assert result["config"]["followTopic"] == "/gps"
        assert "customTileUrl" not in result["config"]
        assert "followFrame" not in result["config"]
        assert "foxglovePanelTitle" not in result["config"]

    def test_all_layer_values(self) -> None:
        layers: list[Literal["map", "satellite", "custom"]] = [
            "map",
            "satellite",
            "custom",
        ]
        for layer in layers:
            panel = MapPanel(id=f"map-{layer}", layer=layer)
            result = panel.to_dict()
            assert result["config"]["layer"] == layer

    def test_all_max_native_zoom_values(self) -> None:
        zoom_levels: list[Literal[18, 19, 20, 21, 22, 23, 24]] = [
            18,
            19,
            20,
            21,
            22,
            23,
            24,
        ]
        for zoom in zoom_levels:
            panel = MapPanel(id=f"map-zoom-{zoom}", max_native_zoom=zoom)
            result = panel.to_dict()
            assert result["config"]["maxNativeZoom"] == zoom

    def test_topic_config_conversion(self) -> None:
        topic_config = {
            "/topic1": MapTopicConfig(history_mode="none", point_display_mode="pin"),
            "/topic2": MapTopicConfig(point_size=12.0, color="#ff00ff"),
        }
        panel = MapPanel(id="map-topics", topic_config=topic_config)
        result = panel.to_dict()
        assert len(result["config"]["topicConfig"]) == 2
        assert result["config"]["topicConfig"]["/topic1"]["historyMode"] == "none"
        assert result["config"]["topicConfig"]["/topic1"]["pointDisplayMode"] == "pin"
        assert result["config"]["topicConfig"]["/topic2"]["pointSize"] == 12.0
        assert result["config"]["topicConfig"]["/topic2"]["color"] == "#ff00ff"

    def test_empty_topic_config(self) -> None:
        panel = MapPanel(id="map-empty", topic_config={})
        result = panel.to_dict()
        assert result["config"]["topicConfig"] == {}

    def test_to_json(self) -> None:
        center = MapCoordinates(lat=45.0, lon=-93.0)
        panel = MapPanel(
            id="map-json", center=center, zoom_level=14.0, layer="satellite"
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "map-json"
        assert parsed["type"] == "map"
        assert parsed["config"]["center"]["lat"] == 45.0
        assert parsed["config"]["center"]["lon"] == -93.0
        assert parsed["config"]["zoomLevel"] == 14.0
        assert parsed["config"]["layer"] == "satellite"

    def test_disabled_topics(self) -> None:
        panel = MapPanel(
            id="map-disabled",
            disabled_topics=["/topic1", "/topic2", "/topic3"],
        )
        result = panel.to_dict()
        assert result["config"]["disabledTopics"] == ["/topic1", "/topic2", "/topic3"]

    def test_topic_colors(self) -> None:
        panel = MapPanel(
            id="map-colors",
            topic_colors={
                "/gps": "#ff0000",
                "/navsat": "#00ff00",
                "/location": "#0000ff",
            },
        )
        result = panel.to_dict()
        assert result["config"]["topicColors"]["/gps"] == "#ff0000"
        assert result["config"]["topicColors"]["/navsat"] == "#00ff00"
        assert result["config"]["topicColors"]["/location"] == "#0000ff"


class TestParametersPanel:
    def test_creation_with_defaults(self) -> None:
        panel = ParametersPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Parameters"
        assert parsed["id"].startswith("Parameters!")
        assert parsed["config"]["title"] == "Parameters"

    def test_creation_with_id(self) -> None:
        panel = ParametersPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Parameters"
        assert parsed["id"] == "custom-id"
        assert parsed["config"]["title"] == "Parameters"


class TestPublishPanel:
    def test_creation_with_defaults(self) -> None:
        panel = PublishPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Publish"
        assert parsed["id"].startswith("Publish!")
        assert parsed["config"]["advancedView"] is True
        assert parsed["config"]["value"] == "{}"

    def test_creation_with_id(self) -> None:
        panel = PublishPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Publish"
        assert parsed["id"] == "custom-id"
        assert parsed["config"]["advancedView"] is True

    def test_creation_with_all_params(self) -> None:
        panel = PublishPanel(
            id="publish-1",
            topic_name="/cmd",
            datatype="std_msgs/String",
            button_text="Send",
            button_tooltip="Click to publish",
            button_color="#ff0000",
            advanced_view=False,
            value='{"data": "test"}',
            foxglove_panel_title="Publish Panel",
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "publish-1"
        assert parsed["config"]["topicName"] == "/cmd"
        assert parsed["config"]["datatype"] == "std_msgs/String"
        assert parsed["config"]["buttonText"] == "Send"
        assert parsed["config"]["buttonTooltip"] == "Click to publish"
        assert parsed["config"]["buttonColor"] == "#ff0000"
        assert parsed["config"]["advancedView"] is False
        assert parsed["config"]["value"] == '{"data": "test"}'
        assert parsed["config"]["foxglovePanelTitle"] == "Publish Panel"


class TestServiceCallPanel:
    def test_creation_with_defaults(self) -> None:
        panel = ServiceCallPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "CallService"
        assert parsed["id"].startswith("CallService!")
        assert parsed["config"]["requestPayload"] == "{}"
        assert parsed["config"]["layout"] == "vertical"
        assert parsed["config"]["editingMode"] is True
        assert parsed["config"]["timeoutSeconds"] == 10

    def test_creation_with_id(self) -> None:
        panel = ServiceCallPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "CallService"
        assert parsed["id"] == "custom-id"
        assert parsed["config"]["layout"] == "vertical"

    def test_creation_with_all_params(self) -> None:
        panel = ServiceCallPanel(
            id="service-1",
            service_name="/service/example",
            request_payload='{"key": "value"}',
            layout="horizontal",
            button_text="Call Service",
            button_tooltip="Click to call service",
            button_color="#00ff00",
            editing_mode=False,
            timeout_seconds=30,
            foxglove_panel_title="Service Call Panel",
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "service-1"
        assert parsed["config"]["serviceName"] == "/service/example"
        assert parsed["config"]["requestPayload"] == '{"key": "value"}'
        assert parsed["config"]["layout"] == "horizontal"
        assert parsed["config"]["buttonText"] == "Call Service"
        assert parsed["config"]["buttonTooltip"] == "Click to call service"
        assert parsed["config"]["buttonColor"] == "#00ff00"
        assert parsed["config"]["editingMode"] is False
        assert parsed["config"]["timeoutSeconds"] == 30
        assert parsed["config"]["foxglovePanelTitle"] == "Service Call Panel"


class TestNameFilter:
    def test_creation_with_defaults(self) -> None:
        filter_obj = NameFilter()
        result = filter_obj.to_dict()
        assert result["visible"] is True

    def test_creation_with_false(self) -> None:
        filter_obj = NameFilter(visible=False)
        result = filter_obj.to_dict()
        assert result["visible"] is False

    def test_to_dict_filters_none_values(self) -> None:
        filter_obj = NameFilter(visible=None)
        result = filter_obj.to_dict()
        assert "visible" not in result


class TestLogPanel:
    def test_creation_with_defaults(self) -> None:
        panel = LogPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "RosOut"
        assert parsed["id"].startswith("RosOut!")
        assert parsed["config"]["searchTerms"] == []
        assert parsed["config"]["minLogLevel"] == 1
        assert parsed["config"]["fontSize"] == 12

    def test_creation_with_id(self) -> None:
        panel = LogPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "RosOut"
        assert parsed["id"] == "custom-id"
        assert parsed["config"]["minLogLevel"] == 1

    def test_creation_with_all_params(self) -> None:
        name_filter = {
            "node1": NameFilter(visible=True),
            "node2": NameFilter(visible=False),
        }
        panel = LogPanel(
            id="log-1",
            search_terms=["error", "warning"],
            min_log_level=3,
            topic_to_render="/rosout",
            name_filter=name_filter,
            font_size=16,
            foxglove_panel_title="Log Panel",
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "log-1"
        assert parsed["config"]["searchTerms"] == ["error", "warning"]
        assert parsed["config"]["minLogLevel"] == 3
        assert parsed["config"]["topicToRender"] == "/rosout"
        assert parsed["config"]["fontSize"] == 16
        assert parsed["config"]["foxglovePanelTitle"] == "Log Panel"
        assert "nameFilter" in parsed["config"]
        assert parsed["config"]["nameFilter"]["node1"]["visible"] is True
        assert parsed["config"]["nameFilter"]["node2"]["visible"] is False


class TestTablePanel:
    def test_creation_with_defaults(self) -> None:
        panel = TablePanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Table"
        assert parsed["id"].startswith("Table!")
        assert "topicPath" not in parsed["config"]

    def test_creation_with_id(self) -> None:
        panel = TablePanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "Table"
        assert parsed["id"] == "custom-id"
        assert "topicPath" not in parsed["config"]

    def test_creation_with_all_params(self) -> None:
        panel = TablePanel(
            id="table-1",
            topic_path="/camera/ring_front_center/camera_info",
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "table-1"


class TestTopicGraphPanel:
    def test_creation_with_defaults(self) -> None:
        panel = TopicGraphPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "TopicGraph"
        assert parsed["id"].startswith("TopicGraph!")

    def test_creation_with_id(self) -> None:
        panel = TopicGraphPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "TopicGraph"
        assert parsed["id"] == "custom-id"


class TestTransformTreePanel:
    def test_creation_with_defaults(self) -> None:
        panel = TransformTreePanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "TransformTree"
        assert parsed["id"].startswith("TransformTree!")

    def test_creation_with_id(self) -> None:
        panel = TransformTreePanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "TransformTree"
        assert parsed["id"] == "custom-id"


class TestDataSourceInfoPanel:
    def test_creation_with_defaults(self) -> None:
        panel = DataSourceInfoPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "SourceInfo"
        assert parsed["id"].startswith("SourceInfo!")

    def test_creation_with_id(self) -> None:
        panel = DataSourceInfoPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "SourceInfo"
        assert parsed["id"] == "custom-id"


class TestVariableSliderPanel:
    def test_creation_with_defaults(self) -> None:
        panel = VariableSliderPanel()
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "GlobalVariableSliderPanel"
        assert parsed["id"].startswith("GlobalVariableSliderPanel!")

    def test_creation_with_id(self) -> None:
        panel = VariableSliderPanel(id="custom-id")
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["type"] == "GlobalVariableSliderPanel"
        assert parsed["id"] == "custom-id"

    def test_creation_with_all_params(self) -> None:
        panel = VariableSliderPanel(
            id="variable-slider",
            global_variable_name="globalVariable1",
            slider_props=VariableSliderConfig(min=1, max=15, step=1),
        )
        json_str = panel.to_json()
        parsed = json.loads(json_str)
        assert parsed["id"] == "variable-slider"
        assert parsed["config"]["globalVariableName"] == "globalVariable1"
        assert parsed["config"]["sliderProps"]["min"] == 1
        assert parsed["config"]["sliderProps"]["max"] == 15
        assert parsed["config"]["sliderProps"]["step"] == 1


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
            ImagePanel(
                id="image",
                image_mode=ImageModeConfig(image_topic="/camera/image"),
            ),
            ThreeDeePanel(id="3d"),
            StateTransitionsPanel(id="state"),
            TeleopPanel(id="teleop", topic="/cmd_vel"),
            MapPanel(
                id="map",
                center=MapCoordinates(lat=37.7749, lon=-122.4194),
            ),
            ParametersPanel(id="params"),
            PublishPanel(id="publish", topic_name="/topic"),
            ServiceCallPanel(id="service", service_name="/service"),
            LogPanel(id="log", topic_to_render="/rosout"),
            TablePanel(id="table", topic_path="/camera/ring_front_center/camera_info"),
            TopicGraphPanel(id="table"),
            TransformTreePanel(id="transform-tree"),
            DataSourceInfoPanel(id="source-info"),
            VariableSliderPanel(id="variable-slider"),
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
