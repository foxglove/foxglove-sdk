// Backward-compatibility test: verifies @foxglove/schemas re-exports everything from @foxglove/messages.

import * as messages from "@foxglove/messages";

import * as schemas from "./index";

describe("@foxglove/schemas backward compatibility", () => {
  it("re-exports all named exports from @foxglove/messages", () => {
    const messageExports = messages as Record<string, unknown>;
    const schemaExports = schemas as Record<string, unknown>;
    const messageKeys = Object.keys(messageExports).sort();
    const schemaKeys = Object.keys(schemaExports).sort();

    expect(schemaKeys).toEqual(messageKeys);

    for (const key of messageKeys) {
      expect(schemaExports[key]).toBe(messageExports[key]);
    }
  });
});
