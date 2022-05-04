// Generated from ArrowMarker by @foxglove/message-schemas

import { Color } from "./Color";
import { Duration } from "./Duration";
import { KeyValuePair } from "./KeyValuePair";
import { Pose } from "./Pose";
import { Time } from "./Time";

/** A marker representing an arrow */
export type ArrowMarker = {
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

  /** Position of the arrow's tail and orientation of the arrow. Identity orientation means the arrow points in the +x direction. */
  pose: Pose;

  /** Length of the arrow */
  length: number;

  /** Diameter of the arrow shaft */
  shaft_diameter: number;

  /** Diameter of the arrow head */
  head_diameter: number;

  /** Length of the arrow head */
  head_length: number;

  /** Color of the arrow */
  color: Color;
};
