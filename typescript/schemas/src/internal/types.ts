export type FoxglovePrimitive = "string" | "float64" | "int32" | "uint32" | "boolean" | "bytes";

export type FoxgloveEnumSchema = {
  type: "enum";
  name: string;
  description: string;
  parentSchemaName: string;
  protobufEnumName: string;
  values: ReadonlyArray<{
    value: number;
    name: string;
    description?: string;
    /**
     * Omit this value's constant from ROS 1 .msg files and document it in the enum field's
     * comment instead. ROS 1 includes constants in a message's MD5, so adding a constant to a
     * published message stops it from interoperating with earlier builds and recorded bags.
     */
    omitRos1Constant?: boolean;
  }>;
};

export type FoxgloveMessageField = {
  name: string;
  type:
    | { type: "primitive"; name: FoxglovePrimitive }
    | { type: "nested"; schema: FoxgloveMessageSchema }
    | { type: "enum"; enum: FoxgloveEnumSchema };
  array?: true | number;
  description: string;
  protobufFieldNumber?: number;
  flatbuffersFieldNumber?: number;
  defaultValue?: string | number | boolean;
  /** Required by default. Set to true for optional fields. IDLs without optional (e.g. ROS 1) always require a value. */
  optional?: true;
};

// Flatbuffers and OMG IDL use "Time" instead of "Timestamp" for backwards compatibility.
export type FoxgloveMessageSchema = {
  type: "message";
  name: string;
  description: string;
  rosEquivalent?: keyof typeof import("@foxglove/rosmsg-msgs-common").ros1;
  ros2Equivalent?: keyof typeof import("@foxglove/rosmsg-msgs-common").ros2jazzy;
  protoEquivalent?: "google.protobuf.Timestamp" | "google.protobuf.Duration";
  fields: ReadonlyArray<FoxgloveMessageField>;
};

export type FoxgloveSchema = FoxgloveMessageSchema | FoxgloveEnumSchema;
