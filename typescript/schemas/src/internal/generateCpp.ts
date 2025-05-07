import assert from "assert";
import { FoxgloveEnumSchema, FoxgloveMessageSchema, FoxglovePrimitive } from "./types";

function primitiveToCpp(type: FoxglovePrimitive) {
  switch (type) {
    case "uint32":
      return "uint32_t";
    case "bytes":
      return "std::vector<std::byte>";
    case "string":
      return "std::string";
    case "boolean":
      return "bool";
    case "float64":
      return "double";
    case "time":
      return "std::optional<foxglove::Timestamp>";
    case "duration":
      return "std::optional<foxglove::Duration>";
  }
}

function primitiveDefaultValue(type: FoxglovePrimitive) {
  switch (type) {
    case "uint32":
      return 0;
    case "boolean":
      return false;
    case "float64":
      return 0;
    case "string":
    case "bytes":
    case "time":
    case "duration":
      return undefined;
  }
}

function formatComment(comment: string, indent: number) {
  const spaces = " ".repeat(indent);
  return comment
    .split("\n")
    .map((line) => `${spaces}/// ${line}`)
    .join("\n");
}

function toCamelCase(name: string) {
  return name.substring(0, 1).toLowerCase() + name.substring(1);
}

function toSnakeCase(name: string) {
  const snakeName = name.replace("JSON", "Json").replace(/([A-Z])/g, "_$1").toLowerCase();
  return snakeName.startsWith("_") ? snakeName.substring(1) : snakeName;
}

function isSameAsCType(schema: FoxgloveMessageSchema): boolean {
  return schema.fields.every((field) => field.type.type === "primitive" && field.type.name !== "bytes" && field.type.name !== "string");
}

/**
 * Yield `schemas` in an order such that dependencies come before dependents, so structs don't end
 * up referencing [incomplete types](https://en.cppreference.com/w/cpp/language/incomplete_type).
 */
function* topologicalOrder(
  schemas: readonly FoxgloveMessageSchema[],
  seenSchemaNames: Set<string> = new Set(),
): Iterable<FoxgloveMessageSchema> {
  for (const schema of schemas) {
    if (seenSchemaNames.has(schema.name)) {
      continue;
    }
    seenSchemaNames.add(schema.name);
    for (const field of schema.fields) {
      if (field.type.type === "nested") {
        yield* topologicalOrder([field.type.schema], seenSchemaNames);
      }
    }
    yield schema;
  }
}

export function generateHppSchemas(
  schemas: readonly FoxgloveMessageSchema[],
  enums: readonly FoxgloveEnumSchema[],
): string {
  const enumsByParentSchema = new Map<string, FoxgloveEnumSchema>();
  for (const enumSchema of enums) {
    if (enumsByParentSchema.has(enumSchema.parentSchemaName)) {
      throw new Error(
        `Multiple enums with the same parent schema not currently supported ${enumSchema.parentSchemaName}`,
      );
    }
    enumsByParentSchema.set(enumSchema.parentSchemaName, enumSchema);
  }

  const orderedSchemas = Array.from(topologicalOrder(schemas));
  if (orderedSchemas.length !== schemas.length) {
    throw new Error(
      `Invariant: topologicalOrder should return same number of schemas (got ${orderedSchemas.length} instead of ${schemas.length})`,
    );
  }
  const structDefs = orderedSchemas.map((schema) => {
    let enumDef: string[] = [];
    const enumSchema = enumsByParentSchema.get(schema.name);
    if (enumSchema) {
      enumDef = [
        formatComment(enumSchema.description, 2),
        `  enum class ${enumSchema.name} : uint8_t {`,
        enumSchema.values
          .map((value) => {
            const comment =
              value.description != undefined ? formatComment(value.description, 4) + "\n" : "";
            return `${comment}    ${value.name.toUpperCase()} = ${value.value},`;
          })
          .join("\n"),
        `  };`,
      ];
    }
    return [
      formatComment(schema.description, 0),
      `struct ${schema.name} {`,
      ...enumDef,
      schema.fields
        .map((field) => {
          let fieldType;
          let defaultStr = "";
          switch (field.type.type) {
            case "enum":
              fieldType = field.type.enum.name;
              break;
            case "nested":
              fieldType = `${field.type.schema.name}`;
              break;
            case "primitive": {
              const defaultValue =
                field.array != undefined
                  ? undefined
                  : (field.defaultValue ?? primitiveDefaultValue(field.type.name));
              defaultStr = defaultValue != undefined ? ` = ${defaultValue}` : "";
              fieldType = primitiveToCpp(field.type.name);
              break;
            }
          }
          if (typeof field.array === "number") {
            fieldType = `std::array<${fieldType}, ${field.array}>`;
          } else if (field.array) {
            fieldType = `std::vector<${fieldType}>`;
          } else if (field.type.type === "nested") {
            fieldType = `std::optional<${fieldType}>`;
          }
          return `${formatComment(field.description, 2)}\n  ${fieldType} ${toSnakeCase(field.name)}${defaultStr};`;
        })
        .join("\n\n"),
      `};`,
    ].join("\n");
  });

  const traitSpecializations = schemas.filter((schema) => !schema.name.endsWith("Primitive")).map(
    (schema) =>
      `template<>\nstruct BuiltinSchema<foxglove::schemas::${schema.name}>;`
  );

  const includes = [
    "#include <array>",
    "#include <cstdint>",
    "#include <string>",
    "#include <type_traits>",
    "#include <vector>",
    "#include <optional>",
    "",
    "#include <foxglove/time.hpp>",
    "#include <foxglove/error.hpp>",
  ];

  const outputSections = [
    "// Generated by https://github.com/foxglove/foxglove-sdk",

    "#pragma once",
    includes.join("\n"),

    "struct foxglove_channel;",

    "namespace foxglove::schemas {",

    structDefs.join("\n\n"),

    "} // namespace foxglove::schemas",

    "namespace foxglove::internal {",

    "template<class T>\nstruct BuiltinSchema : std::false_type {};",
    traitSpecializations.join("\n"),

    "} // namespace foxglove::internal",
  ].filter(Boolean);

  return outputSections.join("\n\n") + "\n";
}

function cppToC(schema: FoxgloveMessageSchema, copyTypes: Set<string>): string[] {
  return schema.fields.map((field) => {
    const srcName = toSnakeCase(field.name);
    const dstName = srcName;
    if (field.array != undefined) {
      if (typeof field.array === "number") {
        return `::memcpy(dest.${dstName}, src.${srcName}.data(), src.${srcName}.size() * sizeof(*src.${srcName}.data()));`;
      } else {
        if (field.type.type === "nested") {
          if (copyTypes.has(field.type.schema.name)) {
            return `dest.${dstName} = reinterpret_cast<const foxglove_${toSnakeCase(field.type.schema.name)}*>(src.${srcName}.data());\n    dest.${dstName}_count = src.${srcName}.size();`;
          } else {
            return `dest.${dstName} = arena.map<foxglove_${toSnakeCase(field.type.schema.name)}>(src.${srcName}, ${toCamelCase(field.type.schema.name)}ToC);
    dest.${dstName}_count = src.${srcName}.size();`;
          }
        } else if (field.type.type === "primitive") {
          assert(field.type.name !== "bytes");
          return `dest.${dstName} = src.${srcName}.data();\n    dest.${dstName}_count = src.${srcName}.size();`;
        } else {
          throw Error(`unsupported array type: ${field.type.type}`);
        }
      }
    }
    switch (field.type.type) {
      case "primitive":
        if (field.type.name === "string") {
          return `dest.${dstName} = {src.${srcName}.data(), src.${srcName}.size()};`;
        } else if (field.type.name === "bytes") {
          return `dest.${dstName} = reinterpret_cast<const unsigned char *>(src.${srcName}.data());\n    dest.${dstName}_len = src.${srcName}.size();`;
        } else if (field.type.name === "time") {
          return `dest.${dstName} = src.${srcName} ? reinterpret_cast<const foxglove_timestamp*>(&*src.${srcName}) : nullptr;`;
        } else if (field.type.name === "duration") {
          return `dest.${dstName} = src.${srcName} ? reinterpret_cast<const foxglove_duration*>(&*src.${srcName}) : nullptr;`;
        }
        return `dest.${dstName} = src.${srcName};`;
      case "enum":
        return `dest.${dstName} = static_cast<foxglove_${toSnakeCase(field.type.enum.name)}>(src.${srcName});`;
      case "nested":
        if (copyTypes.has(field.type.schema.name)) {
          return `dest.${dstName} = src.${srcName} ? reinterpret_cast<const foxglove_${toSnakeCase(field.type.schema.name)}*>(&*src.${srcName}) : nullptr;`;
        } else {
          return `dest.${dstName} = src.${srcName} ? arena.map_one<foxglove_${toSnakeCase(field.type.schema.name)}>(src.${srcName}.value(), ${toCamelCase(field.type.schema.name)}ToC) : nullptr;`;
        }
    }
  });
}

export function generateCppSchemas(
  schemas: FoxgloveMessageSchema[],
): string {
  // Sort by name
  schemas.sort((a, b) => a.name.localeCompare(b.name));

  const copyTypes = new Set(schemas.map((schema) => {
    return isSameAsCType(schema) ? schema.name : "";
  }).filter((name) => name.length > 0));

  const conversionFuncDecls = schemas.flatMap(
    (schema) => {
      if (isSameAsCType(schema)) {
        return [];
      }
      return [`void ${toCamelCase(schema.name)}ToC(foxglove_${toSnakeCase(schema.name)}& dest, const ${schema.name}& src, Arena& arena);`];
    }
  );

  const traitSpecializations = schemas.flatMap(
    (schema) => {
      if (schema.name.endsWith("Primitive")) {
        return [];
      }

      let conversionCode;
      if (isSameAsCType(schema)) {
        conversionCode = [
          `    return FoxgloveError(foxglove_channel_log_${toSnakeCase(schema.name)}(channel, reinterpret_cast<const foxglove_${toSnakeCase(schema.name)}*>(&msg), logTime ? &*logTime : nullptr));`
        ];
      } else {
        conversionCode = ["    Arena arena;",
        `    foxglove_${toSnakeCase(schema.name)} c_msg;`,
        `    ${toCamelCase(schema.name)}ToC(c_msg, msg, arena);`,
        `    return FoxgloveError(foxglove_channel_log_${toSnakeCase(schema.name)}(channel, &c_msg, logTime ? &*logTime : nullptr));`];
      }

      return [
        "template<>",
        `struct BuiltinSchema<${schema.name}> : std::true_type {`,
        `  inline FoxgloveError logTo(foxglove_channel * const channel, const ${schema.name}& msg, std::optional<uint64_t> logTime = std::nullopt) {`,
        ...conversionCode,
        "  }\n};",
      ]
    }
  );

  const conversionFuncs = schemas.flatMap(
    (schema) => {
      if (isSameAsCType(schema)) {
        return [];
      }
      return [
        `void ${toCamelCase(schema.name)}ToC(foxglove_${toSnakeCase(schema.name)}& dest, const ${schema.name}& src, Arena& arena) {`,
        `    ${cppToC(schema, copyTypes).join("\n    ")}`,
        "}\n",
      ]
    }
  );

  const systemIncludes = [
    "#include <optional>",
    "#include <cstring>",
  ];

  const includes = [
    "#include <foxglove/error.hpp>",
    "#include <foxglove/schemas.hpp>",
    "#include <foxglove/arena.hpp>",
  ];

  const usings = [
    "using namespace foxglove;",
    "using namespace foxglove::schemas;",
  ];

  const outputSections = [
    "// Generated by https://github.com/foxglove/foxglove-sdk",

    "#include <foxglove-c/foxglove-c.h>",

    includes.join("\n"),

    systemIncludes.join("\n"),

    "namespace foxglove::internal {",

    usings.join("\n"),

    conversionFuncDecls.join("\n"),

    traitSpecializations.join("\n"),

    conversionFuncs.join("\n"),

    "} // namespace foxglove::schemas",
  ];

  return outputSections.join("\n\n") + "\n";
}
