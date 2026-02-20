// Backward-compatibility test: verifies @foxglove/schemas re-exports everything from @foxglove/messages.

import * as messages from "@foxglove/messages";

import * as schemas from "./index";

describe("@foxglove/schemas backward compatibility", () => {
  it("re-exports all named exports from @foxglove/messages", () => {
    const messageKeys = Object.keys(messages).sort();
    const schemaKeys = Object.keys(schemas).sort();
    expect(schemaKeys).toEqual(messageKeys);
  });

  it("re-exports identical values", () => {
    for (const key of Object.keys(messages)) {
      expect((schemas as Record<string, unknown>)[key]).toBe(
        (messages as Record<string, unknown>)[key],
      );
    }
  });
});
