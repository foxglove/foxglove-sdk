import { foxgloveEnumSchemas, foxgloveMessageSchemas } from "./schemas";
import { FoxgloveMessageSchema } from "./types";

describe("schemas", () => {
  it("has matching names", () => {
    for (const [key, value] of Object.entries(foxgloveMessageSchemas)) {
      expect(key).toEqual(value.name);
    }
    for (const [key, value] of Object.entries(foxgloveEnumSchemas)) {
      expect(key).toEqual(value.name);
      expect(value.parentSchemaName in foxgloveMessageSchemas).toBe(true);
    }
  });

  const allSchemas = new Set<FoxgloveMessageSchema>();
  function addNestedSchemas(schema: FoxgloveMessageSchema, schemas: Set<FoxgloveMessageSchema>) {
    if (schemas.has(schema)) {
      return;
    }
    schemas.add(schema);
    for (const field of schema.fields) {
      if (field.type.type === "nested") {
        addNestedSchemas(field.type.schema, schemas);
      }
    }
  }
  const exportedSchemas = Object.values(foxgloveMessageSchemas);
  exportedSchemas.forEach((schema) => {
    addNestedSchemas(schema, allSchemas);
  });
  it.each([...allSchemas])("exports nested schemas - $name", (nestedSchema) => {
    expect(exportedSchemas).toContain(nestedSchema);
  });

  it("has valid descriptions", () => {
    for (const schema of Object.values(foxgloveMessageSchemas)) {
      // Ensure descriptions don't contain comment-closing sequences that would break generated code
      expect(schema.description.includes("*/")).toBe(false);
      for (const field of schema.fields) {
        expect(field.description.includes("*/")).toBe(false);
      }
    }
  });
});
