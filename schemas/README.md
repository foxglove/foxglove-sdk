# Foxglove schemas

See [Foxglove Schemas documentation](https://docs.foxglove.dev/docs/visualization/message-schemas/introduction).

All schemas are generated from [schemas.ts](/internal/schemas.ts).

## Optional fields

Optional message fields may be omitted. When omitted, the consumer (e.g. a Foxglove panel) determines the behavior.

If the IDL does not support optional fields (e.g. ROS) you must specify a value for the field.

## Contents

- [enum LineType](#enum-linetype)
- [enum LogLevel](#enum-loglevel)
- [enum NumericType](#enum-numerictype)
- [enum PointsAnnotationType](#enum-pointsannotationtype)
- [enum PositionCovarianceType](#enum-positioncovariancetype)
- [enum SceneEntityDeletionType](#enum-sceneentitydeletiontype)
- [ArrowPrimitive](#arrowprimitive)
- [CameraCalibration](#cameracalibration)
- [CircleAnnotation](#circleannotation)
- [Color](#color)
- [CompressedImage](#compressedimage)
- [CompressedVideo](#compressedvideo)
- [CubePrimitive](#cubeprimitive)
- [CylinderPrimitive](#cylinderprimitive)
- [Duration](#duration)
- [FrameTransform](#frametransform)
- [FrameTransforms](#frametransforms)
- [GeoJSON](#geojson)
- [Grid](#grid)
- [ImageAnnotations](#imageannotations)
- [KeyValuePair](#keyvaluepair)
- [LaserScan](#laserscan)
- [LinePrimitive](#lineprimitive)
- [LocationFix](#locationfix)
- [LocationFixes](#locationfixes)
- [Log](#log)
- [ModelPrimitive](#modelprimitive)
- [PackedElementField](#packedelementfield)
- [Point2](#point2)
- [Point3](#point3)
- [Point3InFrame](#point3inframe)
- [PointCloud](#pointcloud)
- [PointsAnnotation](#pointsannotation)
- [Pose](#pose)
- [PoseInFrame](#poseinframe)
- [PosesInFrame](#posesinframe)
- [Quaternion](#quaternion)
- [RawAudio](#rawaudio)
- [RawImage](#rawimage)
- [SceneEntity](#sceneentity)
- [SceneEntityDeletion](#sceneentitydeletion)
- [SceneUpdate](#sceneupdate)
- [SpherePrimitive](#sphereprimitive)
- [TextAnnotation](#textannotation)
- [TextPrimitive](#textprimitive)
- [Timestamp](#timestamp)
- [TriangleListPrimitive](#trianglelistprimitive)
- [Vector2](#vector2)
- [Vector3](#vector3)
- [VoxelGrid](#voxelgrid)

----

## enum LineType

An enumeration indicating how input points should be interpreted to create lines

name | value | description
---- | ----- | -----------
`LINE_STRIP` | 0 | Connected line segments: 0-1, 1-2, ..., (n-1)-n
`LINE_LOOP` | 1 | Closed polygon: 0-1, 1-2, ..., (n-1)-n, n-0
`LINE_LIST` | 2 | Individual line segments: 0-1, 2-3, 4-5, ...



## enum LogLevel

Log level

name | value | description
---- | ----- | -----------
`UNKNOWN` | 0 | Unknown log level
`DEBUG` | 1 | Debug log level
`INFO` | 2 | Info log level
`WARNING` | 3 | Warning log level
`ERROR` | 4 | Error log level
`FATAL` | 5 | Fatal log level



## enum NumericType

Numeric type

name | value | description
---- | ----- | -----------
`UNKNOWN` | 0 | Unknown numeric type
`UINT8` | 1 | Unsigned 8-bit integer
`INT8` | 2 | Signed 8-bit integer
`UINT16` | 3 | Unsigned 16-bit integer
`INT16` | 4 | Signed 16-bit integer
`UINT32` | 5 | Unsigned 32-bit integer
`INT32` | 6 | Signed 32-bit integer
`FLOAT32` | 7 | 32-bit floating-point number
`FLOAT64` | 8 | 64-bit floating-point number



## enum PointsAnnotationType

Type of points annotation

name | value | description
---- | ----- | -----------
`UNKNOWN` | 0 | Unknown points annotation type
`POINTS` | 1 | Individual points: 0, 1, 2, ...
`LINE_LOOP` | 2 | Closed polygon: 0-1, 1-2, ..., (n-1)-n, n-0
`LINE_STRIP` | 3 | Connected line segments: 0-1, 1-2, ..., (n-1)-n
`LINE_LIST` | 4 | Individual line segments: 0-1, 2-3, 4-5, ...



## enum PositionCovarianceType

Type of position covariance

name | value | description
---- | ----- | -----------
`UNKNOWN` | 0 | Unknown position covariance type
`APPROXIMATED` | 1 | Position covariance is approximated
`DIAGONAL_KNOWN` | 2 | Position covariance is per-axis, so put it along the diagonal
`KNOWN` | 3 | Position covariance of the fix is known



## enum SceneEntityDeletionType

An enumeration indicating which entities should match a SceneEntityDeletion command

name | value | description
---- | ----- | -----------
`MATCHING_ID` | 0 | Delete the existing entity on the same topic that has the provided `id`
`ALL` | 1 | Delete all existing entities on the same topic



## ArrowPrimitive

A primitive representing an arrow

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Position of the arrow's tail and orientation of the arrow. Identity orientation means the arrow points in the +x direction.

</td>
</tr>
<tr>
<td><code>shaft_length</code></td>
<td>

float64

</td>
<td>

Length of the arrow shaft

</td>
</tr>
<tr>
<td><code>shaft_diameter</code></td>
<td>

float64

</td>
<td>

Diameter of the arrow shaft

</td>
</tr>
<tr>
<td><code>head_length</code></td>
<td>

float64

</td>
<td>

Length of the arrow head

</td>
</tr>
<tr>
<td><code>head_diameter</code></td>
<td>

float64

</td>
<td>

Diameter of the arrow head

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Color of the arrow

</td>
</tr>
</table>

## CameraCalibration

Camera calibration parameters

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of calibration data

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for the camera. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.

</td>
</tr>
<tr>
<td><code>width</code></td>
<td>

uint32

</td>
<td>

Image width

</td>
</tr>
<tr>
<td><code>height</code></td>
<td>

uint32

</td>
<td>

Image height

</td>
</tr>
<tr>
<td><code>distortion_model</code></td>
<td>

string

</td>
<td>

Name of distortion model

Supported parameters: `plumb_bob` (k1, k2, p1, p2, k3), `rational_polynomial` (k1, k2, p1, p2, k3, k4, k5, k6), and `kannala_brandt` (k1, k2, k3, k4), and `fisheye62` (k0, k1, k2, k3, p0, p1, crit_theta [optional]). `plumb_bob` and `rational_polynomial` models are based on the pinhole model [OpenCV's](https://docs.opencv.org/4.11.0/d9/d0c/group__calib3d.html) [pinhole camera model](https://en.wikipedia.org/wiki/Distortion_%28optics%29#Software_correction). The `kannala_brandt` model matches the [OpenvCV fisheye](https://docs.opencv.org/4.11.0/db/d58/group__calib3d__fisheye.html) model. The `fisheye62` model matches the [Project Aria's Fisheye62 Model](https://facebookresearch.github.io/projectaria_tools/docs/tech_insights/camera_intrinsic_models).

</td>
</tr>
<tr>
<td><code>D</code></td>
<td>

float64[]

</td>
<td>

Distortion parameters

</td>
</tr>
<tr>
<td><code>K</code></td>
<td>

float64[9]

</td>
<td>

Intrinsic camera matrix (3x3 row-major matrix)

A 3x3 row-major matrix for the raw (distorted) image.

Projects 3D points in the camera coordinate frame to 2D pixel coordinates using the focal lengths (fx, fy) and principal point (cx, cy).

```
    [fx  0 cx]
K = [ 0 fy cy]
    [ 0  0  1]
```

**Uncalibrated cameras:** Following ROS conventions for [CameraInfo](https://docs.ros.org/en/noetic/api/sensor_msgs/html/msg/CameraInfo.html), Foxglove also treats K[0] == 0.0 as indicating an uncalibrated camera, and calibration data will be ignored.


</td>
</tr>
<tr>
<td><code>R</code></td>
<td>

float64[9]

</td>
<td>

Rectification matrix (stereo cameras only, 3x3 row-major matrix)

A rotation matrix aligning the camera coordinate system to the ideal stereo image plane so that epipolar lines in both stereo images are parallel.

</td>
</tr>
<tr>
<td><code>P</code></td>
<td>

float64[12]

</td>
<td>

Projection/camera matrix (3x4 row-major matrix)

```
    [fx'  0  cx' Tx]
P = [ 0  fy' cy' Ty]
    [ 0   0   1   0]
```

By convention, this matrix specifies the intrinsic (camera) matrix of the processed (rectified) image. That is, the left 3x3 portion is the normal camera intrinsic matrix for the rectified image.

It projects 3D points in the camera coordinate frame to 2D pixel coordinates using the focal lengths (fx', fy') and principal point (cx', cy') - these may differ from the values in K.

For monocular cameras, Tx = Ty = 0. Normally, monocular cameras will also have R = the identity and P[1:3,1:3] = K.

Foxglove currently does not support displaying stereo images, so Tx and Ty are ignored.

Given a 3D point [X Y Z]', the projection (x, y) of the point onto the rectified image is given by:

```
[u v w]' = P * [X Y Z 1]'
       x = u / w
       y = v / w
```

This holds for both images of a stereo pair.


</td>
</tr>
</table>

## CircleAnnotation

A circle annotation on a 2D image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of circle

</td>
</tr>
<tr>
<td><code>position</code></td>
<td>

[Point2](#point2)

</td>
<td>

Center of the circle in 2D image coordinates (pixels).
The coordinate uses the top-left corner of the top-left pixel of the image as the origin.

</td>
</tr>
<tr>
<td><code>diameter</code></td>
<td>

float64

</td>
<td>

Circle diameter in pixels

</td>
</tr>
<tr>
<td><code>thickness</code></td>
<td>

float64

</td>
<td>

Line thickness in pixels

</td>
</tr>
<tr>
<td><code>fill_color</code></td>
<td>

[Color](#color)

</td>
<td>

Fill color

</td>
</tr>
<tr>
<td><code>outline_color</code></td>
<td>

[Color](#color)

</td>
<td>

Outline color

</td>
</tr>
</table>

## Color

A color in RGBA format

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>r</code></td>
<td>

float64

</td>
<td>

Red value between 0 and 1

</td>
</tr>
<tr>
<td><code>g</code></td>
<td>

float64

</td>
<td>

Green value between 0 and 1

</td>
</tr>
<tr>
<td><code>b</code></td>
<td>

float64

</td>
<td>

Blue value between 0 and 1

</td>
</tr>
<tr>
<td><code>a</code></td>
<td>

float64

</td>
<td>

Alpha value between 0 and 1

</td>
</tr>
</table>

## CompressedImage

A compressed image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of image

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for the image. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Compressed image data

</td>
</tr>
<tr>
<td><code>format</code></td>
<td>

string

</td>
<td>

Image format

Supported values: `jpeg`, `png`, `webp`, `avif`

</td>
</tr>
</table>

## CompressedVideo

A single frame of a compressed video bitstream

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of video frame

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for the video.

The origin of the frame is the optical center of the camera. +x points to the right in the video, +y points down, and +z points into the plane of the video.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Compressed video frame data.

For packet-based video codecs this data must begin and end on packet boundaries (no partial packets), and must contain enough video packets to decode exactly one image (either a keyframe or delta frame). Note: Foxglove does not support video streams that include B frames because they require lookahead.

Specifically, the requirements for different `format` values are:

- `h264`
  - Use Annex B formatted data
  - Each CompressedVideo message should contain enough NAL units to decode exactly one video frame
  - Each message containing a key frame (IDR) must also include a SPS NAL unit

- `h265` (HEVC)
  - Use Annex B formatted data
  - Each CompressedVideo message should contain enough NAL units to decode exactly one video frame
  - Each message containing a key frame (IRAP) must also include relevant VPS/SPS/PPS NAL units

- `vp9`
  - Each CompressedVideo message should contain exactly one video frame

- `av1`
  - Use the "Low overhead bitstream format" (section 5.2)
  - Each CompressedVideo message should contain enough OBUs to decode exactly one video frame
  - Each message containing a key frame must also include a Sequence Header OBU

</td>
</tr>
<tr>
<td><code>format</code></td>
<td>

string

</td>
<td>

Video format.

Supported values: `h264`, `h265`, `vp9`, `av1`.

Note: compressed video support is subject to hardware limitations and patent licensing, so not all encodings may be supported on all platforms. See more about [H.265 support](https://caniuse.com/hevc), [VP9 support](https://caniuse.com/webm), and [AV1 support](https://caniuse.com/av1).

</td>
</tr>
</table>

## CubePrimitive

A primitive representing a cube or rectangular prism

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Position of the center of the cube and orientation of the cube

</td>
</tr>
<tr>
<td><code>size</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Size of the cube along each axis

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Color of the cube

</td>
</tr>
</table>

## CylinderPrimitive

A primitive representing a cylinder, elliptic cylinder, or truncated cone

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Position of the center of the cylinder and orientation of the cylinder. The flat face(s) are perpendicular to the z-axis.

</td>
</tr>
<tr>
<td><code>size</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Size of the cylinder's bounding box

</td>
</tr>
<tr>
<td><code>bottom_scale</code></td>
<td>

float64

</td>
<td>

0-1, ratio of the diameter of the cylinder's bottom face (min z) to the bottom of the bounding box

</td>
</tr>
<tr>
<td><code>top_scale</code></td>
<td>

float64

</td>
<td>

0-1, ratio of the diameter of the cylinder's top face (max z) to the top of the bounding box

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Color of the cylinder

</td>
</tr>
</table>

## Duration

A duration of time, composed of seconds and nanoseconds

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>sec</code></td>
<td>

int32

</td>
<td>

The number of seconds in the duration

</td>
</tr>
<tr>
<td><code>nsec</code></td>
<td>

uint32

</td>
<td>

The number of nanoseconds in the positive direction

</td>
</tr>
</table>

## FrameTransform

A transform between two reference frames in 3D space. The transform defines the position and orientation of a child frame within a parent frame. Translation moves the origin of the child frame relative to the parent origin. The rotation changes the orientiation of the child frame around its origin.

Examples:

- With translation (x=1, y=0, z=0) and identity rotation (x=0, y=0, z=0, w=1), a point at (x=0, y=0, z=0) in the child frame maps to (x=1, y=0, z=0) in the parent frame.

- With translation (x=1, y=2, z=0) and a 90-degree rotation around the z-axis (x=0, y=0, z=0.707, w=0.707), a point at (x=1, y=0, z=0) in the child frame maps to (x=-1, y=3, z=0) in the parent frame.

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of transform

</td>
</tr>
<tr>
<td><code>parent_frame_id</code></td>
<td>

string

</td>
<td>

Name of the parent frame

</td>
</tr>
<tr>
<td><code>child_frame_id</code></td>
<td>

string

</td>
<td>

Name of the child frame

</td>
</tr>
<tr>
<td><code>translation</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Translation component of the transform, representing the position of the child frame's origin in the parent frame.

</td>
</tr>
<tr>
<td><code>rotation</code></td>
<td>

[Quaternion](#quaternion)

</td>
<td>

Rotation component of the transform, representing the orientation of the child frame in the parent frame

</td>
</tr>
</table>

## FrameTransforms

An array of FrameTransform messages

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>transforms</code></td>
<td>

[FrameTransform](#frametransform)[]

</td>
<td>

Array of transforms

</td>
</tr>
</table>

## GeoJSON

GeoJSON data for annotating maps

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>geojson</code></td>
<td>

string

</td>
<td>

GeoJSON data encoded as a UTF-8 string

</td>
</tr>
</table>

## Grid

A 2D grid of data

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of grid

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of grid's corner relative to frame of reference; grid is positioned in the x-y plane relative to this origin

</td>
</tr>
<tr>
<td><code>column_count</code></td>
<td>

uint32

</td>
<td>

Number of grid columns

</td>
</tr>
<tr>
<td><code>cell_size</code></td>
<td>

[Vector2](#vector2)

</td>
<td>

Size of single grid cell along x and y axes, relative to `pose`

</td>
</tr>
<tr>
<td><code>row_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between rows in `data`

</td>
</tr>
<tr>
<td><code>cell_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between cells within a row in `data`

</td>
</tr>
<tr>
<td><code>fields</code></td>
<td>

[PackedElementField](#packedelementfield)[]

</td>
<td>

Fields in `data`. `red`, `green`, `blue`, and `alpha` are optional for customizing the grid's color.
To enable RGB color visualization in the [3D panel](https://docs.foxglove.dev/docs/visualization/panels/3d#rgba-separate-fields-color-mode), include **all four** of these fields in your `fields` array:

- `red` - Red channel value
- `green` - Green channel value
- `blue` - Blue channel value
- `alpha` - Alpha/transparency channel value

**note:** All four fields must be present with these exact names for RGB visualization to work. The order of fields doesn't matter, but the names must match exactly.

Recommended type: `UINT8` (0-255 range) for standard 8-bit color channels.

Example field definitions:

**RGB color only:**

```javascript
fields: [
 { name: "red", offset: 0, type: NumericType.UINT8 },
 { name: "green", offset: 1, type: NumericType.UINT8 },
 { name: "blue", offset: 2, type: NumericType.UINT8 },
 { name: "alpha", offset: 3, type: NumericType.UINT8 },
];
```

**RGB color with elevation (for 3D terrain visualization):**

```javascript
fields: [
 { name: "red", offset: 0, type: NumericType.UINT8 },
 { name: "green", offset: 1, type: NumericType.UINT8 },
 { name: "blue", offset: 2, type: NumericType.UINT8 },
 { name: "alpha", offset: 3, type: NumericType.UINT8 },
 { name: "elevation", offset: 4, type: NumericType.FLOAT32 },
];
```

When these fields are present, the 3D panel will offer additional "Color Mode" options including "RGBA (separate fields)" to visualize the RGB data directly. For elevation visualization, set the "Elevation field" to your elevation layer name.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Grid cell data, interpreted using `fields`, in row-major (y-major) order.
For the data element starting at byte offset i, the coordinates of its corner closest to the origin will be:

- y = i / row_stride * cell_size.y
- x = (i % row_stride) / cell_stride * cell_size.x

</td>
</tr>
</table>

## ImageAnnotations

Array of annotations for a 2D image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>circles</code></td>
<td>

[CircleAnnotation](#circleannotation)[]

</td>
<td>

Circle annotations

</td>
</tr>
<tr>
<td><code>points</code></td>
<td>

[PointsAnnotation](#pointsannotation)[]

</td>
<td>

Points annotations

</td>
</tr>
<tr>
<td><code>texts</code></td>
<td>

[TextAnnotation](#textannotation)[]

</td>
<td>

Text annotations

</td>
</tr>
<tr>
<td><code>metadata</code> (optional)</td>
<td>

[KeyValuePair](#keyvaluepair)[]

</td>
<td>

Additional user-provided metadata associated with the image annotations. Keys must be unique.

</td>
</tr>
</table>

## KeyValuePair

A key with its associated value

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>key</code></td>
<td>

string

</td>
<td>

Key

</td>
</tr>
<tr>
<td><code>value</code></td>
<td>

string

</td>
<td>

Value

</td>
</tr>
</table>

## LaserScan

A single scan from a planar laser range-finder

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of scan

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of scan relative to frame of reference; points are positioned in the x-y plane relative to this origin; angles are interpreted as counterclockwise rotations around the z axis with 0 rad being in the +x direction

</td>
</tr>
<tr>
<td><code>start_angle</code></td>
<td>

float64

</td>
<td>

Bearing of first point, in radians

</td>
</tr>
<tr>
<td><code>end_angle</code></td>
<td>

float64

</td>
<td>

Bearing of last point, in radians

</td>
</tr>
<tr>
<td><code>ranges</code></td>
<td>

float64[]

</td>
<td>

Distance of detections from origin; assumed to be at equally-spaced angles between `start_angle` and `end_angle`

</td>
</tr>
<tr>
<td><code>intensities</code></td>
<td>

float64[]

</td>
<td>

Intensity of detections

</td>
</tr>
</table>

## LinePrimitive

A primitive representing a series of points connected by lines

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>type</code></td>
<td>

[enum LineType](#enum-linetype)

</td>
<td>

Drawing primitive to use for lines

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of lines relative to reference frame

</td>
</tr>
<tr>
<td><code>thickness</code></td>
<td>

float64

</td>
<td>

Line thickness

</td>
</tr>
<tr>
<td><code>scale_invariant</code></td>
<td>

boolean

</td>
<td>

Indicates whether `thickness` is a fixed size in screen pixels (true), or specified in world coordinates and scales with distance from the camera (false)

</td>
</tr>
<tr>
<td><code>points</code></td>
<td>

[Point3](#point3)[]

</td>
<td>

Points along the line

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Solid color to use for the whole line. Ignored if `colors` is non-empty.

</td>
</tr>
<tr>
<td><code>colors</code></td>
<td>

[Color](#color)[]

</td>
<td>

Per-point colors (if non-empty, must have the same length as `points`).

</td>
</tr>
<tr>
<td><code>indices</code></td>
<td>

uint32[]

</td>
<td>

Indices into the `points` and `colors` attribute arrays, which can be used to avoid duplicating attribute data.

If omitted or empty, indexing will not be used. This default behavior is equivalent to specifying [0, 1, ..., N-1] for the indices (where N is the number of `points` provided).

</td>
</tr>
</table>

## LocationFix

A navigation satellite fix for any Global Navigation Satellite System

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of the message

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame for the sensor. Latitude and longitude readings are at the origin of the frame.

</td>
</tr>
<tr>
<td><code>latitude</code></td>
<td>

float64

</td>
<td>

Latitude in degrees

</td>
</tr>
<tr>
<td><code>longitude</code></td>
<td>

float64

</td>
<td>

Longitude in degrees

</td>
</tr>
<tr>
<td><code>altitude</code></td>
<td>

float64

</td>
<td>

Altitude in meters

</td>
</tr>
<tr>
<td><code>position_covariance</code></td>
<td>

float64[9]

</td>
<td>

Position covariance (m^2) defined relative to a tangential plane through the reported position. The components are East, North, and Up (ENU), in row-major order.

</td>
</tr>
<tr>
<td><code>position_covariance_type</code></td>
<td>

[enum PositionCovarianceType](#enum-positioncovariancetype)

</td>
<td>

If `position_covariance` is available, `position_covariance_type` must be set to indicate the type of covariance.

</td>
</tr>
<tr>
<td><code>color</code> (optional)</td>
<td>

[Color](#color)

</td>
<td>

Color used to visualize the location

</td>
</tr>
<tr>
<td><code>metadata</code> (optional)</td>
<td>

[KeyValuePair](#keyvaluepair)[]

</td>
<td>

Additional user-provided metadata associated with the location fix. Keys must be unique.

</td>
</tr>
</table>

## LocationFixes

A group of LocationFix messages

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>fixes</code></td>
<td>

[LocationFix](#locationfix)[]

</td>
<td>

An array of location fixes

</td>
</tr>
</table>

## Log

A log message

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of log message

</td>
</tr>
<tr>
<td><code>level</code></td>
<td>

[enum LogLevel](#enum-loglevel)

</td>
<td>

Log level

</td>
</tr>
<tr>
<td><code>message</code></td>
<td>

string

</td>
<td>

Log message

</td>
</tr>
<tr>
<td><code>name</code></td>
<td>

string

</td>
<td>

Process or node name

</td>
</tr>
<tr>
<td><code>file</code></td>
<td>

string

</td>
<td>

Filename

</td>
</tr>
<tr>
<td><code>line</code></td>
<td>

uint32

</td>
<td>

Line number in the file

</td>
</tr>
</table>

## ModelPrimitive

A primitive representing a 3D model file loaded from an external URL or embedded data

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of model relative to reference frame

</td>
</tr>
<tr>
<td><code>scale</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Scale factor to apply to the model along each axis

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Solid color to use for the whole model if `override_color` is true.

</td>
</tr>
<tr>
<td><code>override_color</code></td>
<td>

boolean

</td>
<td>

Whether to use the color specified in `color` instead of any materials embedded in the original model.

</td>
</tr>
<tr>
<td><code>url</code></td>
<td>

string

</td>
<td>

URL pointing to model file. One of `url` or `data` should be non-empty.

</td>
</tr>
<tr>
<td><code>media_type</code></td>
<td>

string

</td>
<td>

[Media type](https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types) of embedded model (e.g. `model/gltf-binary`). Required if `data` is provided instead of `url`. Overrides the inferred media type if `url` is provided.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Embedded model. One of `url` or `data` should be non-empty. If `data` is non-empty, `media_type` must be set to indicate the type of the data.

</td>
</tr>
</table>

## PackedElementField

A field present within each element in a byte array of packed elements.

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>name</code></td>
<td>

string

</td>
<td>

Name of the field

</td>
</tr>
<tr>
<td><code>offset</code></td>
<td>

uint32

</td>
<td>

Byte offset from start of data buffer

</td>
</tr>
<tr>
<td><code>type</code></td>
<td>

[enum NumericType](#enum-numerictype)

</td>
<td>

Type of data in the field. Integers are stored using little-endian byte order.

</td>
</tr>
</table>

## Point2

A point representing a position in 2D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>x</code></td>
<td>

float64

</td>
<td>

x coordinate position

</td>
</tr>
<tr>
<td><code>y</code></td>
<td>

float64

</td>
<td>

y coordinate position

</td>
</tr>
</table>

## Point3

A point representing a position in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>x</code></td>
<td>

float64

</td>
<td>

x coordinate position

</td>
</tr>
<tr>
<td><code>y</code></td>
<td>

float64

</td>
<td>

y coordinate position

</td>
</tr>
<tr>
<td><code>z</code></td>
<td>

float64

</td>
<td>

z coordinate position

</td>
</tr>
</table>

## Point3InFrame

A timestamped point for a position in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of point

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for point position

</td>
</tr>
<tr>
<td><code>point</code></td>
<td>

[Point3](#point3)

</td>
<td>

Point in 3D space

</td>
</tr>
</table>

## PointCloud

A collection of N-dimensional points, which may contain additional fields with information like normals, intensity, etc.

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of point cloud

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

The origin of the point cloud relative to the frame of reference

</td>
</tr>
<tr>
<td><code>point_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between points in the `data`

</td>
</tr>
<tr>
<td><code>fields</code></td>
<td>

[PackedElementField](#packedelementfield)[]

</td>
<td>

Fields in `data`. At least 2 coordinate fields from `x`, `y`, and `z` are required for each point's position; `red`, `green`, `blue`, and `alpha` are optional for customizing each point's color.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Point data, interpreted using `fields`

</td>
</tr>
</table>

## PointsAnnotation

An array of points on a 2D image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of annotation

</td>
</tr>
<tr>
<td><code>type</code></td>
<td>

[enum PointsAnnotationType](#enum-pointsannotationtype)

</td>
<td>

Type of points annotation to draw

</td>
</tr>
<tr>
<td><code>points</code></td>
<td>

[Point2](#point2)[]

</td>
<td>

Points in 2D image coordinates (pixels).
These coordinates use the top-left corner of the top-left pixel of the image as the origin.

</td>
</tr>
<tr>
<td><code>outline_color</code></td>
<td>

[Color](#color)

</td>
<td>

Outline color

</td>
</tr>
<tr>
<td><code>outline_colors</code></td>
<td>

[Color](#color)[]

</td>
<td>

Per-point colors, if `type` is `POINTS`, or per-segment stroke colors, if `type` is `LINE_LIST`, `LINE_STRIP` or `LINE_LOOP`.

</td>
</tr>
<tr>
<td><code>fill_color</code></td>
<td>

[Color](#color)

</td>
<td>

Fill color

</td>
</tr>
<tr>
<td><code>thickness</code></td>
<td>

float64

</td>
<td>

Stroke thickness in pixels

</td>
</tr>
</table>

## Pose

A position and orientation for an object or reference frame in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>position</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Point denoting position in 3D space

</td>
</tr>
<tr>
<td><code>orientation</code></td>
<td>

[Quaternion](#quaternion)

</td>
<td>

Quaternion denoting orientation in 3D space

</td>
</tr>
</table>

## PoseInFrame

A timestamped pose for an object or reference frame in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of pose

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for pose position and orientation

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Pose in 3D space

</td>
</tr>
</table>

## PosesInFrame

An array of timestamped poses for an object or reference frame in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of pose

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for pose position and orientation

</td>
</tr>
<tr>
<td><code>poses</code></td>
<td>

[Pose](#pose)[]

</td>
<td>

Poses in 3D space

</td>
</tr>
</table>

## Quaternion

A [quaternion](https://eater.net/quaternions) representing a rotation in 3D space

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>x</code></td>
<td>

float64

</td>
<td>

x value

</td>
</tr>
<tr>
<td><code>y</code></td>
<td>

float64

</td>
<td>

y value

</td>
</tr>
<tr>
<td><code>z</code></td>
<td>

float64

</td>
<td>

z value

</td>
</tr>
<tr>
<td><code>w</code></td>
<td>

float64

</td>
<td>

w value

</td>
</tr>
</table>

## RawAudio

A single block of an audio bitstream

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of the start of the audio block

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Audio data. The samples in the data must be interleaved and little-endian

</td>
</tr>
<tr>
<td><code>format</code></td>
<td>

string

</td>
<td>

Audio format. Only 'pcm-s16' is currently supported

</td>
</tr>
<tr>
<td><code>sample_rate</code></td>
<td>

uint32

</td>
<td>

Sample rate in Hz

</td>
</tr>
<tr>
<td><code>number_of_channels</code></td>
<td>

uint32

</td>
<td>

Number of channels in the audio block

</td>
</tr>
</table>

## RawImage

A raw image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of image

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference for the image. The origin of the frame is the optical center of the camera. +x points to the right in the image, +y points down, and +z points into the plane of the image.

</td>
</tr>
<tr>
<td><code>width</code></td>
<td>

uint32

</td>
<td>

Image width in pixels

</td>
</tr>
<tr>
<td><code>height</code></td>
<td>

uint32

</td>
<td>

Image height in pixels

</td>
</tr>
<tr>
<td><code>encoding</code></td>
<td>

string

</td>
<td>

Encoding of the raw image data. See the `data` field description for supported values.

</td>
</tr>
<tr>
<td><code>step</code></td>
<td>

uint32

</td>
<td>

Byte length of a single row. This is usually some multiple of `width` depending on the encoding, but can be greater to incorporate padding.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Raw image data.

For each `encoding` value, the `data` field contains image pixel data serialized as follows:

- `yuv422` or `uyvy`:
  - Pixel colors are decomposed into [Y'UV](https://en.wikipedia.org/wiki/Y%E2%80%B2UV) channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - U and V values are shared between horizontal pairs of pixels. Each pair of output pixels is serialized as [U, Y1, V, Y2].
  - `step` must be greater than or equal to `width` * 2.
- `yuv422_yuy2` or  `yuyv`:
  - Pixel colors are decomposed into [Y'UV](https://en.wikipedia.org/wiki/Y%E2%80%B2UV) channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - U and V values are shared between horizontal pairs of pixels. Each pair of output pixels is encoded as [Y1, U, Y2, V].
  - `step` must be greater than or equal to `width` * 2.
- `rgb8`:
  - Pixel colors are decomposed into Red, Green, and Blue channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [R, G, B].
  - `step` must be greater than or equal to `width` * 3.
- `rgba8`:
  - Pixel colors are decomposed into Red, Green, Blue, and Alpha channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [R, G, B, Alpha].
  - `step` must be greater than or equal to `width` * 4.
- `bgr8` or `8UC3`:
  - Pixel colors are decomposed into Blue, Green, and Red channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is serialized as [B, G, R].
  - `step` must be greater than or equal to `width` * 3.
- `bgra8`:
  - Pixel colors are decomposed into Blue, Green, Red, and Alpha channels.
  - Pixel channel values are represented as unsigned 8-bit integers.
  - Each output pixel is encoded as [B, G, R, Alpha].
  - `step` must be greater than or equal to `width` * 4.
- `32FC1`:
  - Pixel brightness is represented as a single-channel, 32-bit little-endian IEEE 754 floating-point value, ranging from 0.0 (black) to 1.0 (white).
  - `step` must be greater than or equal to `width` * 4.
- `bayer_rggb8`, `bayer_bggr8`, `bayer_gbrg8`, or `bayer_grbg8`:
  - Pixel colors are decomposed into Red, Blue and Green channels.
  - Pixel channel values are represented as unsigned 8-bit integers, and serialized in a 2x2 bayer filter pattern.
  - The order of the four letters after `bayer_` determine the layout, so for `bayer_wxyz8` the pattern is:
  ```plaintext
  w | x
  - + -
  y | z
  ```
  - `step` must be greater than or equal to `width`.
- `mono8` or `8UC1`:
  - Pixel brightness is represented as unsigned 8-bit integers.
  - `step` must be greater than or equal to `width`.
- `mono16` or `16UC1`:
  - Pixel brightness is represented as 16-bit unsigned little-endian integers. Rendering of these values is controlled in [Image panel color mode settings](https://docs.foxglove.dev/docs/visualization/panels/image#general).
  - `step` must be greater than or equal to `width` * 2.


</td>
</tr>
</table>

## SceneEntity

A visual element in a 3D scene. An entity may be composed of multiple primitives which all share the same frame of reference.

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of the entity

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference

</td>
</tr>
<tr>
<td><code>id</code></td>
<td>

string

</td>
<td>

Identifier for the entity. A entity will replace any prior entity on the same topic with the same `id`.

</td>
</tr>
<tr>
<td><code>lifetime</code></td>
<td>

[Duration](#duration)

</td>
<td>

Length of time (relative to `timestamp`) after which the entity should be automatically removed. Zero value indicates the entity should remain visible until it is replaced or deleted.

</td>
</tr>
<tr>
<td><code>frame_locked</code></td>
<td>

boolean

</td>
<td>

Whether the entity should keep its location in the fixed frame (false) or follow the frame specified in `frame_id` as it moves relative to the fixed frame (true)

</td>
</tr>
<tr>
<td><code>metadata</code></td>
<td>

[KeyValuePair](#keyvaluepair)[]

</td>
<td>

Additional user-provided metadata associated with the entity. Keys must be unique.

</td>
</tr>
<tr>
<td><code>arrows</code></td>
<td>

[ArrowPrimitive](#arrowprimitive)[]

</td>
<td>

Arrow primitives

</td>
</tr>
<tr>
<td><code>cubes</code></td>
<td>

[CubePrimitive](#cubeprimitive)[]

</td>
<td>

Cube primitives

</td>
</tr>
<tr>
<td><code>spheres</code></td>
<td>

[SpherePrimitive](#sphereprimitive)[]

</td>
<td>

Sphere primitives

</td>
</tr>
<tr>
<td><code>cylinders</code></td>
<td>

[CylinderPrimitive](#cylinderprimitive)[]

</td>
<td>

Cylinder primitives

</td>
</tr>
<tr>
<td><code>lines</code></td>
<td>

[LinePrimitive](#lineprimitive)[]

</td>
<td>

Line primitives

</td>
</tr>
<tr>
<td><code>triangles</code></td>
<td>

[TriangleListPrimitive](#trianglelistprimitive)[]

</td>
<td>

Triangle list primitives

</td>
</tr>
<tr>
<td><code>texts</code></td>
<td>

[TextPrimitive](#textprimitive)[]

</td>
<td>

Text primitives

</td>
</tr>
<tr>
<td><code>models</code></td>
<td>

[ModelPrimitive](#modelprimitive)[]

</td>
<td>

Model primitives

</td>
</tr>
</table>

## SceneEntityDeletion

Command to remove previously published entities

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of the deletion. Only matching entities earlier than this timestamp will be deleted.

</td>
</tr>
<tr>
<td><code>type</code></td>
<td>

[enum SceneEntityDeletionType](#enum-sceneentitydeletiontype)

</td>
<td>

Type of deletion action to perform

</td>
</tr>
<tr>
<td><code>id</code></td>
<td>

string

</td>
<td>

Identifier which must match if `type` is `MATCHING_ID`.

</td>
</tr>
</table>

## SceneUpdate

An update to the entities displayed in a 3D scene

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>deletions</code></td>
<td>

[SceneEntityDeletion](#sceneentitydeletion)[]

</td>
<td>

Scene entities to delete

</td>
</tr>
<tr>
<td><code>entities</code></td>
<td>

[SceneEntity](#sceneentity)[]

</td>
<td>

Scene entities to add or replace

</td>
</tr>
</table>

## SpherePrimitive

A primitive representing a sphere or ellipsoid

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Position of the center of the sphere and orientation of the sphere

</td>
</tr>
<tr>
<td><code>size</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Size (diameter) of the sphere along each axis

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Color of the sphere

</td>
</tr>
</table>

## TextAnnotation

A text label on a 2D image

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of annotation

</td>
</tr>
<tr>
<td><code>position</code></td>
<td>

[Point2](#point2)

</td>
<td>

Bottom-left origin of the text label in 2D image coordinates (pixels).
The coordinate uses the top-left corner of the top-left pixel of the image as the origin.

</td>
</tr>
<tr>
<td><code>text</code></td>
<td>

string

</td>
<td>

Text to display

</td>
</tr>
<tr>
<td><code>font_size</code></td>
<td>

float64

</td>
<td>

Font size in pixels

</td>
</tr>
<tr>
<td><code>text_color</code></td>
<td>

[Color](#color)

</td>
<td>

Text color

</td>
</tr>
<tr>
<td><code>background_color</code></td>
<td>

[Color](#color)

</td>
<td>

Background fill color

</td>
</tr>
</table>

## TextPrimitive

A primitive representing a text label

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Position of the center of the text box and orientation of the text. Identity orientation means the text is oriented in the xy-plane and flows from -x to +x.

</td>
</tr>
<tr>
<td><code>billboard</code></td>
<td>

boolean

</td>
<td>

Whether the text should respect `pose.orientation` (false) or always face the camera (true)

</td>
</tr>
<tr>
<td><code>font_size</code></td>
<td>

float64

</td>
<td>

Font size (height of one line of text)

</td>
</tr>
<tr>
<td><code>scale_invariant</code></td>
<td>

boolean

</td>
<td>

Indicates whether `font_size` is a fixed size in screen pixels (true), or specified in world coordinates and scales with distance from the camera (false)

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Color of the text

</td>
</tr>
<tr>
<td><code>text</code></td>
<td>

string

</td>
<td>

Text

</td>
</tr>
</table>

## Timestamp

A timestamp composed of seconds and nanoseconds

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>sec</code></td>
<td>

uint32

</td>
<td>

The number of seconds since a user-defined epoch

</td>
</tr>
<tr>
<td><code>nsec</code></td>
<td>

uint32

</td>
<td>

The number of nanoseconds since the sec value

</td>
</tr>
</table>

## TriangleListPrimitive

A primitive representing a set of triangles or a surface tiled by triangles

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of triangles relative to reference frame

</td>
</tr>
<tr>
<td><code>points</code></td>
<td>

[Point3](#point3)[]

</td>
<td>

Vertices to use for triangles, interpreted as a list of triples (0-1-2, 3-4-5, ...)

</td>
</tr>
<tr>
<td><code>color</code></td>
<td>

[Color](#color)

</td>
<td>

Solid color to use for the whole shape. Ignored if `colors` is non-empty.

</td>
</tr>
<tr>
<td><code>colors</code></td>
<td>

[Color](#color)[]

</td>
<td>

Per-vertex colors (if specified, must have the same length as `points`).

</td>
</tr>
<tr>
<td><code>indices</code></td>
<td>

uint32[]

</td>
<td>

Indices into the `points` and `colors` attribute arrays, which can be used to avoid duplicating attribute data.

If omitted or empty, indexing will not be used. This default behavior is equivalent to specifying [0, 1, ..., N-1] for the indices (where N is the number of `points` provided).

</td>
</tr>
</table>

## Vector2

A vector in 2D space that represents a direction only

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>x</code></td>
<td>

float64

</td>
<td>

x coordinate length

</td>
</tr>
<tr>
<td><code>y</code></td>
<td>

float64

</td>
<td>

y coordinate length

</td>
</tr>
</table>

## Vector3

A vector in 3D space that represents a direction only

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>x</code></td>
<td>

float64

</td>
<td>

x coordinate length

</td>
</tr>
<tr>
<td><code>y</code></td>
<td>

float64

</td>
<td>

y coordinate length

</td>
</tr>
<tr>
<td><code>z</code></td>
<td>

float64

</td>
<td>

z coordinate length

</td>
</tr>
</table>

## VoxelGrid

A 3D grid of data

<table>
  <tr>
    <th>field</th>
    <th>type</th>
    <th>description</th>
  </tr>
<tr>
<td><code>timestamp</code></td>
<td>

[Timestamp](#timestamp)

</td>
<td>

Timestamp of grid

</td>
</tr>
<tr>
<td><code>frame_id</code></td>
<td>

string

</td>
<td>

Frame of reference

</td>
</tr>
<tr>
<td><code>pose</code></td>
<td>

[Pose](#pose)

</td>
<td>

Origin of the grid’s lower-front-left corner in the reference frame. The grid’s pose is defined relative to this corner, so an untransformed grid with an identity orientation has this corner at the origin.

</td>
</tr>
<tr>
<td><code>row_count</code></td>
<td>

uint32

</td>
<td>

Number of grid rows

</td>
</tr>
<tr>
<td><code>column_count</code></td>
<td>

uint32

</td>
<td>

Number of grid columns

</td>
</tr>
<tr>
<td><code>cell_size</code></td>
<td>

[Vector3](#vector3)

</td>
<td>

Size of single grid cell along x, y, and z axes, relative to `pose`

</td>
</tr>
<tr>
<td><code>slice_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between depth slices in `data`

</td>
</tr>
<tr>
<td><code>row_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between rows in `data`

</td>
</tr>
<tr>
<td><code>cell_stride</code></td>
<td>

uint32

</td>
<td>

Number of bytes between cells within a row in `data`

</td>
</tr>
<tr>
<td><code>fields</code></td>
<td>

[PackedElementField](#packedelementfield)[]

</td>
<td>

Fields in `data`. `red`, `green`, `blue`, and `alpha` are optional for customizing the grid's color.

</td>
</tr>
<tr>
<td><code>data</code></td>
<td>

bytes

</td>
<td>

Grid cell data, interpreted using `fields`, in depth-major, row-major (Z-Y-X) order.
For the data element starting at byte offset i, the coordinates of its corner closest to the origin will be:

- z = i / slice_stride * cell_size.z
- y = (i % slice_stride) / row_stride * cell_size.y
- x = (i % row_stride) / cell_stride * cell_size.x

</td>
</tr>
</table>