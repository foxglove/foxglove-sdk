/// This file is the source of truth for all generated Foxglove schemas.
/// It is used to generate the JSON schema, FlatBuffer, Protobuf,and TypeScript definitions.
///
/// Edit this file and run `make generate` to regenerate all the various schema files.

import { FoxgloveEnumSchema, FoxgloveMessageSchema } from "./types";

const Duration: FoxgloveMessageSchema = {
  type: "message",
  name: "Duration",
  description: "A duration of time, composed of seconds and nanoseconds",
  rosEquivalent: "std_msgs/Duration",
  ros2Equivalent: "builtin_interfaces/Duration",
  protoEquivalent: "google.protobuf.Duration",
  fields: [
    {
      name: "sec",
      type: { type: "primitive", name: "int32" },
      description: "The number of seconds in the duration",
    },
    {
      name: "nsec",
      type: { type: "primitive", name: "uint32" },
      description: "The number of nanoseconds in the positive direction",
    },
  ],
};

const Timestamp: FoxgloveMessageSchema = {
  type: "message",
  name: "Timestamp",
  description: "A timestamp composed of seconds and nanoseconds",
  rosEquivalent: "std_msgs/Time",
  ros2Equivalent: "builtin_interfaces/Time",
  protoEquivalent: "google.protobuf.Timestamp",
  fields: [
    {
      name: "sec",
      type: { type: "primitive", name: "uint32" },
      description: "The number of seconds since a user-defined epoch",
    },
    {
      name: "nsec",
      type: { type: "primitive", name: "uint32" },
      description: "The number of nanoseconds since the sec value",
    },
  ],
};

const RawAudio: FoxgloveMessageSchema = {
  type: "message",
  name: "RawAudio",
  description: "A single block of an audio bitstream",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of the start of the audio block",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description: `Audio data. The samples in the data must be interleaved and little-endian`,
    },
    {
      name: "format",
      type: { type: "primitive", name: "string" },
      description: "Audio format. Only 'pcm-s16' is currently supported",
    },
    {
      name: "sample_rate",
      type: { type: "primitive", name: "uint32" },
      description: "Sample rate in Hz",
    },
    {
      name: "number_of_channels",
      type: { type: "primitive", name: "uint32" },
      description: "Number of channels in the audio block",
    },
  ],
};

const Color: FoxgloveMessageSchema = {
  type: "message",
  name: "Color",
  description: "A color in RGBA format",
  fields: [
    {
      name: "r",
      type: { type: "primitive", name: "float64" },
      description: "Red value between 0 and 1",
      defaultValue: 1.0,
    },
    {
      name: "g",
      type: { type: "primitive", name: "float64" },
      description: "Green value between 0 and 1",
      defaultValue: 1.0,
    },
    {
      name: "b",
      type: { type: "primitive", name: "float64" },
      description: "Blue value between 0 and 1",
      defaultValue: 1.0,
    },
    {
      name: "a",
      type: { type: "primitive", name: "float64" },
      description: "Alpha value between 0 and 1",
      defaultValue: 1.0,
    },
  ],
};

const Vector2: FoxgloveMessageSchema = {
  type: "message",
  name: "Vector2",
  description: "A vector in 2D space that represents a direction only",
  fields: [
    {
      name: "x",
      type: { type: "primitive", name: "float64" },
      description: "x coordinate length",
      defaultValue: 1.0,
    },
    {
      name: "y",
      type: { type: "primitive", name: "float64" },
      description: "y coordinate length",
      defaultValue: 1.0,
    },
  ],
};

const Vector3: FoxgloveMessageSchema = {
  type: "message",
  name: "Vector3",
  description: "A vector in 3D space that represents a direction only",
  rosEquivalent: "geometry_msgs/Vector3",
  ros2Equivalent: "geometry_msgs/Vector3",
  fields: [
    {
      name: "x",
      type: { type: "primitive", name: "float64" },
      description: "x coordinate length",
      defaultValue: 1.0,
    },
    {
      name: "y",
      type: { type: "primitive", name: "float64" },
      description: "y coordinate length",
      defaultValue: 1.0,
    },
    {
      name: "z",
      type: { type: "primitive", name: "float64" },
      description: "z coordinate length",
      defaultValue: 1.0,
    },
  ],
};

const Point2: FoxgloveMessageSchema = {
  type: "message",
  name: "Point2",
  description: "A point representing a position in 2D space",
  fields: [
    {
      name: "x",
      type: { type: "primitive", name: "float64" },
      description: "x coordinate position",
    },
    {
      name: "y",
      type: { type: "primitive", name: "float64" },
      description: "y coordinate position",
    },
  ],
};

const Point3: FoxgloveMessageSchema = {
  type: "message",
  name: "Point3",
  description: "A point representing a position in 3D space",
  rosEquivalent: "geometry_msgs/Point",
  ros2Equivalent: "geometry_msgs/Point",
  fields: [
    {
      name: "x",
      type: { type: "primitive", name: "float64" },
      description: "x coordinate position",
    },
    {
      name: "y",
      type: { type: "primitive", name: "float64" },
      description: "y coordinate position",
    },
    {
      name: "z",
      type: { type: "primitive", name: "float64" },
      description: "z coordinate position",
    },
  ],
};

const Point3InFrame: FoxgloveMessageSchema = {
  type: "message",
  name: "Point3InFrame",
  description: "A timestamped point for a position in 3D space",
  rosEquivalent: "geometry_msgs/PointStamped",
  ros2Equivalent: "geometry_msgs/PointStamped",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of point",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference for point position",
    },
    {
      name: "point",
      type: { type: "nested", schema: Point3 },
      description: "Point in 3D space",
    },
  ],
};

const Quaternion: FoxgloveMessageSchema = {
  type: "message",
  name: "Quaternion",
  description: "A [quaternion](https://eater.net/quaternions) representing a rotation in 3D space",
  rosEquivalent: "geometry_msgs/Quaternion",
  ros2Equivalent: "geometry_msgs/Quaternion",
  fields: [
    {
      name: "x",
      type: { type: "primitive", name: "float64" },
      description: "x value",
    },
    {
      name: "y",
      type: { type: "primitive", name: "float64" },
      description: "y value",
    },
    {
      name: "z",
      type: { type: "primitive", name: "float64" },
      description: "z value",
    },
    {
      name: "w",
      type: { type: "primitive", name: "float64" },
      description: "w value",
      defaultValue: 1.0,
    },
  ],
};

const Pose: FoxgloveMessageSchema = {
  type: "message",
  name: "Pose",
  description: "A position and orientation for an object or reference frame in 3D space",
  rosEquivalent: "geometry_msgs/Pose",
  ros2Equivalent: "geometry_msgs/Pose",
  fields: [
    {
      name: "position",
      type: { type: "nested", schema: Vector3 },
      description: "Point denoting position in 3D space",
    },
    {
      name: "orientation",
      type: { type: "nested", schema: Quaternion },
      description: "Quaternion denoting orientation in 3D space",
    },
  ],
};

const KeyValuePair: FoxgloveMessageSchema = {
  type: "message",
  name: "KeyValuePair",
  description: "A key with its associated value",
  fields: [
    {
      name: "key",
      type: { type: "primitive", name: "string" },
      description: "Key",
    },
    {
      name: "value",
      type: { type: "primitive", name: "string" },
      description: "Value",
    },
  ],
};

const SceneEntityDeletionType: FoxgloveEnumSchema = {
  type: "enum",
  name: "SceneEntityDeletionType",
  parentSchemaName: "SceneEntityDeletion",
  protobufEnumName: "Type",
  description:
    "An enumeration indicating which entities should match a SceneEntityDeletion command",
  values: [
    {
      value: 0,
      name: "MATCHING_ID",
      description: "Delete the existing entity on the same topic that has the provided `id`",
    },
    { value: 1, name: "ALL", description: "Delete all existing entities on the same topic" },
  ],
};

const SceneEntityDeletion: FoxgloveMessageSchema = {
  type: "message",
  name: "SceneEntityDeletion",
  description: "Command to remove previously published entities",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description:
        "Timestamp of the deletion. Only matching entities earlier than this timestamp will be deleted.",
    },
    {
      name: "type",
      type: { type: "enum", enum: SceneEntityDeletionType },
      description: "Type of deletion action to perform",
    },
    {
      name: "id",
      type: { type: "primitive", name: "string" },
      description: "Identifier which must match if `type` is `MATCHING_ID`.",
    },
  ],
};

const ArrowPrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "ArrowPrimitive",
  description: "A primitive representing an arrow",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Position of the arrow's tail and orientation of the arrow. Identity orientation means the arrow points in the +x direction.",
    },
    {
      name: "shaft_length",
      type: { type: "primitive", name: "float64" },
      description: "Length of the arrow shaft",
    },
    {
      name: "shaft_diameter",
      type: { type: "primitive", name: "float64" },
      description: "Diameter of the arrow shaft",
    },
    {
      name: "head_length",
      type: { type: "primitive", name: "float64" },
      description: "Length of the arrow head",
    },
    {
      name: "head_diameter",
      type: { type: "primitive", name: "float64" },
      description: "Diameter of the arrow head",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color of the arrow",
    },
  ],
};

const CubePrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "CubePrimitive",
  description: "A primitive representing a cube or rectangular prism",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Position of the center of the cube and orientation of the cube",
    },
    {
      name: "size",
      type: { type: "nested", schema: Vector3 },
      description: "Size of the cube along each axis",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color of the cube",
    },
  ],
};

const SpherePrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "SpherePrimitive",
  description: "A primitive representing a sphere or ellipsoid",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Position of the center of the sphere and orientation of the sphere",
    },
    {
      name: "size",
      type: { type: "nested", schema: Vector3 },
      description: "Size (diameter) of the sphere along each axis",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color of the sphere",
    },
  ],
};

const CylinderPrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "CylinderPrimitive",
  description: "A primitive representing a cylinder, elliptic cylinder, or truncated cone",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Position of the center of the cylinder and orientation of the cylinder. The flat face(s) are perpendicular to the z-axis.",
    },
    {
      name: "size",
      type: { type: "nested", schema: Vector3 },
      description: "Size of the cylinder's bounding box",
    },
    {
      name: "bottom_scale",
      type: { type: "primitive", name: "float64" },
      description:
        "0-1, ratio of the diameter of the cylinder's bottom face (min z) to the bottom of the bounding box",
    },
    {
      name: "top_scale",
      type: { type: "primitive", name: "float64" },
      description:
        "0-1, ratio of the diameter of the cylinder's top face (max z) to the top of the bounding box",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color of the cylinder",
    },
  ],
};

const LineType: FoxgloveEnumSchema = {
  type: "enum",
  name: "LineType",
  parentSchemaName: "LinePrimitive",
  protobufEnumName: "Type",
  description: "An enumeration indicating how input points should be interpreted to create lines",
  values: [
    {
      value: 0,
      name: "LINE_STRIP",
      description: "Connected line segments: 0-1, 1-2, ..., (n-1)-n",
    },
    { value: 1, name: "LINE_LOOP", description: "Closed polygon: 0-1, 1-2, ..., (n-1)-n, n-0" },
    { value: 2, name: "LINE_LIST", description: "Individual line segments: 0-1, 2-3, 4-5, ..." },
  ],
};

const LinePrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "LinePrimitive",
  description: "A primitive representing a series of points connected by lines",
  fields: [
    {
      name: "type",
      type: { type: "enum", enum: LineType },
      description: "Drawing primitive to use for lines",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Origin of lines relative to reference frame",
    },
    {
      name: "thickness",
      type: { type: "primitive", name: "float64" },
      description: "Line thickness",
    },
    {
      name: "scale_invariant",
      type: { type: "primitive", name: "boolean" },
      description:
        "Indicates whether `thickness` is a fixed size in screen pixels (true), or specified in world coordinates and scales with distance from the camera (false)",
    },
    {
      name: "points",
      type: { type: "nested", schema: Point3 },
      array: true,
      description: "Points along the line",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Solid color to use for the whole line. Ignored if `colors` is non-empty.",
    },
    {
      name: "colors",
      type: { type: "nested", schema: Color },
      array: true,
      description: "Per-point colors (if non-empty, must have the same length as `points`).",
    },
    {
      name: "indices",
      type: { type: "primitive", name: "uint32" },
      array: true,
      description:
        "Indices into the `points` and `colors` attribute arrays, which can be used to avoid duplicating attribute data.\n\nIf omitted or empty, indexing will not be used. This default behavior is equivalent to specifying [0, 1, ..., N-1] for the indices (where N is the number of `points` provided).",
    },
  ],
};

const TextPrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "TextPrimitive",
  description: "A primitive representing a text label",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Position of the center of the text box and orientation of the text. Identity orientation means the text is oriented in the xy-plane and flows from -x to +x.",
    },
    {
      name: "billboard",
      type: { type: "primitive", name: "boolean" },
      description:
        "Whether the text should respect `pose.orientation` (false) or always face the camera (true)",
    },
    {
      name: "font_size",
      type: { type: "primitive", name: "float64" },
      description: "Font size (height of one line of text)",
    },
    {
      name: "scale_invariant",
      type: { type: "primitive", name: "boolean" },
      description:
        "Indicates whether `font_size` is a fixed size in screen pixels (true), or specified in world coordinates and scales with distance from the camera (false)",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color of the text",
    },
    {
      name: "text",
      type: { type: "primitive", name: "string" },
      description: "Text",
    },
  ],
};

const TriangleListPrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "TriangleListPrimitive",
  description: "A primitive representing a set of triangles or a surface tiled by triangles",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Origin of triangles relative to reference frame",
    },
    {
      name: "points",
      type: { type: "nested", schema: Point3 },
      array: true,
      description:
        "Vertices to use for triangles, interpreted as a list of triples (0-1-2, 3-4-5, ...)",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Solid color to use for the whole shape. Ignored if `colors` is non-empty.",
    },
    {
      name: "colors",
      type: { type: "nested", schema: Color },
      array: true,
      description: "Per-vertex colors (if specified, must have the same length as `points`).",
    },
    {
      name: "indices",
      type: { type: "primitive", name: "uint32" },
      array: true,
      description:
        "Indices into the `points` and `colors` attribute arrays, which can be used to avoid duplicating attribute data.\n\nIf omitted or empty, indexing will not be used. This default behavior is equivalent to specifying [0, 1, ..., N-1] for the indices (where N is the number of `points` provided).",
    },
  ],
};

const ModelPrimitive: FoxgloveMessageSchema = {
  type: "message",
  name: "ModelPrimitive",
  description:
    "A primitive representing a 3D model file loaded from an external URL or embedded data",
  fields: [
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Origin of model relative to reference frame",
    },
    {
      name: "scale",
      type: { type: "nested", schema: Vector3 },
      description: "Scale factor to apply to the model along each axis",
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Solid color to use for the whole model if `override_color` is true.",
    },
    {
      name: "override_color",
      type: { type: "primitive", name: "boolean" },
      description:
        "Whether to use the color specified in `color` instead of any materials embedded in the original model.",
    },
    {
      name: "url",
      type: { type: "primitive", name: "string" },
      description: "URL pointing to model file. One of `url` or `data` should be non-empty.",
    },
    {
      name: "media_type",
      type: { type: "primitive", name: "string" },
      description:
        "[Media type](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types) of embedded model (e.g. `model/gltf-binary`). Required if `data` is provided instead of `url`. Overrides the inferred media type if `url` is provided.",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description:
        "Embedded model. One of `url` or `data` should be non-empty. If `data` is non-empty, `media_type` must be set to indicate the type of the data.",
    },
  ],
};

const SceneEntity: FoxgloveMessageSchema = {
  type: "message",
  name: "SceneEntity",
  description:
    "A visual element in a 3D scene. An entity may be composed of multiple primitives which all share the same frame of reference.",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of the entity",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference",
    },
    {
      name: "id",
      type: { type: "primitive", name: "string" },
      description:
        "Identifier for the entity. A entity will replace any prior entity on the same topic with the same `id`.",
    },
    {
      name: "lifetime",
      type: { type: "nested", schema: Duration },
      description:
        "Length of time (relative to `timestamp`) after which the entity should be automatically removed. Zero value indicates the entity should remain visible until it is replaced or deleted.",
    },
    {
      name: "frame_locked",
      type: { type: "primitive", name: "boolean" },
      description:
        "Whether the entity should keep its location in the fixed frame (false) or follow the frame specified in `frame_id` as it moves relative to the fixed frame (true)",
    },
    {
      name: "metadata",
      type: { type: "nested", schema: KeyValuePair },
      array: true,
      description:
        "Additional user-provided metadata associated with the entity. Keys must be unique.",
    },
    {
      name: "arrows",
      type: { type: "nested", schema: ArrowPrimitive },
      array: true,
      description: "Arrow primitives",
    },
    {
      name: "cubes",
      type: { type: "nested", schema: CubePrimitive },
      array: true,
      description: "Cube primitives",
    },
    {
      name: "spheres",
      type: { type: "nested", schema: SpherePrimitive },
      array: true,
      description: "Sphere primitives",
    },
    {
      name: "cylinders",
      type: { type: "nested", schema: CylinderPrimitive },
      array: true,
      description: "Cylinder primitives",
    },
    {
      name: "lines",
      type: { type: "nested", schema: LinePrimitive },
      array: true,
      description: "Line primitives",
    },
    {
      name: "triangles",
      type: { type: "nested", schema: TriangleListPrimitive },
      array: true,
      description: "Triangle list primitives",
    },
    {
      name: "texts",
      type: { type: "nested", schema: TextPrimitive },
      array: true,
      description: "Text primitives",
    },
    {
      name: "models",
      type: { type: "nested", schema: ModelPrimitive },
      array: true,
      description: "Model primitives",
    },
  ],
};

const SceneUpdate: FoxgloveMessageSchema = {
  type: "message",
  name: "SceneUpdate",
  description: "An update to the entities displayed in a 3D scene",
  fields: [
    {
      name: "deletions",
      type: { type: "nested", schema: SceneEntityDeletion },
      array: true,
      description: "Scene entities to delete",
    },
    {
      name: "entities",
      type: { type: "nested", schema: SceneEntity },
      array: true,
      description: "Scene entities to add or replace",
    },
  ],
};

const CameraCalibration: FoxgloveMessageSchema = {
  type: "message",
  name: "CameraCalibration",
  description: "Camera calibration parameters",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of calibration data",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description:
        "Frame of reference for the camera. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.",
      protobufFieldNumber: 9,
    },
    {
      name: "width",
      type: { type: "primitive", name: "uint32" },
      description: "Image width",
    },
    {
      name: "height",
      type: { type: "primitive", name: "uint32" },
      description: "Image height",
    },
    {
      name: "distortion_model",
      type: { type: "primitive", name: "string" },
      description:
        "Name of distortion model\n\nSupported parameters: `plumb_bob` (k1, k2, p1, p2, k3), `rational_polynomial` (k1, k2, p1, p2, k3, k4, k5, k6), and `kannala_brandt` (k1, k2, k3, k4), and `fisheye62` (k0, k1, k2, k3, p0, p1, crit_theta [optional]). `plumb_bob` and `rational_polynomial` models are based on the pinhole model [OpenCV's](https://docs.opencv.org/4.11.0/d9/d0c/group__calib3d.html) [pinhole camera model](https://en.wikipedia.org/wiki/Distortion_%28optics%29#Software_correction). The `kannala_brandt` model matches the [OpenvCV fisheye](https://docs.opencv.org/4.11.0/db/d58/group__calib3d__fisheye.html) model. The `fisheye62` model matches the [Project Aria's Fisheye62 Model](https://facebookresearch.github.io/projectaria_tools/docs/tech_insights/camera_intrinsic_models).",
    },
    {
      name: "D",
      type: { type: "primitive", name: "float64" },
      description: "Distortion parameters",
      array: true,
    },
    {
      name: "K",
      type: { type: "primitive", name: "float64" },
      array: 9,
      description: `Intrinsic camera matrix (3x3 row-major matrix)

A 3x3 row-major matrix for the raw (distorted) image.

Projects 3D points in the camera coordinate frame to 2D pixel coordinates using the focal lengths (fx, fy) and principal point (cx, cy).

\`\`\`
    [fx  0 cx]
K = [ 0 fy cy]
    [ 0  0  1]
\`\`\`

**Uncalibrated cameras:** Following ROS conventions for [CameraInfo](https://docs.ros.org/en/noetic/api/sensor_msgs/html/msg/CameraInfo.html), Foxglove also treats K[0] == 0.0 as indicating an uncalibrated camera, and calibration data will be ignored.
`,
    },
    {
      name: "R",
      type: { type: "primitive", name: "float64" },
      array: 9,
      description: `Rectification matrix (stereo cameras only, 3x3 row-major matrix)

A rotation matrix aligning the camera coordinate system to the ideal stereo image plane so that epipolar lines in both stereo images are parallel.`,
    },
    {
      name: "P",
      type: { type: "primitive", name: "float64" },
      array: 12,
      description: `Projection/camera matrix (3x4 row-major matrix)

\`\`\`
    [fx'  0  cx' Tx]
P = [ 0  fy' cy' Ty]
    [ 0   0   1   0]
\`\`\`

By convention, this matrix specifies the intrinsic (camera) matrix of the processed (rectified) image. That is, the left 3x3 portion is the normal camera intrinsic matrix for the rectified image.

It projects 3D points in the camera coordinate frame to 2D pixel coordinates using the focal lengths (fx', fy') and principal point (cx', cy') - these may differ from the values in K.

For monocular cameras, Tx = Ty = 0. Normally, monocular cameras will also have R = the identity and P[1:3,1:3] = K.

Foxglove currently does not support displaying stereo images, so Tx and Ty are ignored.

Given a 3D point [X Y Z]', the projection (x, y) of the point onto the rectified image is given by:

\`\`\`
[u v w]' = P * [X Y Z 1]'
       x = u / w
       y = v / w
\`\`\`

This holds for both images of a stereo pair.
`,
    },
  ],
};

const CompressedImage: FoxgloveMessageSchema = {
  type: "message",
  name: "CompressedImage",
  description: "A compressed image",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of image",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description:
        "Frame of reference for the image. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.",
      protobufFieldNumber: 4,
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description: "Compressed image data",
    },
    {
      name: "format",
      type: { type: "primitive", name: "string" },
      description: "Image format\n\nSupported values: `jpeg`, `png`, `webp`, `avif`",
    },
  ],
};

const CompressedVideo: FoxgloveMessageSchema = {
  type: "message",
  name: "CompressedVideo",
  description: "A single frame of a compressed video bitstream",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of video frame",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description:
        "Frame of reference for the video.\n\nThe origin of the frame is the optical center of the camera. +x points to the right in the video, +y points down, and +z points into the plane of the video.",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description: `Compressed video frame data.

For packet-based video codecs this data must begin and end on packet boundaries (no partial packets), and must contain enough video packets to decode exactly one image (either a keyframe or delta frame). Note: Foxglove does not support video streams that include B frames because they require lookahead.

Specifically, the requirements for different \`format\` values are:

- \`h264\`
  - Use Annex B formatted data
  - Each CompressedVideo message should contain enough NAL units to decode exactly one video frame
  - Each message containing a key frame (IDR) must also include a SPS NAL unit

- \`h265\` (HEVC)
  - Use Annex B formatted data
  - Each CompressedVideo message should contain enough NAL units to decode exactly one video frame
  - Each message containing a key frame (IRAP) must also include relevant VPS/SPS/PPS NAL units

- \`vp9\`
  - Each CompressedVideo message should contain exactly one video frame

- \`av1\`
  - Use the "Low overhead bitstream format" (section 5.2)
  - Each CompressedVideo message should contain enough OBUs to decode exactly one video frame
  - Each message containing a key frame must also include a Sequence Header OBU`,
    },
    {
      name: "format",
      type: { type: "primitive", name: "string" },
      description:
        "Video format.\n\nSupported values: `h264`, `h265`, `vp9`, `av1`.\n\nNote: compressed video support is subject to hardware limitations and patent licensing, so not all encodings may be supported on all platforms. See more about [H.265 support](https://caniuse.com/hevc), [VP9 support](https://caniuse.com/webm), and [AV1 support](https://caniuse.com/av1).",
    },
  ],
};

const RawImage: FoxgloveMessageSchema = {
  type: "message",
  name: "RawImage",
  description: "A raw image",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of image",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description:
        "Frame of reference for the image. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.",
      protobufFieldNumber: 7,
    },
    {
      name: "width",
      type: { type: "primitive", name: "uint32" },
      description: "Image width in pixels",
    },
    {
      name: "height",
      type: { type: "primitive", name: "uint32" },
      description: "Image height in pixels",
    },
    {
      name: "encoding",
      type: { type: "primitive", name: "string" },
      description:
        "Encoding of the raw image data. See the `data` field description for supported values.",
    },
    {
      name: "step",
      type: { type: "primitive", name: "uint32" },
      description:
        "Byte length of a single row. This is usually some multiple of `width` depending on the encoding, but can be greater to incorporate padding.",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description: `Raw image data.

For each \`encoding\` value, the \`data\` field contains image pixel data serialized as follows:

- \`yuv422\` or \`uyvy\`:
  - Pixel colors are decomposed into [Y'UV](https://en.wikipedia.org/wiki/Y%E2%80%B2UV) channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - U and V values are shared between horizontal pairs of pixels. Each pair of output pixels is serialized as [U, Y1, V, Y2].
  - \`step\` must be greater than or equal to \`width\` * 2.
- \`yuv422_yuy2\` or  \`yuyv\`:
  - Pixel colors are decomposed into [Y'UV](https://en.wikipedia.org/wiki/Y%E2%80%B2UV) channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - U and V values are shared between horizontal pairs of pixels. Each pair of output pixels is encoded as [Y1, U, Y2, V].
  - \`step\` must be greater than or equal to \`width\` * 2.
- \`rgb8\`:
  - Pixel colors are decomposed into Red, Green, and Blue channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [R, G, B].
  - \`step\` must be greater than or equal to \`width\` * 3.
- \`rgba8\`:
  - Pixel colors are decomposed into Red, Green, Blue, and Alpha channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [R, G, B, Alpha].
  - \`step\` must be greater than or equal to \`width\` * 4.
- \`bgr8\` or \`8UC3\`:
  - Pixel colors are decomposed into Blue, Green, and Red channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [B, G, R].
  - \`step\` must be greater than or equal to \`width\` * 3.
- \`bgra8\`:
  - Pixel colors are decomposed into Blue, Green, Red, and Alpha channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is encoded as [B, G, R, Alpha].
  - \`step\` must be greater than or equal to \`width\` * 4.
- \`32FC1\`:
  - Pixel brightness is represented as a single-channel, 32-bit little-endian IEEE 754 floating-point value, ranging from 0.0 (black) to 1.0 (white).
  - \`step\` must be greater than or equal to \`width\` * 4.
- \`bayer_rggb8\`, \`bayer_bggr8\`, \`bayer_gbrg8\`, or \`bayer_grbg8\`:
  - Pixel colors are decomposed into Red, Blue and Green channels.
  - Pixel channel values are represented as unsigned 8-bit integers, and serialized in a 2x2 bayer filter pattern.
  - The order of the four letters after \`bayer_\` determine the layout, so for \`bayer_wxyz8\` the pattern is:
  \`\`\`plaintext
  w | x
  - + -
  y | z
  \`\`\`
  - \`step\` must be greater than or equal to \`width\`.
- \`mono8\` or \`8UC1\`:
  - Pixel brightness is represented as unsigned 8-bit integers.
  - \`step\` must be greater than or equal to \`width\`.
- \`mono16\` or \`16UC1\`:
  - Pixel brightness is represented as 16-bit unsigned little-endian integers. Rendering of these values is controlled in [Image panel color mode settings](https://docs.foxglove.dev/docs/visualization/panels/image#general).
  - \`step\` must be greater than or equal to \`width\` * 2.
`,
    },
  ],
};

const FrameTransform: FoxgloveMessageSchema = {
  type: "message",
  name: "FrameTransform",
  description:
    "A transform between two reference frames in 3D space. The transform defines the position and orientation of a child frame within a parent frame. Translation moves the origin of the child frame relative to the parent origin. The rotation changes the orientiation of the child frame around its origin.\n\nExamples:\n\n- With translation (x=1, y=0, z=0) and identity rotation (x=0, y=0, z=0, w=1), a point at (x=0, y=0, z=0) in the child frame maps to (x=1, y=0, z=0) in the parent frame.\n\n- With translation (x=1, y=2, z=0) and a 90-degree rotation around the z-axis (x=0, y=0, z=0.707, w=0.707), a point at (x=1, y=0, z=0) in the child frame maps to (x=-1, y=3, z=0) in the parent frame.",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of transform",
    },
    {
      name: "parent_frame_id",
      type: { type: "primitive", name: "string" },
      description: "Name of the parent frame",
    },
    {
      name: "child_frame_id",
      type: { type: "primitive", name: "string" },
      description: "Name of the child frame",
    },
    {
      name: "translation",
      type: { type: "nested", schema: Vector3 },
      description:
        "Translation component of the transform, representing the position of the child frame's origin in the parent frame.",
    },
    {
      name: "rotation",
      type: { type: "nested", schema: Quaternion },
      description:
        "Rotation component of the transform, representing the orientation of the child frame in the parent frame",
    },
  ],
};

const FrameTransforms: FoxgloveMessageSchema = {
  type: "message",
  name: "FrameTransforms",
  description: "An array of FrameTransform messages",
  fields: [
    {
      name: "transforms",
      type: { type: "nested", schema: FrameTransform },
      array: true,
      description: "Array of transforms",
    },
  ],
};

const PoseInFrame: FoxgloveMessageSchema = {
  type: "message",
  name: "PoseInFrame",
  description: "A timestamped pose for an object or reference frame in 3D space",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of pose",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference for pose position and orientation",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "Pose in 3D space",
    },
  ],
};

const PosesInFrame: FoxgloveMessageSchema = {
  type: "message",
  name: "PosesInFrame",
  description: "An array of timestamped poses for an object or reference frame in 3D space",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of pose",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference for pose position and orientation",
    },
    {
      name: "poses",
      type: { type: "nested", schema: Pose },
      description: "Poses in 3D space",
      array: true,
    },
  ],
};

const GeoJSON: FoxgloveMessageSchema = {
  type: "message",
  name: "GeoJSON",
  description: "GeoJSON data for annotating maps",
  fields: [
    {
      name: "geojson",
      type: { type: "primitive", name: "string" },
      description: "GeoJSON data encoded as a UTF-8 string",
    },
  ],
};

const NumericType: FoxgloveEnumSchema = {
  type: "enum",
  name: "NumericType",
  description: "Numeric type",
  parentSchemaName: "PackedElementField",
  protobufEnumName: "NumericType",
  values: [
    { name: "UNKNOWN", value: 0, description: "Unknown numeric type" },
    { name: "UINT8", value: 1, description: "Unsigned 8-bit integer" },
    { name: "INT8", value: 2, description: "Signed 8-bit integer" },
    { name: "UINT16", value: 3, description: "Unsigned 16-bit integer" },
    { name: "INT16", value: 4, description: "Signed 16-bit integer" },
    { name: "UINT32", value: 5, description: "Unsigned 32-bit integer" },
    { name: "INT32", value: 6, description: "Signed 32-bit integer" },
    { name: "FLOAT32", value: 7, description: "32-bit floating-point number" },
    { name: "FLOAT64", value: 8, description: "64-bit floating-point number" },
  ],
};

const PackedElementField: FoxgloveMessageSchema = {
  type: "message",
  name: "PackedElementField",
  description: "A field present within each element in a byte array of packed elements.",
  fields: [
    {
      name: "name",
      type: { type: "primitive", name: "string" },
      description: "Name of the field",
    },
    {
      name: "offset",
      type: { type: "primitive", name: "uint32" },
      description: "Byte offset from start of data buffer",
    },
    {
      name: "type",
      type: { type: "enum", enum: NumericType },
      description: "Type of data in the field. Integers are stored using little-endian byte order.",
    },
  ],
};

const Grid: FoxgloveMessageSchema = {
  type: "message",
  name: "Grid",
  description: "A 2D grid of data",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of grid",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Origin of grid's corner relative to frame of reference; grid is positioned in the x-y plane relative to this origin",
    },
    {
      name: "column_count",
      type: { type: "primitive", name: "uint32" },
      description: "Number of grid columns",
    },
    {
      name: "cell_size",
      type: { type: "nested", schema: Vector2 },
      description: "Size of single grid cell along x and y axes, relative to `pose`",
    },
    {
      name: "row_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between rows in `data`",
    },
    {
      name: "cell_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between cells within a row in `data`",
    },
    {
      name: "fields",
      type: { type: "nested", schema: PackedElementField },
      array: true,
      description:
        'Fields in `data`. `red`, `green`, `blue`, and `alpha` are optional for customizing the grid\'s color.\nTo enable RGB color visualization in the [3D panel](https://docs.foxglove.dev/docs/visualization/panels/3d#rgba-separate-fields-color-mode), include **all four** of these fields in your `fields` array:\n\n- `red` - Red channel value\n- `green` - Green channel value\n- `blue` - Blue channel value\n- `alpha` - Alpha/transparency channel value\n\n**note:** All four fields must be present with these exact names for RGB visualization to work. The order of fields doesn\'t matter, but the names must match exactly.\n\nRecommended type: `UINT8` (0-255 range) for standard 8-bit color channels.\n\nExample field definitions:\n\n**RGB color only:**\n\n```javascript\nfields: [\n { name: "red", offset: 0, type: NumericType.UINT8 },\n { name: "green", offset: 1, type: NumericType.UINT8 },\n { name: "blue", offset: 2, type: NumericType.UINT8 },\n { name: "alpha", offset: 3, type: NumericType.UINT8 },\n];\n```\n\n**RGB color with elevation (for 3D terrain visualization):**\n\n```javascript\nfields: [\n { name: "red", offset: 0, type: NumericType.UINT8 },\n { name: "green", offset: 1, type: NumericType.UINT8 },\n { name: "blue", offset: 2, type: NumericType.UINT8 },\n { name: "alpha", offset: 3, type: NumericType.UINT8 },\n { name: "elevation", offset: 4, type: NumericType.FLOAT32 },\n];\n```\n\nWhen these fields are present, the 3D panel will offer additional "Color Mode" options including "RGBA (separate fields)" to visualize the RGB data directly. For elevation visualization, set the "Elevation field" to your elevation layer name.',
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description:
        "Grid cell data, interpreted using `fields`, in row-major (y-major) order.\nFor the data element starting at byte offset i, the coordinates of its corner closest to the origin will be:\n\n- y = i / row_stride * cell_size.y\n- x = (i % row_stride) / cell_stride * cell_size.x",
    },
  ],
};

const VoxelGrid: FoxgloveMessageSchema = {
  type: "message",
  name: "VoxelGrid",
  description: "A 3D grid of data",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of grid",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Origin of the grid’s lower-front-left corner in the reference frame. The grid’s pose is defined relative to this corner, so an untransformed grid with an identity orientation has this corner at the origin.",
    },
    {
      name: "row_count",
      type: { type: "primitive", name: "uint32" },
      description: "Number of grid rows",
    },
    {
      name: "column_count",
      type: { type: "primitive", name: "uint32" },
      description: "Number of grid columns",
    },
    {
      name: "cell_size",
      type: { type: "nested", schema: Vector3 },
      description: "Size of single grid cell along x, y, and z axes, relative to `pose`",
    },
    {
      name: "slice_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between depth slices in `data`",
    },
    {
      name: "row_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between rows in `data`",
    },
    {
      name: "cell_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between cells within a row in `data`",
    },
    {
      name: "fields",
      type: { type: "nested", schema: PackedElementField },
      array: true,
      description:
        "Fields in `data`. `red`, `green`, `blue`, and `alpha` are optional for customizing the grid's color.",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description:
        "Grid cell data, interpreted using `fields`, in depth-major, row-major (Z-Y-X) order.\nFor the data element starting at byte offset i, the coordinates of its corner closest to the origin will be:\n\n- z = i / slice_stride * cell_size.z\n- y = (i % slice_stride) / row_stride * cell_size.y\n- x = (i % row_stride) / cell_stride * cell_size.x",
    },
  ],
};

const CircleAnnotation: FoxgloveMessageSchema = {
  type: "message",
  name: "CircleAnnotation",
  description: "A circle annotation on a 2D image",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of circle",
    },
    {
      name: "position",
      type: { type: "nested", schema: Point2 },
      description:
        "Center of the circle in 2D image coordinates (pixels).\nThe coordinate uses the top-left corner of the top-left pixel of the image as the origin.",
    },
    {
      name: "diameter",
      type: { type: "primitive", name: "float64" },
      description: "Circle diameter in pixels",
    },
    {
      name: "thickness",
      type: { type: "primitive", name: "float64" },
      description: "Line thickness in pixels",
    },
    {
      name: "fill_color",
      type: { type: "nested", schema: Color },
      description: "Fill color",
    },
    {
      name: "outline_color",
      type: { type: "nested", schema: Color },
      description: "Outline color",
    },
  ],
};

const PointsAnnotationType: FoxgloveEnumSchema = {
  type: "enum",
  name: "PointsAnnotationType",
  description: "Type of points annotation",
  parentSchemaName: "PointsAnnotation",
  protobufEnumName: "Type",
  values: [
    { name: "UNKNOWN", value: 0, description: "Unknown points annotation type" },
    { name: "POINTS", value: 1, description: "Individual points: 0, 1, 2, ..." },
    { name: "LINE_LOOP", value: 2, description: "Closed polygon: 0-1, 1-2, ..., (n-1)-n, n-0" },
    {
      name: "LINE_STRIP",
      value: 3,
      description: "Connected line segments: 0-1, 1-2, ..., (n-1)-n",
    },
    { name: "LINE_LIST", value: 4, description: "Individual line segments: 0-1, 2-3, 4-5, ..." },
  ],
};

const PointsAnnotation: FoxgloveMessageSchema = {
  type: "message",
  name: "PointsAnnotation",
  description: "An array of points on a 2D image",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of annotation",
    },
    {
      name: "type",
      type: { type: "enum", enum: PointsAnnotationType },
      description: "Type of points annotation to draw",
    },
    {
      name: "points",
      type: { type: "nested", schema: Point2 },
      description:
        "Points in 2D image coordinates (pixels).\nThese coordinates use the top-left corner of the top-left pixel of the image as the origin.",
      array: true,
    },
    {
      name: "outline_color",
      type: { type: "nested", schema: Color },
      description: "Outline color",
    },
    {
      name: "outline_colors",
      type: { type: "nested", schema: Color },
      description:
        "Per-point colors, if `type` is `POINTS`, or per-segment stroke colors, if `type` is `LINE_LIST`, `LINE_STRIP` or `LINE_LOOP`.",
      array: true,
    },
    {
      name: "fill_color",
      type: { type: "nested", schema: Color },
      description: "Fill color",
    },
    {
      name: "thickness",
      type: { type: "primitive", name: "float64" },
      description: "Stroke thickness in pixels",
    },
  ],
};

const TextAnnotation: FoxgloveMessageSchema = {
  type: "message",
  name: "TextAnnotation",
  description: "A text label on a 2D image",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of annotation",
    },
    {
      name: "position",
      type: { type: "nested", schema: Point2 },
      description:
        "Bottom-left origin of the text label in 2D image coordinates (pixels).\nThe coordinate uses the top-left corner of the top-left pixel of the image as the origin.",
    },
    {
      name: "text",
      type: { type: "primitive", name: "string" },
      description: "Text to display",
    },
    {
      name: "font_size",
      type: { type: "primitive", name: "float64" },
      description: "Font size in pixels",
      defaultValue: 12.0,
    },
    {
      name: "text_color",
      type: { type: "nested", schema: Color },
      description: "Text color",
    },
    {
      name: "background_color",
      type: { type: "nested", schema: Color },
      description: "Background fill color",
    },
  ],
};

const ImageAnnotations: FoxgloveMessageSchema = {
  type: "message",
  name: "ImageAnnotations",
  description: "Array of annotations for a 2D image",
  fields: [
    {
      name: "circles",
      type: { type: "nested", schema: CircleAnnotation },
      description: "Circle annotations",
      array: true,
    },
    {
      name: "points",
      type: { type: "nested", schema: PointsAnnotation },
      description: "Points annotations",
      array: true,
    },
    {
      name: "texts",
      type: { type: "nested", schema: TextAnnotation },
      description: "Text annotations",
      array: true,
    },
    {
      name: "metadata",
      type: { type: "nested", schema: KeyValuePair },
      description:
        "Additional user-provided metadata associated with the image annotations. Keys must be unique.",
      array: true,
      optional: true,
    },
  ],
};

const PositionCovarianceType: FoxgloveEnumSchema = {
  type: "enum",
  name: "PositionCovarianceType",
  description: "Type of position covariance",
  parentSchemaName: "LocationFix",
  protobufEnumName: "PositionCovarianceType",
  values: [
    { name: "UNKNOWN", value: 0, description: "Unknown position covariance type" },
    { name: "APPROXIMATED", value: 1, description: "Position covariance is approximated" },
    {
      name: "DIAGONAL_KNOWN",
      value: 2,
      description: "Position covariance is per-axis, so put it along the diagonal",
    },
    { name: "KNOWN", value: 3, description: "Position covariance of the fix is known" },
  ],
};

const LocationFix: FoxgloveMessageSchema = {
  type: "message",
  name: "LocationFix",
  description: "A navigation satellite fix for any Global Navigation Satellite System",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of the message",
      protobufFieldNumber: 6,
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description:
        "Frame for the sensor. Latitude and longitude readings are at the origin of the frame.",
      protobufFieldNumber: 7,
    },
    {
      name: "latitude",
      type: { type: "primitive", name: "float64" },
      description: "Latitude in degrees",
      protobufFieldNumber: 1,
    },
    {
      name: "longitude",
      type: { type: "primitive", name: "float64" },
      description: "Longitude in degrees",
      protobufFieldNumber: 2,
    },
    {
      name: "altitude",
      type: { type: "primitive", name: "float64" },
      description: "Altitude in meters",
      protobufFieldNumber: 3,
    },
    {
      name: "position_covariance",
      type: { type: "primitive", name: "float64" },
      description:
        "Position covariance (m^2) defined relative to a tangential plane through the reported position. The components are East, North, and Up (ENU), in row-major order.",
      array: 9,
      protobufFieldNumber: 4,
    },
    {
      name: "position_covariance_type",
      type: { type: "enum", enum: PositionCovarianceType },
      description:
        "If `position_covariance` is available, `position_covariance_type` must be set to indicate the type of covariance.",
      protobufFieldNumber: 5,
    },
    {
      name: "color",
      type: { type: "nested", schema: Color },
      description: "Color used to visualize the location",
      protobufFieldNumber: 8,
      optional: true,
    },
    {
      name: "metadata",
      type: { type: "nested", schema: KeyValuePair },
      array: true,
      description:
        "Additional user-provided metadata associated with the location fix. Keys must be unique.",
      protobufFieldNumber: 9,
      optional: true,
    },
  ],
};

const LocationFixes: FoxgloveMessageSchema = {
  type: "message",
  name: "LocationFixes",
  description: "A group of LocationFix messages",
  fields: [
    {
      name: "fixes",
      type: { type: "nested", schema: LocationFix },
      array: true,
      description: "An array of location fixes",
    },
  ],
};

const LogLevel: FoxgloveEnumSchema = {
  type: "enum",
  name: "LogLevel",
  description: "Log level",
  parentSchemaName: "Log",
  protobufEnumName: "Level",
  values: [
    { name: "UNKNOWN", value: 0, description: "Unknown log level" },
    { name: "DEBUG", value: 1, description: "Debug log level" },
    { name: "INFO", value: 2, description: "Info log level" },
    { name: "WARNING", value: 3, description: "Warning log level" },
    { name: "ERROR", value: 4, description: "Error log level" },
    { name: "FATAL", value: 5, description: "Fatal log level" },
  ],
};

const Log: FoxgloveMessageSchema = {
  type: "message",
  name: "Log",
  description: "A log message",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of log message",
    },
    {
      name: "level",
      type: { type: "enum", enum: LogLevel },
      description: "Log level",
    },
    {
      name: "message",
      type: { type: "primitive", name: "string" },
      description: "Log message",
    },
    {
      name: "name",
      type: { type: "primitive", name: "string" },
      description: "Process or node name",
    },
    {
      name: "file",
      type: { type: "primitive", name: "string" },
      description: "Filename",
    },
    {
      name: "line",
      type: { type: "primitive", name: "uint32" },
      description: "Line number in the file",
    },
  ],
};

const PointCloud: FoxgloveMessageSchema = {
  type: "message",
  name: "PointCloud",
  description:
    "A collection of N-dimensional points, which may contain additional fields with information like normals, intensity, etc.",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of point cloud",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description: "The origin of the point cloud relative to the frame of reference",
    },
    {
      name: "point_stride",
      type: { type: "primitive", name: "uint32" },
      description: "Number of bytes between points in the `data`",
    },
    {
      name: "fields",
      type: { type: "nested", schema: PackedElementField },
      array: true,
      description:
        "Fields in `data`. At least 2 coordinate fields from `x`, `y`, and `z` are required for each point's position; `red`, `green`, `blue`, and `alpha` are optional for customizing each point's color.",
    },
    {
      name: "data",
      type: { type: "primitive", name: "bytes" },
      description: "Point data, interpreted using `fields`",
    },
  ],
};

const LaserScan: FoxgloveMessageSchema = {
  type: "message",
  name: "LaserScan",
  description: "A single scan from a planar laser range-finder",
  fields: [
    {
      name: "timestamp",
      type: { type: "nested", schema: Timestamp },
      description: "Timestamp of scan",
    },
    {
      name: "frame_id",
      type: { type: "primitive", name: "string" },
      description: "Frame of reference",
    },
    {
      name: "pose",
      type: { type: "nested", schema: Pose },
      description:
        "Origin of scan relative to frame of reference; points are positioned in the x-y plane relative to this origin; angles are interpreted as counterclockwise rotations around the z axis with 0 rad being in the +x direction",
    },
    {
      name: "start_angle",
      type: { type: "primitive", name: "float64" },
      description: "Bearing of first point, in radians",
    },
    {
      name: "end_angle",
      type: { type: "primitive", name: "float64" },
      description: "Bearing of last point, in radians",
    },
    {
      name: "ranges",
      type: { type: "primitive", name: "float64" },
      description:
        "Distance of detections from origin; assumed to be at equally-spaced angles between `start_angle` and `end_angle`",
      array: true,
    },
    {
      name: "intensities",
      type: { type: "primitive", name: "float64" },
      description: "Intensity of detections",
      array: true,
    },
  ],
};

export const foxgloveMessageSchemas = {
  ArrowPrimitive,
  CameraCalibration,
  CircleAnnotation,
  Color,
  CompressedImage,
  CompressedVideo,
  CylinderPrimitive,
  CubePrimitive,
  Duration,
  FrameTransform,
  FrameTransforms,
  GeoJSON,
  Grid,
  VoxelGrid,
  ImageAnnotations,
  KeyValuePair,
  LaserScan,
  LinePrimitive,
  LocationFix,
  LocationFixes,
  Log,
  SceneEntityDeletion,
  SceneEntity,
  SceneUpdate,
  ModelPrimitive,
  PackedElementField,
  Point2,
  Point3,
  Point3InFrame,
  PointCloud,
  PointsAnnotation,
  Pose,
  PoseInFrame,
  PosesInFrame,
  Quaternion,
  RawAudio,
  RawImage,
  SpherePrimitive,
  TextAnnotation,
  TextPrimitive,
  Timestamp,
  TriangleListPrimitive,
  Vector2,
  Vector3,
};

export const foxgloveEnumSchemas = {
  LineType,
  LogLevel,
  SceneEntityDeletionType,
  NumericType,
  PointsAnnotationType,
  PositionCovarianceType,
};
