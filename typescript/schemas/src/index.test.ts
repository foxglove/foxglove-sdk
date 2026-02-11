/**
 * Tests for the deprecated @foxglove/schemas package.
 *
 * This package re-exports everything from @foxglove/messages for backward compatibility.
 */

import * as schemas from "./index";
import * as messages from "@foxglove/messages";

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
    expect(schemas.CompressedImage).toBe(messages.CompressedImage);
    expect(schemas.Log).toBe(messages.Log);
    expect(schemas.SceneUpdate).toBe(messages.SceneUpdate);
    expect(schemas.PointCloud).toBe(messages.PointCloud);
  });
});
