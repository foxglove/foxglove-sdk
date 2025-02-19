// Generated by https://github.com/foxglove/foxglove-sdk
// Options: {}

import { Color } from "./Color";
import { Point3 } from "./Point3";
import { Pose } from "./Pose";

/** A primitive representing a set of triangles or a surface tiled by triangles */
export type TriangleListPrimitive = {
  /** Origin of triangles relative to reference frame */
  pose: Pose;

  /** Vertices to use for triangles, interpreted as a list of triples (0-1-2, 3-4-5, ...) */
  points: Point3[];

  /** Solid color to use for the whole shape. One of `color` or `colors` must be provided. */
  color: Color;

  /** Per-vertex colors (if specified, must have the same length as `points`). One of `color` or `colors` must be provided. */
  colors: Color[];

  /**
   * Indices into the `points` and `colors` attribute arrays, which can be used to avoid duplicating attribute data.
   * 
   * If omitted or empty, indexing will not be used. This default behavior is equivalent to specifying [0, 1, ..., N-1] for the indices (where N is the number of `points` provided).
   */
  indices: number[];
};
