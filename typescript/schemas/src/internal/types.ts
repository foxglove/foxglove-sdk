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
  }>;
};

export type FoxgloveMessageField = {
  name: string;
  type:
    | { type: "primitive"; name: FoxglovePrimitive }
    | { type: "nested"; schema: FoxgloveMessageSchema }
    | { type: "enum"; enum: FoxgloveEnumSchema };
  array?: true | number;
  required?: true;
  description: string;
  protobufFieldNumber?: number;
  defaultValue?: string | number | boolean;
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
