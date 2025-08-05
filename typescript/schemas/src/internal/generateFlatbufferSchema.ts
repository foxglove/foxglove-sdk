import {
  FoxgloveEnumSchema,
  FoxgloveMessageSchema,
  FoxglovePrimitive,
  FoxgloveSchema,
} from "./types";

// Flatbuffers only supports nested vectors via table
export const BYTE_VECTOR_FB = `
namespace foxglove;

/// Used for nesting byte vectors
table ByteVector {
  data:[uint8] (id: 0);
}
root_type ByteVector;
`;

export const TIME_FB = `
namespace foxglove;

struct Time {
  /// Represents seconds of UTC time since Unix epoch 1970-01-01T00:00:00Z
  sec:uint32;
  /// Nano-second fractions from 0 to 999,999,999 inclusive
  nsec:uint32;
}
`;

export const DURATION_FB = `
namespace foxglove;

struct Duration {
  /// Signed seconds of the span of time.
  sec:int32;
  /// if sec === 0 : -999,999,999 <= nsec <= +999,999,999
  /// otherwise sign of sec must match sign of nsec or be 0 and abs(nsec) <= 999,999,999
  nsec:int32;
}
`;

function primitiveToFlatbuffers(type: Exclude<FoxglovePrimitive, "time" | "duration">) {
  switch (type) {
    case "int32":
      return "int32";
    case "uint32":
      return "uint32";
    case "bytes":
      return "[uint8]";
    case "string":
      return "string";
    case "boolean":
      return "bool";
    case "float64":
      return "double";
  }
}

/**
 * Time is used instead of Timestamp for backwards compatibility.
 */
export function flatbufferMessageSchemaName(schema: FoxgloveMessageSchema): string {
  if (schema.name === "Timestamp") {
    return "Time";
  }
  return schema.name;
}

/**
 * Time and Duration are not namespaced, for backwards compatibility.
 */
function namespacedMessageSchemaName(schema: FoxgloveMessageSchema): string {
  if (schema.name === "Timestamp" || schema.name === "Duration") {
    return flatbufferMessageSchemaName(schema);
  }
  return `foxglove.${flatbufferMessageSchemaName(schema)}`;
}

export function generateFlatbuffers(
  schema: FoxgloveSchema,
  nestedEnums: FoxgloveEnumSchema[],
): string {
  const enumDefinitions: string[] = [];
  for (const enumSchema of nestedEnums) {
    const fields = enumSchema.values.map(({ name, value, description }) => {
      if (description != undefined) {
        return `/// ${description}\n  ${name} = ${value},`;
      } else {
        return `${name} = ${value},`;
      }
    });
    enumDefinitions.push(
      // `///` comments required to show up in compiled flatbuffer schemas
      `/// ${enumSchema.description}\nenum ${enumSchema.name} : ubyte {\n  ${fields.join(
        "\n\n  ",
      )}\n}\n`,
    );
  }

  let definition;
  const imports = new Set<string>();
  switch (schema.type) {
    case "enum": {
      const fields = schema.values.map(({ name, value, description }) => {
        if (description != undefined) {
          return `/// ${description}\n  ${name} = ${value},`;
        } else {
          return `${name} = ${value},`;
        }
      });

      // `///` comments required to show up in compiled flatbuffer schemas
      definition = `/// ${schema.description}\nenum ${schema.name} : ubyte {\n  ${fields.join(
        "\n\n  ",
      )}\n}\n`;
      break;
    }
    case "message": {
      const fields = schema.fields.map((field, fieldId) => {
        const isArray = field.array != undefined;

        let type;
        switch (field.type.type) {
          case "enum":
            type = field.type.enum.name;
            break;
          case "nested":
            type = namespacedMessageSchemaName(field.type.schema);
            imports.add(flatbufferMessageSchemaName(field.type.schema));
            break;
          case "primitive":
            if (field.type.name === "bytes" && isArray) {
              type = "ByteVector";
              imports.add("ByteVector");
            } else {
              type = primitiveToFlatbuffers(field.type.name);
            }
            break;
        }
        let lengthComment;

        if (typeof field.array === "number") {
          // can't specify length of vector outside of struct, all of these are tables
          lengthComment = `  /// length ${field.array}\n`;
        }
        let defaultValue;
        if (field.defaultValue != undefined && !isArray) {
          if (field.type.type === "primitive") {
            if (typeof field.defaultValue === "string") {
              defaultValue = `"${field.defaultValue}"`;
            } else if (typeof field.defaultValue === "number") {
              if (Number.isInteger(field.defaultValue) && field.type.name === "float64") {
                // if it is a floating point number that is an integer, we need to add a decimal point
                defaultValue = `${field.defaultValue}.0`;
              } else {
                defaultValue = field.defaultValue.toString();
              }
            } else if (typeof field.defaultValue === "boolean") {
              // uses same 'false'/'true' as js
              defaultValue = field.defaultValue.toString();
            }
          } else if (field.type.type === "enum") {
            // default enums are just the enum string of the enum and don't require other formatting
            // ie: type numericType: NumericType = INT32;
            defaultValue = field.defaultValue as string;
          }
        }
        if (field.defaultValue != undefined && defaultValue == undefined) {
          throw new Error("Flatbuffers does not support non-scalar default values");
        }

        return `${field.description
          .trim()
          .split("\n")
          .map((line) => `  /// ${line}\n`)
          .join("")}${
          // can't have inline comments, so the lengthComment needs to be above
          lengthComment ?? ""
          // convert field.name to lowercase for flatbuffer compilation compliance
        }  ${field.name.toLowerCase()}:${isArray ? `[${type}]` : type}${
          defaultValue ? ` = ${defaultValue}` : ""
        } (id: ${fieldId});`;
      });

      definition = `${enumDefinitions.join("\n\n")}/// ${schema.description}\ntable ${
        schema.name
      } {\n${fields.join("\n\n")}\n}\n\nroot_type ${schema.name};`;
      break;
    }
  }

  const outputSections = [
    `// Generated by https://github.com/foxglove/foxglove-sdk`,

    Array.from(imports)
      .sort()
      .map((name) => `include "${name}.fbs";`)
      .join("\n"),

    `namespace foxglove;`,

    definition,
  ].filter(Boolean);

  return outputSections.join("\n\n") + "\n";
}
