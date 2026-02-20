// Backward-compatibility test: verifies @foxglove/schemas re-exports everything from @foxglove/messages.

import * as messages from "@foxglove/messages";

import * as schemas from "./index";

const messagesRecord: Record<string, unknown> = messages;
const schemasRecord: Record<string, unknown> = schemas;

describe("@foxglove/schemas backward compatibility", () => {
  it("re-exports all named exports from @foxglove/messages", () => {
    const messageKeys = Object.keys(messagesRecord).sort();
    const schemaKeys = Object.keys(schemasRecord).sort();
    expect(schemaKeys).toEqual(messageKeys);
  });

  it("re-exports identical values", () => {
    for (const key of Object.keys(messagesRecord)) {
      expect(schemasRecord[key]).toBe(messagesRecord[key]);
    }
  });
});
