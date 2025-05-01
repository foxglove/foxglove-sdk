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

function toSnakeCase(name: string) {
  const snakeName = name.replace("JSON", "Json").replace(/([A-Z])/g, "_$1").toLowerCase();
  return snakeName.startsWith("_") ? snakeName.substring(1) : snakeName;
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

function cppToC(schema: FoxgloveMessageSchema) {
  return schema.fields.map((field) => {
    const srcName = toSnakeCase(field.name);
    const dstName = srcName;
    if (field.array != undefined) {
      if (typeof field.array === "number") {
        return `::memcpy(cMsg.${dstName}, msg.${srcName}.data(), msg.${srcName}.size() * sizeof(*msg.${srcName}.data()));`;
      } else {
        if (field.type.type === "nested") {
          return `cMsg.${dstName} = reinterpret_cast<const foxglove_${toSnakeCase(field.type.schema.name)}*>(msg.${srcName}.data());\n    cMsg.${dstName}_count = msg.${srcName}.size();`;
        } else if (field.type.type === "primitive") {
          assert(field.type.name !== "bytes");
          return `cMsg.${dstName} = msg.${srcName}.data();\n    cMsg.${dstName}_count = msg.${srcName}.size();`;
        } else {
          throw Error(`unsupported array type: ${field.type.type}`);
        }
      }
    }
    switch (field.type.type) {
      case "primitive":
        if (field.type.name === "string") {
          return `cMsg.${dstName} = {msg.${srcName}.data(), msg.${srcName}.size()};`;
        } else if (field.type.name === "bytes") {
          return `cMsg.${dstName} = reinterpret_cast<const unsigned char *>(msg.${srcName}.data());\n    cMsg.${dstName}_len = msg.${srcName}.size();`;
        } else if (field.type.name === "time") {
          return `cMsg.${dstName} = msg.${srcName} ? reinterpret_cast<const foxglove_timestamp*>(&*msg.${srcName}) : nullptr;`;
        } else if (field.type.name === "duration") {
          return `cMsg.${dstName} = msg.${srcName} ? reinterpret_cast<const foxglove_duration*>(&*msg.${srcName}) : nullptr;`;
        }
        return `cMsg.${dstName} = msg.${srcName};`;
      case "enum":
        return `cMsg.${dstName} = static_cast<foxglove_${toSnakeCase(field.type.enum.name)}>(msg.${srcName});`;
      case "nested":
        return `cMsg.${dstName} = msg.${srcName} ? reinterpret_cast<const foxglove_${toSnakeCase(field.type.schema.name)}*>(&*msg.${srcName}) : nullptr;`;
    }
  }).join("\n    ");
}

export function generateCppSchemas(
  schemas: FoxgloveMessageSchema[],
): string {
  // Sort by name
  schemas.sort((a, b) => a.name.localeCompare(b.name));

  const traitSpecializations = schemas.flatMap(
    (schema) => {
      if (schema.name.endsWith("Primitive")) {
        return [];
      }
      return [
        "template<>",
        `struct BuiltinSchema<${schema.name}> : std::true_type {`,
        `  inline FoxgloveError log_to(foxglove_channel * const channel, const ${schema.name}& msg, std::optional<uint64_t> logTime = std::nullopt) {`,
        `    foxglove_${toSnakeCase(schema.name)} cMsg;`,
        `    ${cppToC(schema)}`,
        `    return FoxgloveError(foxglove_channel_log_${toSnakeCase(schema.name)}(channel, &cMsg, logTime ? &*logTime : nullptr));`,
        "  }\n};",
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

    traitSpecializations.join("\n"),

    "} // namespace foxglove::schemas",
  ];

  return outputSections.join("\n\n") + "\n";
}
