// Backward-compatibility test: verifies @foxglove/schemas re-exports everything from @foxglove/messages.

import * as messages from "@foxglove/messages";
import * as messagesInternal from "@foxglove/messages/internal";
import * as messagesJsonschema from "@foxglove/messages/jsonschema";

import * as schemas from "./index";

// These use require() because the subpath wrappers are CommonJS files outside of src/.
// eslint-disable-next-line @typescript-eslint/no-require-imports
const schemasInternal = require("@foxglove/schemas/internal") as Record<string, unknown>;
// eslint-disable-next-line @typescript-eslint/no-require-imports
const schemasJsonschema = require("@foxglove/schemas/jsonschema") as Record<string, unknown>;

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

  it("re-exports all named exports from @foxglove/messages/internal", () => {
    const messageInternalExports = messagesInternal as Record<string, unknown>;
    const messageInternalKeys = Object.keys(messageInternalExports).sort();
    const schemaInternalKeys = Object.keys(schemasInternal).sort();

    expect(schemaInternalKeys).toEqual(messageInternalKeys);

    for (const key of messageInternalKeys) {
      expect(schemasInternal[key]).toBe(messageInternalExports[key]);
    }
  });

  it("re-exports all named exports from @foxglove/messages/jsonschema", () => {
    const messageJsonschemaExports = messagesJsonschema as Record<string, unknown>;
    const messageJsonschemaKeys = Object.keys(messageJsonschemaExports).sort();
    const schemaJsonschemaKeys = Object.keys(schemasJsonschema).sort();

    expect(schemaJsonschemaKeys).toEqual(messageJsonschemaKeys);

    for (const key of messageJsonschemaKeys) {
      expect(schemasJsonschema[key]).toBe(messageJsonschemaExports[key]);
    }
  });
});
