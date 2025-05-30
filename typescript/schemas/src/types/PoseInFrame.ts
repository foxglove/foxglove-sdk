// Generated by https://github.com/foxglove/foxglove-sdk
// Options: {}

import { Pose } from "./Pose";
import { Time } from "./Time";

/** A timestamped pose for an object or reference frame in 3D space */
export type PoseInFrame = {
  /** Timestamp of pose */
  timestamp: Time;

  /** Frame of reference for pose position and orientation */
  frame_id: string;

  /** Pose in 3D space */
  pose: Pose;
};
