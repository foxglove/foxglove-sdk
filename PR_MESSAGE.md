### Changelog

Add optional `message_id` and `lineage` fields to all top-level Foxglove message schemas for tracking data provenance through processing pipelines.

### Docs

None - schema documentation is auto-generated and included in-line with schema definitions.

### Description

**Problem**: Engineers debugging robotics systems struggle to trace data flow through processing pipelines. When an output is incorrect (e.g., a fused perception result), determining which input message(s) caused the issue requires manually correlating timestamps across topics and mentally reconstructing the data flow graph.

**Solution**: This PR introduces lineage tracking fields to Foxglove schemas, enabling messages to reference their inputs and be traced back through the processing pipeline.

**Changes**:

1. **New schema types** for lineage tracking:
   - `InputReference`: Identifies a parent message by topic name and message_id
   - `StateReference`: Identifies node state at processing time (topic + message_id)
   - `LineageInfo`: Contains an array of input references, processing_node name, and optional state reference

2. **Added `message_id` and `lineage` fields to 25 message types**:

   *Sensor data types:*
   - `RawImage`, `CompressedImage`, `CompressedVideo`, `PointCloud`, `LaserScan`, `RawAudio`, `CameraCalibration`, `LocationFix`

   *Derived/processed data types:*
   - `Grid`, `VoxelGrid`, `FrameTransform`, `PoseInFrame`, `PosesInFrame`, `Point3InFrame`

   *Annotation types:*
   - `CircleAnnotation`, `PointsAnnotation`, `TextAnnotation`

   *Scene/visualization types:*
   - `SceneEntity`, `SceneEntityDeletion`, `SceneUpdate`

   *Container/aggregation types:*
   - `FrameTransforms`, `ImageAnnotations`, `LocationFixes`, `GeoJSON`

   *Logging:*
   - `Log`

3. **Intentionally excluded** (primitives that are always embedded, never published alone):
   - Math primitives: `Color`, `Vector2`, `Vector3`, `Point2`, `Point3`, `Quaternion`, `Pose`
   - Time primitives: `Duration`, `Timestamp`
   - Other primitives: `KeyValuePair`, `PackedElementField`
   - 3D shape primitives: `ArrowPrimitive`, `CubePrimitive`, `SpherePrimitive`, etc.

4. **Schema formats regenerated**: Protobuf, JSON Schema, ROS1 messages, ROS2 messages, FlatBuffer, TypeScript types

**Design decisions**:
- `message_id` is format-agnostic (UUID7 recommended, but ULID or custom formats supported)
- Fields use protobuf field numbers 100/101 to avoid conflicts with existing fields
- Both fields are optional for backward compatibility

**Usage example**:
```python
from foxglove import PointCloud, LineageInfo, InputReference

fused_cloud = PointCloud(
    timestamp=now(),
    data=fused_points,
    message_id="019453f2-abcd-7000-8000-000000000001",  # UUID7
    lineage=LineageInfo(
        inputs=[
            InputReference(topic="/lidar_front/points", message_id="019453f1-..."),
            InputReference(topic="/lidar_rear/points", message_id="019453f0-...")
        ],
        processing_node="point_cloud_fusion"
    )
)
```

**Manual testing TODO for reviewers**:
- [ ] Verify TypeScript types compile: `cd typescript/schemas && yarn build`
- [ ] Verify protobuf definitions parse correctly
- [ ] Verify ROS message definitions are valid (CI should cover)
