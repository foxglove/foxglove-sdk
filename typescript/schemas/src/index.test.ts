/**
 * Tests for the deprecated @foxglove/schemas package.
 *
 * This package re-exports everything from @foxglove/messages for backward compatibility.
 */

import * as messages from "@foxglove/messages";
import type {
  CompressedImage as MessagesCompressedImage,
  Log as MessagesLog,
  PointCloud as MessagesPointCloud,
  SceneUpdate as MessagesSceneUpdate,
} from "@foxglove/messages";

import * as schemas from "./index";
import type {
  CompressedImage as SchemasCompressedImage,
  Log as SchemasLog,
  PointCloud as SchemasPointCloud,
  SceneUpdate as SchemasSceneUpdate,
} from "./index";

type IsEqual<LeftType, RightType> = [LeftType] extends [RightType]
  ? [RightType] extends [LeftType]
    ? true
    : false
  : false;
type AssertType<Condition extends true> = Condition;

export type CompressedImageTypeIsReexported = AssertType<
  IsEqual<SchemasCompressedImage, MessagesCompressedImage>
>;
export type LogTypeIsReexported = AssertType<IsEqual<SchemasLog, MessagesLog>>;
export type SceneUpdateTypeIsReexported = AssertType<
  IsEqual<SchemasSceneUpdate, MessagesSceneUpdate>
>;
export type PointCloudTypeIsReexported = AssertType<IsEqual<SchemasPointCloud, MessagesPointCloud>>;

// Runtime tests
describe("@foxglove/schemas backward compatibility", () => {
  it("re-exports all named exports from @foxglove/messages", () => {
    const messageExports = messages as Record<string, unknown>;
    const schemaExports = schemas as Record<string, unknown>;
    const messageKeys = Object.keys(messageExports);
    const schemaKeys = Object.keys(schemaExports);

    // Every export from messages should be in schemas
    for (const key of messageKeys) {
      expect(schemaKeys).toContain(key);
      expect(schemaExports[key]).toBe(messageExports[key]);
    }
  });
});
