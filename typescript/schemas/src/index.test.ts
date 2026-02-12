/**
 * Tests for the deprecated @foxglove/schemas package.
 *
 * This package re-exports everything from @foxglove/messages for backward compatibility.
 */

import * as messages from "@foxglove/messages";

import * as schemas from "./index";

// Compile-time type tests
import type {
  CompressedImage as MessagesCompressedImage,
  Log as MessagesLog,
  PointCloud as MessagesPointCloud,
  SceneUpdate as MessagesSceneUpdate,
} from "@foxglove/messages";

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
    const messageKeys = Object.keys(messages);
    const schemaKeys = Object.keys(schemas);

    // Every export from messages should be in schemas
    for (const key of messageKeys) {
      expect(schemaKeys).toContain(key);
      expect((schemas as Record<string, unknown>)[key]).toBe(
        (messages as Record<string, unknown>)[key],
      );
    }
  });
});
