import { describe, expect, it } from "vitest";

import { exportTypeScriptSchemas } from "./exportTypeScriptSchemas";

describe("exportTypeScriptSchemas", () => {
  it("exports schemas", () => {
    const schemas = exportTypeScriptSchemas();
    expect(schemas).toMatchSnapshot();
  });
});
