import { exportTypeScriptSchemas } from "./exportTypeScriptSchemas";

describe("exportTypeScriptSchemas", () => {
  it("exports schemas", () => {
    const schemas = exportTypeScriptSchemas();
    expect(schemas).toMatchSnapshot();
  });
});
