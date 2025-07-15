import { foxgloveMessageSchemas } from "./schemas";
import { FoxgloveEnumSchema, FoxgloveMessageSchema, FoxglovePrimitive } from "./types";

const { Duration, Timestamp } = foxgloveMessageSchemas;

const allPrimitives: FoxglovePrimitive[] = ["boolean", "bytes", "float64", "uint32", "string"];

export const exampleEnum: FoxgloveEnumSchema = {
  type: "enum",
  name: "ExampleEnum",
  protobufEnumName: "ExampleProtoEnum",
  parentSchemaName: "ExampleMessage",
  description: "An example enum",
  values: [
    { name: "A", value: 0, description: "Value A" },
    { name: "B", value: 1, description: "Value B" },
  ],
};

const exampleNestedMessage: FoxgloveMessageSchema = {
  type: "message",
  name: "NestedMessage",
  description: "An example nested message",
  fields: [
    {
      name: "field_enum",
      description: "An enum field",
      type: { type: "primitive", name: "uint32" },
    },
  ],
};

export const exampleMessage: FoxgloveMessageSchema = {
  type: "message",
  name: "ExampleMessage",
  description: "An example type",
  fields: [
    {
      name: "field_duration",
      description: "duration field",
      type: { type: "nested", schema: Duration },
    },
    {
      name: "field_time",
      description: "time field",
      type: { type: "nested", schema: Timestamp },
    },
    ...allPrimitives.map((name): FoxgloveMessageSchema["fields"][0] => ({
      name: `field_${name}`,
      description: `${name} field`,
      type: { type: "primitive", name },
      defaultValue:
        name === "boolean"
          ? true
          : name === "string"
            ? "string-type"
            : name === "uint32"
              ? 5
              : name === "float64"
                ? 1.0
                : // time and duration and bytes
                  undefined,
    })),
    {
      name: "field_duration_array",
      description: "duration array field",
      type: { type: "nested", schema: Duration },
      array: true,
    },
    {
      name: "field_time_array",
      description: "time array field",
      type: { type: "nested", schema: Timestamp },
      array: true,
    },
    ...allPrimitives.map((name): FoxgloveMessageSchema["fields"][0] => ({
      name: `field_${name}_array`,
      description: `${name} array field`,
      type: { type: "primitive", name },
      array: true,
    })),
    {
      name: "field_duration_fixed_array",
      description: "duration fixed-length array field",
      type: { type: "nested", schema: Duration },
      array: 3,
    },
    {
      name: "field_time_fixed_array",
      description: "time fixed-length array field",
      type: { type: "nested", schema: Timestamp },
      array: 3,
    },
    ...allPrimitives.map((name): FoxgloveMessageSchema["fields"][0] => ({
      name: `field_${name}_fixed_array`,
      description: `${name} fixed-length array field`,
      type: { type: "primitive", name },
      array: 3,
    })),
    {
      name: "field_enum",
      description: "An enum field",
      type: { type: "enum", enum: exampleEnum },
    },
    {
      name: "field_enum_array",
      description: "An enum array field",
      type: { type: "enum", enum: exampleEnum },
      array: true,
    },
    {
      name: "field_nested",
      description: "A nested field",
      type: { type: "nested", schema: exampleNestedMessage },
    },
    {
      name: "field_nested_array",
      description: "A nested array field\nWith\na\nvery\nlong\ndescription",
      type: { type: "nested", schema: exampleNestedMessage },
      array: true,
      protobufFieldNumber: 4,
    },
  ],
};

export const exampleMessageWithoutArrayOfBytes: FoxgloveMessageSchema = {
  ...exampleMessage,
  fields: exampleMessage.fields.filter(
    ({ name }) => name !== "field_bytes_array" && name !== "field_bytes_fixed_array",
  ),
};
