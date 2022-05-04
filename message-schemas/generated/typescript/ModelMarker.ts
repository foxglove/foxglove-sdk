// Generated from ModelMarker by @foxglove/message-schemas

import { Color } from "./Color";
import { Duration } from "./Duration";
import { KeyValuePair } from "./KeyValuePair";
import { Pose } from "./Pose";
import { Time } from "./Time";
import { Vector3 } from "./Vector3";

/** A marker representing a 3D model */
export type ModelMarker = {
  /** Timestamp of the marker */
  timestamp: Time;

  /** Frame of reference */
  frame_id: string;

  /** Namespace into which the marker should be grouped. A marker will replace any prior marker on the same topic with the same `namespace` and `id`. */
  namespace: string;

  /** Identifier for the marker. A marker will replace any prior marker on the same topic with the same `namespace` and `id`. */
  id: string;

  /** Length of time (relative to `timestamp`) after which the marker should be automatically removed. Zero value indicates the marker should remain visible until it is replaced or deleted. */
  lifetime: Duration;

  /** Whether the marker should keep its location in the fixed frame (false) or follow the frame specified in `frame_id` as it moves relative to the fixed frame (true) */
  frame_locked: boolean;

  /** Additional user-provided metadata associated with the marker. Keys must be unique. */
  metadata: KeyValuePair[];

  /** Origin of model relative to reference frame */
  pose: Pose;

  /** Scale factor to apply to the model along each axis */
  scale: Vector3;

  /** Solid color to use for the whole model. If `use_embedded_materials` is true, this color is blended on top of the embedded material color. */
  color: Color;

  /** Whether to use materials embedded in the model, or only the `color` */
  use_embedded_materials: boolean;

  /** URL pointing to model file. Either `url` or `mime_type` and `data` should be provided. */
  url: string;

  /** MIME type of embedded model (e.g. `model/gltf-binary`). Either `url` or `mime_type` and `data` should be provided. */
  mime_type: string;

  /** Embedded model. Either `url` or `mime_type` and `data` should be provided. */
  data: Uint8Array;
};
