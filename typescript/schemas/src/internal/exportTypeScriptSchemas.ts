import {
  GenerateTypeScriptOptions,
  LEGACY_TIME_TS,
  generateTypeScript,
} from "./generateTypeScript";
import { foxgloveEnumSchemas, foxgloveMessageSchemas } from "./schemas";

/**
 * Export schemas as TypeScript source code, keyed by the file base name (without `.ts` suffix).
 *
 * @returns a map of file base name => schema source.
 */
export function exportTypeScriptSchemas(
  options: GenerateTypeScriptOptions = {},
): Map<string, string> {
  const schemas = new Map<string, string>();

  for (const schema of Object.values(foxgloveMessageSchemas)) {
    schemas.set(schema.name, generateTypeScript(schema, options));
  }

  for (const schema of Object.values(foxgloveEnumSchemas)) {
    schemas.set(schema.name, generateTypeScript(schema, options));
  }

  schemas.set("Time", LEGACY_TIME_TS);

  const allSchemaNames = [
    ...Object.values(foxgloveMessageSchemas),
    ...Object.values(foxgloveEnumSchemas),
  ]
    .map((schema) => schema.name)
    // Include the legacy `Time` export for backwards compatibility.
    .concat(["Time"])
    .sort((a, b) => a.localeCompare(b));
  let indexTS = "";
  for (const schemaName of allSchemaNames) {
    indexTS += `export * from "./${schemaName}";\n`;
  }
  schemas.set("index", indexTS);

  return schemas;
}
