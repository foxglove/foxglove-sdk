import { exportTypeScriptSchemas } from "./exportTypeScriptSchemas.ts";

describe("exportTypeScriptSchemas", () => {
  it("exports schemas", () => {
    const schemas = exportTypeScriptSchemas();
    expect(schemas).toMatchSnapshot();
  });
});
