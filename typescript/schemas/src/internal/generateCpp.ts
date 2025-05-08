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
  const snakeName = name
    .replace("JSON", "Json")
    .replace(/([A-Z])/g, "_$1")
    .toLowerCase();
  return snakeName.startsWith("_") ? snakeName.substring(1) : snakeName;
}

function isSameAsCType(schema: FoxgloveMessageSchema): boolean {
  return schema.fields.every(
    (field) =>
      field.type.type === "primitive" &&
      field.type.name !== "bytes" &&
      field.type.name !== "string",
  );
}

function hasChannelType(schema: FoxgloveMessageSchema): boolean {
  return !schema.name.endsWith("Primitive") && schema.name !== "Color";
}

/**
 * Yield `schemas` in an order such that dependencies come before dependents, so structs don't end
 * up referencing [incomplete types](https://en.cppreference.com/w/cpp/language/incomplete_type).
 */
function* topologicalOrder(
  schemas: readonly FoxgloveMessageSchema[],
  seenSchemaNames = new Set<string>(),
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
              fieldType = field.type.schema.name;
              break;
            case "primitive": {
              defaultStr =
                field.array != undefined
                  ? ` = ${(field.defaultValue ?? primitiveDefaultValue(field.type.name) ?? "").toString()}`
                  : "";
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

  const channelClasses = schemas.filter(hasChannelType).map(
    (schema) => `/// @brief A channel for logging ${schema.name} messages to a topic.
      class ${schema.name}Channel {
      public:
        /// @brief Create a new channel.
        ///
        /// @param topic The topic name. You should choose a unique topic name per channel for
        /// compatibility with the Foxglove app.
        /// @param context The context which associates logs to a sink. If omitted, the default context is
        /// used.
        static FoxgloveResult<${schema.name}Channel> create(const std::string_view& topic, const Context& context = Context());

        /// @brief Log a message to the channel.
        ///
        /// @param value The ${schema.name} message to log.
        /// @param log_time The timestamp of the message. If omitted, the current time is used.
        FoxgloveError log(const ${schema.name}& value, std::optional<uint64_t> log_time = std::nullopt);

        /// @brief Uniquely identifies a channel in the context of this program.
        ///
        /// @return The ID of the channel.
        [[nodiscard]] uint64_t id() const;

        ${schema.name}Channel(const ${schema.name}Channel& other) noexcept = delete;
        ${schema.name}Channel& operator=(const ${schema.name}Channel& other) noexcept = delete;
        /// @brief Default move constructor.
        ${schema.name}Channel(${schema.name}Channel&& other) noexcept = default;
        /// @brief Default destructor.
        ~${schema.name}Channel() = default;

      private:
        explicit ${schema.name}Channel(ChannelUniquePtr&& channel)
            : impl_(std::move(channel)) {}

        ChannelUniquePtr impl_;
    };`,
  );

  const includes = [
    "#include <array>",
    "#include <cstdint>",
    "#include <string>",
    "#include <type_traits>",
    "#include <vector>",
    "#include <optional>",
    "#include <memory>",
    "",
    "#include <foxglove/time.hpp>",
    "#include <foxglove/error.hpp>",
    "#include <foxglove/context.hpp>",
  ];

  const uniquePtr = [
    "/// @brief A functor for deleting a channel. Used by ChannelUniquePtr. For internal use only.",
    "struct ChannelDeleter {",
    "  void operator()(const foxglove_channel* ptr) const noexcept;",
    "};",
    "/// @brief A unique pointer to a C foxglove_channel pointer. For internal use only.",
    "typedef std::unique_ptr<const foxglove_channel, ChannelDeleter> ChannelUniquePtr;",
  ];

  const outputSections = [
    "// Generated by https://github.com/foxglove/foxglove-sdk",

    "#pragma once",
    includes.join("\n"),

    "struct foxglove_channel;",

    "namespace foxglove::schemas {",

    uniquePtr.join("\n"),

    structDefs.join("\n\n"),

    channelClasses.join("\n\n"),

    "} // namespace foxglove::schemas",
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

export function generateCppSchemas(schemas: FoxgloveMessageSchema[]): string {
  // Sort by name
  schemas.sort((a, b) => a.name.localeCompare(b.name));

  const copyTypes = new Set(
    schemas
      .map((schema) => {
        return isSameAsCType(schema) ? schema.name : "";
      })
      .filter((name) => name.length > 0),
  );

  const conversionFuncDecls = schemas.flatMap((schema) => {
    if (isSameAsCType(schema)) {
      return [];
    }
    return [
      `void ${toCamelCase(schema.name)}ToC(foxglove_${toSnakeCase(schema.name)}& dest, const ${schema.name}& src, Arena& arena);`,
    ];
  });

  const traitSpecializations = schemas.flatMap((schema) => {
    if (!hasChannelType(schema)) {
      return [];
    }

    const snakeName = toSnakeCase(schema.name);
    let conversionCode;
    if (isSameAsCType(schema)) {
      conversionCode = [
        `    return FoxgloveError(foxglove_channel_log_${snakeName}(impl_.get(), reinterpret_cast<const foxglove_${snakeName}*>(&msg), log_time ? &*log_time : nullptr));`,
      ];
    } else {
      conversionCode = [
        "    Arena arena;",
        `    foxglove_${snakeName} c_msg;`,
        `    ${toCamelCase(schema.name)}ToC(c_msg, msg, arena);`,
        `    return FoxgloveError(foxglove_channel_log_${snakeName}(impl_.get(), &c_msg, log_time ? &*log_time : nullptr));`,
      ];
    }

    return [
      `FoxgloveResult<${schema.name}Channel> ${schema.name}Channel::create(const std::string_view& topic, const Context& context) {`,
      "    const foxglove_channel* channel = nullptr;",
      `    foxglove_error error = foxglove_channel_create_${snakeName}({topic.data(), topic.size()}, context.getInner(), &channel);`,
      "    if (error != foxglove_error::FOXGLOVE_ERROR_OK || channel == nullptr) {",
      "      return foxglove::unexpected(FoxgloveError(error));",
      "    }",
      `    return ${schema.name}Channel(ChannelUniquePtr(channel));`,
      "}\n",
      `FoxgloveError ${schema.name}Channel::log(const ${schema.name}& msg, std::optional<uint64_t> log_time) {`,
      ...conversionCode,
      "}\n",
      `uint64_t ${schema.name}Channel::id() const {`,
      "    return foxglove_channel_get_id(impl_.get());",
      "}\n\n",
    ];
  });

  const conversionFuncs = schemas.flatMap((schema) => {
    if (isSameAsCType(schema)) {
      return [];
    }
    return [
      `void ${toCamelCase(schema.name)}ToC(foxglove_${toSnakeCase(schema.name)}& dest, const ${schema.name}& src, Arena& arena) {`,
      `    ${cppToC(schema, copyTypes).join("\n    ")}`,
      "}\n",
    ];
  });

  const channelUniquePtr = [
    "void ChannelDeleter::operator()(const foxglove_channel* ptr) const noexcept {",
    "  foxglove_channel_free(ptr);",
    "};",
  ];

  const systemIncludes = ["#include <optional>", "#include <cstring>"];

  const includes = [
    "#include <foxglove/error.hpp>",
    "#include <foxglove/schemas.hpp>",
    "#include <foxglove/arena.hpp>",
    "#include <foxglove/context.hpp>",
  ];

  const outputSections = [
    "// Generated by https://github.com/foxglove/foxglove-sdk",

    "#include <foxglove-c/foxglove-c.h>",

    includes.join("\n"),

    systemIncludes.join("\n"),

    "namespace foxglove::schemas {",

    channelUniquePtr.join("\n"),

    conversionFuncDecls.join("\n"),

    traitSpecializations.join("\n"),

    conversionFuncs.join("\n"),

    "} // namespace foxglove::schemas",
  ];

  return outputSections.join("\n\n") + "\n";
}
