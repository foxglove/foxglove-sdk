^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
Changelog for package foxglove_msgs
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

3.1.0 (2025-02-11)
------------------
* Remove invalid generated schemas from package

3.0.1 (2024-12-10)
------------------
* Updating distortion model description
* Update ImageAnnotation schemas' coordinate fields with comment specifying the coordinate space
* Update RawImage schema with endianness
* Update CompressedVideo format strings

3.0.0 (2023-10-31)
------------------
* Add a frame_id field to LocationFix
* Update RawImage encodings
* Add CompressedVideo schema
* Improve doc on PointAnnotation field outline_color

2.2.0 (2023-04-18)
------------------
* Add TextAnnotation
* add FrameTransforms Message
* Remove "all float64s" comment from PointCloud schema
* Add units to PointsAnnotation and CircleAnnotation
* Update descriptions for PointsAnnotationType and LineType
* Elaborate on required fields for PointCloud and Grid messages

2.1.1 (2022-11-18)
------------------
* Fix typo in CubePrimitive comments

2.1.0 (2022-11-18)
------------------
* Add frame_id to RawImage, CompressedImage, and CameraCalibration
* Add new SceneEntity and Primitive types
* Updated documentation for several types and fields

2.0.0 (2022-07-15)
------------------
* Consolidate Transform and FrameTransform
* Contributors: Adrian Macneil

1.2.2 (2022-06-13)
------------------
* Fix CMake build accessing parent directory
* Contributors: Jacob Bandes-Storch

1.2.1 (2022-06-03)
------------------
* Fix CMake install share directory
* Contributors: Jacob Bandes-Storch

1.2.0 (2022-05-17)
------------------
* Add new Foxglove message types to foxglove_msgs
* Contributors: Jacob Bandes-Storch

1.1.0 (2022-04-19)
------------------
* Provide foxglove_msgs for ROS1 and ROS2
* Contributors: Daisuke Nishimatsu, John Hurliman, wep21

1.0.0 (2021-05-26)
-------------------
* Initial release with foxglove_msgs/ImageMarkerArray message
