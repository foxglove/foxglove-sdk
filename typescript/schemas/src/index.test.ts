/**
 * Tests for the deprecated @foxglove/schemas package.
 *
 * This package re-exports everything from @foxglove/messages for backward compatibility.
 */

import { CompressedImage, Log, PointCloud, SceneUpdate } from "@foxglove/messages";

import * as schemas from "./index";

describe("@foxglove/schemas backward compatibility", () => {
  it("re-exports all types from @foxglove/messages", () => {
    // Verify key types are exported
    expect(schemas.CompressedImage).toBeDefined();
    expect(schemas.Log).toBeDefined();
    expect(schemas.SceneUpdate).toBeDefined();
    expect(schemas.PointCloud).toBeDefined();
  });

  it("exports the same types as @foxglove/messages", () => {
    // Verify type identity
    expect(schemas.CompressedImage).toBe(CompressedImage);
    expect(schemas.Log).toBe(Log);
    expect(schemas.SceneUpdate).toBe(SceneUpdate);
    expect(schemas.PointCloud).toBe(PointCloud);
  });
});
