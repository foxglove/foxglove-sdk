import { parse as parseMessageDefinition } from "@foxglove/rosmsg";

import {
  generateRosMsgMergedSchema,
  generateRosMsgDefinition,
  generateRosMsg,
} from "./generateRos";
import { foxgloveMessageSchemas } from "./schemas";
import { exampleMessageWithoutArrayOfBytes } from "./testFixtures";

describe("generateRosMsgDefinition", () => {
  it("generates msg definition description for ROS 1", () => {
    expect(generateRosMsgDefinition(exampleMessageWithoutArrayOfBytes, { rosVersion: 1 }))
      .toMatchInlineSnapshot(`
      {
        "description": "An example type",
        "fields": [
          {
            "arrayLength": undefined,
            "description": "duration field",
            "isArray": false,
            "isComplex": false,
            "name": "field_duration",
            "type": "duration",
          },
          {
            "arrayLength": undefined,
            "description": "time field",
            "isArray": false,
            "isComplex": false,
            "name": "field_time",
            "type": "time",
          },
          {
            "arrayLength": undefined,
            "description": "boolean field",
            "isArray": false,
            "isComplex": false,
            "name": "field_boolean",
            "type": "bool",
          },
          {
            "arrayLength": undefined,
            "description": "bytes field",
            "isArray": true,
            "isComplex": false,
            "name": "field_bytes",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "float64 field",
            "isArray": false,
            "isComplex": false,
            "name": "field_float64",
            "type": "float64",
          },
          {
            "arrayLength": undefined,
            "description": "uint32 field",
            "isArray": false,
            "isComplex": false,
            "name": "field_uint32",
            "type": "uint32",
          },
          {
            "arrayLength": undefined,
            "description": "string field",
            "isArray": false,
            "isComplex": false,
            "name": "field_string",
            "type": "string",
          },
          {
            "arrayLength": undefined,
            "description": "duration array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_duration_array",
            "type": "duration",
          },
          {
            "arrayLength": undefined,
            "description": "time array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_time_array",
            "type": "time",
          },
          {
            "arrayLength": undefined,
            "description": "boolean array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_boolean_array",
            "type": "bool",
          },
          {
            "arrayLength": undefined,
            "description": "float64 array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_float64_array",
            "type": "float64",
          },
          {
            "arrayLength": undefined,
            "description": "uint32 array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_uint32_array",
            "type": "uint32",
          },
          {
            "arrayLength": undefined,
            "description": "string array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_string_array",
            "type": "string",
          },
          {
            "arrayLength": 3,
            "description": "duration fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_duration_fixed_array",
            "type": "duration",
          },
          {
            "arrayLength": 3,
            "description": "time fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_time_fixed_array",
            "type": "time",
          },
          {
            "arrayLength": 3,
            "description": "boolean fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_boolean_fixed_array",
            "type": "bool",
          },
          {
            "arrayLength": 3,
            "description": "float64 fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_float64_fixed_array",
            "type": "float64",
          },
          {
            "arrayLength": 3,
            "description": "uint32 fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_uint32_fixed_array",
            "type": "uint32",
          },
          {
            "arrayLength": 3,
            "description": "string fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_string_fixed_array",
            "type": "string",
          },
          {
            "description": "Value A",
            "isConstant": true,
            "name": "A",
            "type": "uint8",
            "value": 0,
            "valueText": "0",
          },
          {
            "description": "Value B",
            "isConstant": true,
            "name": "B",
            "type": "uint8",
            "value": 1,
            "valueText": "1",
          },
          {
            "arrayLength": undefined,
            "description": "An enum field",
            "isArray": false,
            "isComplex": false,
            "name": "field_enum",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "An enum array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_enum_array",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "A nested field",
            "isArray": false,
            "isComplex": true,
            "name": "field_nested",
            "type": "foxglove_msgs/NestedMessage",
          },
          {
            "arrayLength": undefined,
            "description": "A nested array field
      With
      a
      very
      long
      description",
            "isArray": true,
            "isComplex": true,
            "name": "field_nested_array",
            "type": "foxglove_msgs/NestedMessage",
          },
        ],
        "originalName": "ExampleMessage",
        "rosFullInterfaceName": "foxglove_msgs/ExampleMessage",
        "rosMsgInterfaceName": "foxglove_msgs/ExampleMessage",
      }
    `);
  });
  it("generates msg definition description for ROS 2", () => {
    expect(generateRosMsgDefinition(exampleMessageWithoutArrayOfBytes, { rosVersion: 2 }))
      .toMatchInlineSnapshot(`
      {
        "description": "An example type",
        "fields": [
          {
            "arrayLength": undefined,
            "description": "duration field",
            "isArray": false,
            "isComplex": false,
            "name": "field_duration",
            "type": "duration",
          },
          {
            "arrayLength": undefined,
            "description": "time field",
            "isArray": false,
            "isComplex": false,
            "name": "field_time",
            "type": "time",
          },
          {
            "arrayLength": undefined,
            "description": "boolean field",
            "isArray": false,
            "isComplex": false,
            "name": "field_boolean",
            "type": "bool",
          },
          {
            "arrayLength": undefined,
            "description": "bytes field",
            "isArray": true,
            "isComplex": false,
            "name": "field_bytes",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "float64 field",
            "isArray": false,
            "isComplex": false,
            "name": "field_float64",
            "type": "float64",
          },
          {
            "arrayLength": undefined,
            "description": "uint32 field",
            "isArray": false,
            "isComplex": false,
            "name": "field_uint32",
            "type": "uint32",
          },
          {
            "arrayLength": undefined,
            "description": "string field",
            "isArray": false,
            "isComplex": false,
            "name": "field_string",
            "type": "string",
          },
          {
            "arrayLength": undefined,
            "description": "duration array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_duration_array",
            "type": "duration",
          },
          {
            "arrayLength": undefined,
            "description": "time array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_time_array",
            "type": "time",
          },
          {
            "arrayLength": undefined,
            "description": "boolean array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_boolean_array",
            "type": "bool",
          },
          {
            "arrayLength": undefined,
            "description": "float64 array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_float64_array",
            "type": "float64",
          },
          {
            "arrayLength": undefined,
            "description": "uint32 array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_uint32_array",
            "type": "uint32",
          },
          {
            "arrayLength": undefined,
            "description": "string array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_string_array",
            "type": "string",
          },
          {
            "arrayLength": 3,
            "description": "duration fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_duration_fixed_array",
            "type": "duration",
          },
          {
            "arrayLength": 3,
            "description": "time fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_time_fixed_array",
            "type": "time",
          },
          {
            "arrayLength": 3,
            "description": "boolean fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_boolean_fixed_array",
            "type": "bool",
          },
          {
            "arrayLength": 3,
            "description": "float64 fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_float64_fixed_array",
            "type": "float64",
          },
          {
            "arrayLength": 3,
            "description": "uint32 fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_uint32_fixed_array",
            "type": "uint32",
          },
          {
            "arrayLength": 3,
            "description": "string fixed-length array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_string_fixed_array",
            "type": "string",
          },
          {
            "description": "Value A",
            "isConstant": true,
            "name": "A",
            "type": "uint8",
            "value": 0,
            "valueText": "0",
          },
          {
            "description": "Value B",
            "isConstant": true,
            "name": "B",
            "type": "uint8",
            "value": 1,
            "valueText": "1",
          },
          {
            "arrayLength": undefined,
            "description": "An enum field",
            "isArray": false,
            "isComplex": false,
            "name": "field_enum",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "An enum array field",
            "isArray": true,
            "isComplex": false,
            "name": "field_enum_array",
            "type": "uint8",
          },
          {
            "arrayLength": undefined,
            "description": "A nested field",
            "isArray": false,
            "isComplex": true,
            "name": "field_nested",
            "type": "foxglove_msgs/NestedMessage",
          },
          {
            "arrayLength": undefined,
            "description": "A nested array field
      With
      a
      very
      long
      description",
            "isArray": true,
            "isComplex": true,
            "name": "field_nested_array",
            "type": "foxglove_msgs/NestedMessage",
          },
        ],
        "originalName": "ExampleMessage",
        "rosFullInterfaceName": "foxglove_msgs/msg/ExampleMessage",
        "rosMsgInterfaceName": "foxglove_msgs/ExampleMessage",
      }
    `);
  });
});

describe("generateRosMsg", () => {
  it("generates msg file for ROS 1", () => {
    expect(
      generateRosMsg(
        generateRosMsgDefinition(exampleMessageWithoutArrayOfBytes, {
          rosVersion: 1,
        }),

        { rosVersion: 1 },
      ),
    ).toMatchInlineSnapshot(`
      "# foxglove_msgs/ExampleMessage
      # An example type

      # Generated by https://github.com/foxglove/foxglove-sdk

      # duration field
      duration field_duration

      # time field
      time field_time

      # boolean field
      bool field_boolean

      # bytes field
      uint8[] field_bytes

      # float64 field
      float64 field_float64

      # uint32 field
      uint32 field_uint32

      # string field
      string field_string

      # duration array field
      duration[] field_duration_array

      # time array field
      time[] field_time_array

      # boolean array field
      bool[] field_boolean_array

      # float64 array field
      float64[] field_float64_array

      # uint32 array field
      uint32[] field_uint32_array

      # string array field
      string[] field_string_array

      # duration fixed-length array field
      duration[3] field_duration_fixed_array

      # time fixed-length array field
      time[3] field_time_fixed_array

      # boolean fixed-length array field
      bool[3] field_boolean_fixed_array

      # float64 fixed-length array field
      float64[3] field_float64_fixed_array

      # uint32 fixed-length array field
      uint32[3] field_uint32_fixed_array

      # string fixed-length array field
      string[3] field_string_fixed_array

      # Value A
      uint8 A=0

      # Value B
      uint8 B=1

      # An enum field
      uint8 field_enum

      # An enum array field
      uint8[] field_enum_array

      # A nested field
      foxglove_msgs/NestedMessage field_nested

      # A nested array field
      # With
      # a
      # very
      # long
      # description
      foxglove_msgs/NestedMessage[] field_nested_array
      "
    `);
  });

  it("generates msg file for ROS 2", () => {
    expect(
      generateRosMsg(
        generateRosMsgDefinition(exampleMessageWithoutArrayOfBytes, {
          rosVersion: 2,
        }),

        { rosVersion: 2 },
      ),
    ).toMatchInlineSnapshot(`
      "# foxglove_msgs/msg/ExampleMessage
      # An example type

      # Generated by https://github.com/foxglove/foxglove-sdk

      # duration field
      builtin_interfaces/Duration field_duration

      # time field
      builtin_interfaces/Time field_time

      # boolean field
      bool field_boolean

      # bytes field
      uint8[] field_bytes

      # float64 field
      float64 field_float64

      # uint32 field
      uint32 field_uint32

      # string field
      string field_string

      # duration array field
      builtin_interfaces/Duration[] field_duration_array

      # time array field
      builtin_interfaces/Time[] field_time_array

      # boolean array field
      bool[] field_boolean_array

      # float64 array field
      float64[] field_float64_array

      # uint32 array field
      uint32[] field_uint32_array

      # string array field
      string[] field_string_array

      # duration fixed-length array field
      builtin_interfaces/Duration[3] field_duration_fixed_array

      # time fixed-length array field
      builtin_interfaces/Time[3] field_time_fixed_array

      # boolean fixed-length array field
      bool[3] field_boolean_fixed_array

      # float64 fixed-length array field
      float64[3] field_float64_fixed_array

      # uint32 fixed-length array field
      uint32[3] field_uint32_fixed_array

      # string fixed-length array field
      string[3] field_string_fixed_array

      # Value A
      uint8 A=0

      # Value B
      uint8 B=1

      # An enum field
      uint8 field_enum

      # An enum array field
      uint8[] field_enum_array

      # A nested field
      foxglove_msgs/NestedMessage field_nested

      # A nested array field
      # With
      # a
      # very
      # long
      # description
      foxglove_msgs/NestedMessage[] field_nested_array
      "
    `);
  });
});

describe("generateRosMsgMergedSchema", () => {
  it("generates merged schema for ROS 1", () => {
    const mergedSchema = generateRosMsgMergedSchema(exampleMessageWithoutArrayOfBytes, {
      rosVersion: 1,
    });
    expect(mergedSchema).toMatchInlineSnapshot(`
      "# foxglove_msgs/ExampleMessage
      # An example type

      # Generated by https://github.com/foxglove/foxglove-sdk

      # duration field
      duration field_duration

      # time field
      time field_time

      # boolean field
      bool field_boolean

      # bytes field
      uint8[] field_bytes

      # float64 field
      float64 field_float64

      # uint32 field
      uint32 field_uint32

      # string field
      string field_string

      # duration array field
      duration[] field_duration_array

      # time array field
      time[] field_time_array

      # boolean array field
      bool[] field_boolean_array

      # float64 array field
      float64[] field_float64_array

      # uint32 array field
      uint32[] field_uint32_array

      # string array field
      string[] field_string_array

      # duration fixed-length array field
      duration[3] field_duration_fixed_array

      # time fixed-length array field
      time[3] field_time_fixed_array

      # boolean fixed-length array field
      bool[3] field_boolean_fixed_array

      # float64 fixed-length array field
      float64[3] field_float64_fixed_array

      # uint32 fixed-length array field
      uint32[3] field_uint32_fixed_array

      # string fixed-length array field
      string[3] field_string_fixed_array

      # Value A
      uint8 A=0

      # Value B
      uint8 B=1

      # An enum field
      uint8 field_enum

      # An enum array field
      uint8[] field_enum_array

      # A nested field
      foxglove_msgs/NestedMessage field_nested

      # A nested array field
      # With
      # a
      # very
      # long
      # description
      foxglove_msgs/NestedMessage[] field_nested_array
      ================================================================================
      MSG: foxglove_msgs/NestedMessage
      # foxglove_msgs/NestedMessage
      # An example nested message

      # Generated by https://github.com/foxglove/foxglove-sdk

      # An enum field
      uint32 field_enum
      "
    `);
    expect(parseMessageDefinition(mergedSchema)).toMatchInlineSnapshot(`
      [
        {
          "definitions": [
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_duration",
              "type": "duration",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_time",
              "type": "time",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_boolean",
              "type": "bool",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_bytes",
              "type": "uint8",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_float64",
              "type": "float64",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_uint32",
              "type": "uint32",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_string",
              "type": "string",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_duration_array",
              "type": "duration",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_time_array",
              "type": "time",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_boolean_array",
              "type": "bool",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_float64_array",
              "type": "float64",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_uint32_array",
              "type": "uint32",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_string_array",
              "type": "string",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_duration_fixed_array",
              "type": "duration",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_time_fixed_array",
              "type": "time",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_boolean_fixed_array",
              "type": "bool",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_float64_fixed_array",
              "type": "float64",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_uint32_fixed_array",
              "type": "uint32",
            },
            {
              "arrayLength": 3,
              "isArray": true,
              "isComplex": false,
              "name": "field_string_fixed_array",
              "type": "string",
            },
            {
              "isConstant": true,
              "name": "A",
              "type": "uint8",
              "value": 0,
              "valueText": "0",
            },
            {
              "isConstant": true,
              "name": "B",
              "type": "uint8",
              "value": 1,
              "valueText": "1",
            },
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_enum",
              "type": "uint8",
            },
            {
              "isArray": true,
              "isComplex": false,
              "name": "field_enum_array",
              "type": "uint8",
            },
            {
              "isArray": false,
              "isComplex": true,
              "name": "field_nested",
              "type": "foxglove_msgs/NestedMessage",
            },
            {
              "isArray": true,
              "isComplex": true,
              "name": "field_nested_array",
              "type": "foxglove_msgs/NestedMessage",
            },
          ],
          "name": undefined,
        },
        {
          "definitions": [
            {
              "isArray": false,
              "isComplex": false,
              "name": "field_enum",
              "type": "uint32",
            },
          ],
          "name": "foxglove_msgs/NestedMessage",
        },
      ]
    `);
  });

  it("generates merged schema for ROS 2", () => {
    const mergedSchema = generateRosMsgMergedSchema(exampleMessageWithoutArrayOfBytes, {
      rosVersion: 2,
    });
    expect(mergedSchema).toMatchInlineSnapshot(`
      "# foxglove_msgs/msg/ExampleMessage
      # An example type

      # Generated by https://github.com/foxglove/foxglove-sdk

      # duration field
      builtin_interfaces/Duration field_duration

      # time field
      builtin_interfaces/Time field_time

      # boolean field
      bool field_boolean

      # bytes field
      uint8[] field_bytes

      # float64 field
      float64 field_float64

      # uint32 field
      uint32 field_uint32

      # string field
      string field_string

      # duration array field
      builtin_interfaces/Duration[] field_duration_array

      # time array field
      builtin_interfaces/Time[] field_time_array

      # boolean array field
      bool[] field_boolean_array

      # float64 array field
      float64[] field_float64_array

      # uint32 array field
      uint32[] field_uint32_array

      # string array field
      string[] field_string_array

      # duration fixed-length array field
      builtin_interfaces/Duration[3] field_duration_fixed_array

      # time fixed-length array field
      builtin_interfaces/Time[3] field_time_fixed_array

      # boolean fixed-length array field
      bool[3] field_boolean_fixed_array

      # float64 fixed-length array field
      float64[3] field_float64_fixed_array

      # uint32 fixed-length array field
      uint32[3] field_uint32_fixed_array

      # string fixed-length array field
      string[3] field_string_fixed_array

      # Value A
      uint8 A=0

      # Value B
      uint8 B=1

      # An enum field
      uint8 field_enum

      # An enum array field
      uint8[] field_enum_array

      # A nested field
      foxglove_msgs/NestedMessage field_nested

      # A nested array field
      # With
      # a
      # very
      # long
      # description
      foxglove_msgs/NestedMessage[] field_nested_array
      ================================================================================
      MSG: foxglove_msgs/NestedMessage
      # foxglove_msgs/msg/NestedMessage
      # An example nested message

      # Generated by https://github.com/foxglove/foxglove-sdk

      # An enum field
      uint32 field_enum
      "
    `);
    expect(parseMessageDefinition(mergedSchema, { ros2: true })).toMatchInlineSnapshot(`
      [
        {
          "definitions": [
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_duration",
              "type": "duration",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_time",
              "type": "time",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_boolean",
              "type": "bool",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_bytes",
              "type": "uint8",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_float64",
              "type": "float64",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_uint32",
              "type": "uint32",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_string",
              "type": "string",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_duration_array",
              "type": "duration",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_time_array",
              "type": "time",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_boolean_array",
              "type": "bool",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_float64_array",
              "type": "float64",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_uint32_array",
              "type": "uint32",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_string_array",
              "type": "string",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_duration_fixed_array",
              "type": "duration",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_time_fixed_array",
              "type": "time",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_boolean_fixed_array",
              "type": "bool",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_float64_fixed_array",
              "type": "float64",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_uint32_fixed_array",
              "type": "uint32",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": 3,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_string_fixed_array",
              "type": "string",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": undefined,
              "isComplex": undefined,
              "isConstant": true,
              "name": "A",
              "type": "uint8",
              "upperBound": undefined,
              "value": 0,
              "valueText": "0",
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": undefined,
              "isComplex": undefined,
              "isConstant": true,
              "name": "B",
              "type": "uint8",
              "upperBound": undefined,
              "value": 1,
              "valueText": "1",
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_enum",
              "type": "uint8",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_enum_array",
              "type": "uint8",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": true,
              "isConstant": undefined,
              "name": "field_nested",
              "type": "foxglove_msgs/NestedMessage",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": true,
              "isComplex": true,
              "isConstant": undefined,
              "name": "field_nested_array",
              "type": "foxglove_msgs/NestedMessage",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
          ],
          "name": undefined,
        },
        {
          "definitions": [
            {
              "arrayLength": undefined,
              "arrayUpperBound": undefined,
              "defaultValue": undefined,
              "isArray": false,
              "isComplex": false,
              "isConstant": undefined,
              "name": "field_enum",
              "type": "uint32",
              "upperBound": undefined,
              "value": undefined,
              "valueText": undefined,
            },
          ],
          "name": "foxglove_msgs/NestedMessage",
        },
      ]
    `);
  });

  it.each(Object.values(foxgloveMessageSchemas))("generates parseable merged schemas", (schema) => {
    expect(() =>
      parseMessageDefinition(generateRosMsgMergedSchema(schema, { rosVersion: 1 })),
    ).not.toThrow();

    expect(() =>
      parseMessageDefinition(generateRosMsgMergedSchema(schema, { rosVersion: 2 }), { ros2: true }),
    ).not.toThrow();
  });
});
