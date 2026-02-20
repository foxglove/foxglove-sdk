export type Example = {
  label: string;
  code: string;
  layout?: Record<string, unknown>;
};

const SCENE_EXAMPLE: Example = {
  label: "3D Scene",
  code: `\
import foxglove
from foxglove.channels import SceneUpdateChannel
from foxglove.schemas import (
  Color,
  CubePrimitive,
  SceneEntity,
  SceneUpdate,
  Vector3,
)

scene_channel = SceneUpdateChannel("/scene")

with foxglove.open_mcap("playground.mcap") as writer:
  for i in range(10):
    size = 1 + 0.2 * i
    scene_channel.log(
      SceneUpdate(
        entities=[
          SceneEntity(
            cubes=[
              CubePrimitive(
                size=Vector3(x=size, y=size, z=size),
                color=Color(r=1.0, g=0, b=0, a=1.0),
              )
            ],
          ),
        ]
      ),
      log_time=i * 200_000_000,
    )
`,
  layout: {
    configById: {
      "3D!1ehnpb2": {
        cameraState: {
          distance: 20,
          perspective: true,
          phi: 60,
          target: [0, 0, 0],
          targetOffset: [0, 0, 0],
          targetOrientation: [0, 0, 0, 1],
          thetaOffset: 45,
          fovy: 45,
          near: 0.5,
          far: 5000,
        },
        followMode: "follow-pose",
        scene: {},
        transforms: {},
        topics: {
          "/scene": {
            visible: true,
          },
        },
        layers: {
          grid: {
            visible: true,
            drawBehind: false,
            frameLocked: true,
            label: "Grid",
            instanceId: "7cfdaa56-0cc3-4576-b763-5a8882575cd4",
            layerId: "foxglove.Grid",
            size: 10,
            divisions: 10,
            lineWidth: 1,
            color: "#248eff",
            position: [0, 0, 0],
            rotation: [0, 0, 0],
          },
        },
        publish: {
          type: "point",
          poseTopic: "/move_base_simple/goal",
          pointTopic: "/clicked_point",
          poseEstimateTopic: "/initialpose",
          poseEstimateXDeviation: 0.5,
          poseEstimateYDeviation: 0.5,
          poseEstimateThetaDeviation: 0.26179939,
        },
        imageMode: {},
      },
      "RawMessages!2zn7j4u": {
        diffEnabled: false,
        diffMethod: "custom",
        diffTopicPath: "",
        showFullMessageForDiff: false,
        topicPath: "/scene",
        fontSize: 12,
      },
      "Plot!30ea437": {
        paths: [
          {
            value: "/scene.entities[:].cubes[0].size.x",
            enabled: true,
            timestampMethod: "receiveTime",
            label: "Cube size",
          },
        ],
        showXAxisLabels: true,
        showYAxisLabels: true,
        showLegend: true,
        legendDisplay: "floating",
        showPlotValuesInLegend: false,
        isSynced: true,
        xAxisVal: "timestamp",
        sidebarDimension: 240,
      },
    },
    globalVariables: {},
    userNodes: {},
    playbackConfig: {
      speed: 1,
    },
    drawerConfig: {
      tracks: [],
    },
    layout: {
      first: "3D!1ehnpb2",
      second: {
        direction: "row",
        second: "Plot!30ea437",
        first: "RawMessages!2zn7j4u",
        splitPercentage: 33,
      },
      direction: "column",
      splitPercentage: 60.57971014492753,
    },
  },
};

const TRANSFORMS_EXAMPLE: Example = {
  label: "Transforms & Frames",
  code: `\
import math

import foxglove
from foxglove.channels import FrameTransformChannel, SceneUpdateChannel, LogChannel
from foxglove.schemas import (
  Color,
  CubePrimitive,
  FrameTransform,
  Log,
  LogLevel,
  Pose,
  Quaternion,
  SceneEntity,
  SceneUpdate,
  Vector3,
)


def euler_to_quaternion(roll: float, pitch: float, yaw: float) -> Quaternion:
    cr, sr = math.cos(roll / 2), math.sin(roll / 2)
    cp, sp = math.cos(pitch / 2), math.sin(pitch / 2)
    cy, sy = math.cos(yaw / 2), math.sin(yaw / 2)
    return Quaternion(
        w=cr * cp * cy + sr * sp * sy,
        x=sr * cp * cy - cr * sp * sy,
        y=cr * sp * cy + sr * cp * sy,
        z=cr * cp * sy - sr * sp * cy,
    )


boxes_channel = SceneUpdateChannel("/boxes")
tf_channel = FrameTransformChannel("/tf")
log_channel = LogChannel("/log")

with foxglove.open_mcap("playground.mcap") as writer:
  for i in range(60):
    t = i * 100_000_000

    tf_channel.log(
      FrameTransform(
        parent_frame_id="world",
        child_frame_id="rotating",
        rotation=euler_to_quaternion(0.5, 0.0, i * 0.1),
      ),
      log_time=t,
    )

    boxes_channel.log(
      SceneUpdate(
        entities=[
          SceneEntity(
            frame_id="rotating",
            id="box_1",
            cubes=[
              CubePrimitive(
                pose=Pose(
                  position=Vector3(x=0.0, y=0.0, z=2.0),
                  orientation=euler_to_quaternion(0.0, 0.0, -i * 0.1),
                ),
                size=Vector3(x=1.0, y=1.0, z=1.0),
                color=Color(r=1.0, g=0.2, b=0.2, a=1.0),
              ),
            ],
          ),
          SceneEntity(
            frame_id="world",
            id="box_2",
            cubes=[
              CubePrimitive(
                pose=Pose(
                  position=Vector3(
                    x=3.0 * math.cos(i * 0.05),
                    y=3.0 * math.sin(i * 0.05),
                    z=0.5,
                  ),
                  orientation=euler_to_quaternion(0.0, 0.0, i * 0.05),
                ),
                size=Vector3(x=0.8, y=0.8, z=0.8),
                color=Color(r=0.2, g=0.6, b=1.0, a=1.0),
              ),
            ],
          ),
        ],
      ),
      log_time=t,
    )

    log_channel.log(
      Log(
        level=LogLevel.Info,
        message=f"Frame {i}: yaw={i * 0.1:.2f} rad",
        name="transforms_example",
      ),
      log_time=t,
    )
`,
  layout: {
    configById: {
      "3D!tf3d": {
        cameraState: {
          distance: 15,
          perspective: true,
          phi: 50,
          target: [0, 0, 0],
          targetOffset: [0, 0, 0],
          targetOrientation: [0, 0, 0, 1],
          thetaOffset: 30,
          fovy: 45,
          near: 0.5,
          far: 5000,
        },
        followMode: "follow-pose",
        scene: {},
        transforms: {},
        topics: {
          "/boxes": { visible: true },
        },
        layers: {
          grid: {
            visible: true,
            drawBehind: false,
            frameLocked: true,
            label: "Grid",
            instanceId: "tf-grid-layer",
            layerId: "foxglove.Grid",
            size: 10,
            divisions: 10,
            lineWidth: 1,
            color: "#248eff",
            position: [0, 0, 0],
            rotation: [0, 0, 0],
          },
        },
        publish: {
          type: "point",
          poseTopic: "/move_base_simple/goal",
          pointTopic: "/clicked_point",
          poseEstimateTopic: "/initialpose",
          poseEstimateXDeviation: 0.5,
          poseEstimateYDeviation: 0.5,
          poseEstimateThetaDeviation: 0.26179939,
        },
        imageMode: {},
      },
      "Log!tflog": {
        topicToRender: "/log",
        fontSize: 12,
      },
    },
    globalVariables: {},
    userNodes: {},
    playbackConfig: { speed: 1 },
    drawerConfig: { tracks: [] },
    layout: {
      first: "3D!tf3d",
      second: "Log!tflog",
      direction: "column",
      splitPercentage: 70,
    },
  },
};

const LAYOUT_API_EXAMPLE: Example = {
  label: "Layout API",
  code: `\
import math

import foxglove
from foxglove.channels import SceneUpdateChannel, LogChannel
from foxglove.schemas import (
  Color,
  CubePrimitive,
  CylinderPrimitive,
  Log,
  LogLevel,
  Pose,
  Quaternion,
  SceneEntity,
  SceneUpdate,
  SpherePrimitive,
  Vector3,
)
from foxglove.layouts import (
  Layout,
  PlotConfig,
  PlotPanel,
  PlotSeries,
  RawMessagesConfig,
  RawMessagesPanel,
  SplitContainer,
  SplitItem,
  ThreeDeeConfig,
  ThreeDeeCameraState,
  ThreeDeePanel,
  BaseRendererSceneUpdateTopicSettings,
  BaseRendererGridLayerSettings,
  LogConfig,
  LogPanel,
)

# Configure the layout programmatically
playground.set_layout(
  Layout(
    content=SplitContainer(
      direction="column",
      items=[
        SplitItem(
          proportion=2,
          content=SplitContainer(
            direction="row",
            items=[
              SplitItem(
                proportion=2,
                content=ThreeDeePanel(
                  title="3D View",
                  config=ThreeDeeConfig(
                    camera_state=ThreeDeeCameraState(
                      distance=20,
                      perspective=True,
                      phi=55,
                      theta_offset=40,
                      fovy=45,
                      near=0.5,
                      far=5000,
                    ),
                    follow_mode="follow-pose",
                    topics={
                      "/shapes": BaseRendererSceneUpdateTopicSettings(
                        visible=True,
                      ),
                    },
                    layers={
                      "grid": BaseRendererGridLayerSettings(
                        visible=True,
                        instance_id="layout-grid",
                        size=10.0,
                        divisions=10.0,
                        line_width=1.0,
                        color="#248eff",
                      ),
                    },
                  ),
                ),
              ),
              SplitItem(
                proportion=1,
                content=RawMessagesPanel(
                  config=RawMessagesConfig(
                    topic_path="/shapes",
                    font_size=12.0,
                  ),
                ),
              ),
            ],
          ),
        ),
        SplitItem(
          proportion=1,
          content=SplitContainer(
            direction="row",
            items=[
              SplitItem(
                proportion=1,
                content=PlotPanel(
                  title="Sphere Trajectory",
                  config=PlotConfig(
                    paths=[
                      PlotSeries(
                        value="/shapes.entities[:]{id=='sphere'}.spheres[0].size.radius",
                        enabled=True,
                        label="Radius",
                      ),
                    ],
                    show_legend=True,
                    legend_display="floating",
                    show_x_axis_labels=True,
                    show_y_axis_labels=True,
                    is_synced=True,
                    x_axis_val="timestamp",
                  ),
                ),
              ),
              SplitItem(
                proportion=1,
                content=LogPanel(
                  title="Logs",
                  config=LogConfig(
                    topic_to_render="/log",
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    ),
  )
)

# Generate data with multiple shape types
shapes_channel = SceneUpdateChannel("/shapes")
log_channel = LogChannel("/log")

with foxglove.open_mcap("playground.mcap") as writer:
  for i in range(40):
    t = i * 150_000_000
    phase = i * 0.15

    radius = 0.3 + 0.2 * math.sin(phase)
    cube_size = 0.8 + 0.4 * math.cos(phase)

    shapes_channel.log(
      SceneUpdate(
        entities=[
          SceneEntity(
            id="cube",
            cubes=[
              CubePrimitive(
                pose=Pose(
                  position=Vector3(x=-2.0, y=0.0, z=cube_size / 2),
                ),
                size=Vector3(x=cube_size, y=cube_size, z=cube_size),
                color=Color(
                  r=0.5 + 0.5 * math.sin(phase),
                  g=0.3,
                  b=0.5 + 0.5 * math.cos(phase),
                  a=1.0,
                ),
              ),
            ],
          ),
          SceneEntity(
            id="sphere",
            spheres=[
              SpherePrimitive(
                pose=Pose(
                  position=Vector3(x=2.0, y=0.0, z=1.0),
                ),
                size=Vector3(x=radius * 2, y=radius * 2, z=radius * 2),
                color=Color(r=0.2, g=0.8, b=0.4, a=1.0),
              ),
            ],
          ),
          SceneEntity(
            id="cylinder",
            cylinders=[
              CylinderPrimitive(
                pose=Pose(
                  position=Vector3(
                    x=0.0,
                    y=3.0 * math.sin(phase * 0.5),
                    z=1.0,
                  ),
                ),
                size=Vector3(x=0.6, y=0.6, z=2.0),
                color=Color(r=1.0, g=0.6, b=0.1, a=1.0),
              ),
            ],
          ),
        ],
      ),
      log_time=t,
    )

    log_channel.log(
      Log(
        level=LogLevel.Info,
        message=f"Step {i}: radius={radius:.2f}, cube={cube_size:.2f}",
        name="layout_example",
      ),
      log_time=t,
    )
`,
};

export const EXAMPLES: Example[] = [SCENE_EXAMPLE, TRANSFORMS_EXAMPLE, LAYOUT_API_EXAMPLE];

export const DEFAULT_EXAMPLE = EXAMPLES[0]!;

/** A minimal single-panel layout used as fallback when an example sets its layout programmatically. */
export const FALLBACK_LAYOUT: Record<string, unknown> = {
  configById: {
    "3D!fallback": {
      cameraState: {
        distance: 20,
        perspective: true,
        phi: 60,
        target: [0, 0, 0],
        targetOffset: [0, 0, 0],
        targetOrientation: [0, 0, 0, 1],
        thetaOffset: 45,
        fovy: 45,
        near: 0.5,
        far: 5000,
      },
      followMode: "follow-pose",
      scene: {},
      transforms: {},
      topics: {},
      layers: {
        grid: {
          visible: true,
          drawBehind: false,
          frameLocked: true,
          label: "Grid",
          instanceId: "fallback-grid",
          layerId: "foxglove.Grid",
          size: 10,
          divisions: 10,
          lineWidth: 1,
          color: "#248eff",
          position: [0, 0, 0],
          rotation: [0, 0, 0],
        },
      },
      imageMode: {},
    },
  },
  globalVariables: {},
  userNodes: {},
  playbackConfig: { speed: 1 },
  drawerConfig: { tracks: [] },
  layout: "3D!fallback",
};
