import assert from "assert";

import { FoxgloveEnumSchema, FoxgloveMessageSchema, FoxglovePrimitive } from "./types";

function primitiveToRust(type: FoxglovePrimitive) {
  switch (type) {
    case "int32":
      return "i32";
    case "uint32":
      return "u32";
    case "boolean":
      return "bool";
    case "float64":
      return "f64";
    case "time":
      return "Timestamp";
    case "duration":
      return "Duration";
    case "string":
      return "FoxgloveString";
    case "bytes":
      assert(false, "bytes not supported by primitiveToRust");
  }
}

function formatComment(comment: string) {
  return comment
    .split("\n")
    .map((line) => `/// ${line}`)
    .join("\n");
}

function escapeId(id: string) {
  return id === "type" ? `r#${id}` : id;
}

function toSnakeCase(name: string) {
  const snakeName = name.replace(/([A-Z])/g, "_$1").toLowerCase();
  return snakeName.startsWith("_") ? snakeName.substring(1) : snakeName;
}

function toTitleCase(name: string) {
  return name.toLowerCase().replace(/(?:^|_)([a-z])/g, (_, letter: string) => letter.toUpperCase());
}

// We use special FoxgloveTimestamp and FoxgloveDuration types for the time and duration fields.
function shouldGenerateRustType(schema: FoxgloveMessageSchema): boolean {
  return schema.name !== "Timestamp" && schema.name !== "Duration";
}

export function generateRustTypes(
  schemas: readonly FoxgloveMessageSchema[],
  enums: readonly FoxgloveEnumSchema[],
): string {
  const schemaStructs = schemas.filter(shouldGenerateRustType).map((schema) => {
    const { fields, description } = schema;
    const name = schema.name.replace("JSON", "Json");
    const snakeName = toSnakeCase(name);
    return `\
${formatComment(description)}
#[repr(C)]
pub struct ${name} {
  ${fields
    .flatMap((field) => {
      const comment = formatComment(field.description);
      const identName = escapeId(toSnakeCase(field.name));
      let fieldType: string;
      let fieldHasLen = false;
      switch (field.type.type) {
        case "primitive":
          if (field.type.name === "bytes") {
            fieldType = "*const c_uchar";
            fieldHasLen = true;
          } else if (field.type.name === "time") {
            fieldType = "*const FoxgloveTimestamp";
          } else if (field.type.name === "duration") {
            fieldType = "*const FoxgloveDuration";
          } else {
            fieldType = primitiveToRust(field.type.name);
          }
          break;
        case "enum":
          fieldType = `Foxglove${field.type.enum.name}`;
          break;
        case "nested":
          fieldType = field.type.schema.name.replace("JSON", "Json");
          break;
      }
      const lines: string[] = [comment];
      if (typeof field.array === "number") {
        lines.push(`pub ${identName}: [${fieldType}; ${field.array}],`);
        if (fieldHasLen) {
          lines.push(`pub ${identName}_len: [usize; ${field.array}],`);
        }
      } else if (field.array === true) {
        lines.push(`pub ${identName}: *const ${fieldType},`);
        if (fieldHasLen) {
          lines.push(`pub ${identName}_len: *const usize,`);
        }
        lines.push(`pub ${identName}_count: usize,`);
      } else {
        if (field.type.type === "nested") {
          fieldType = `*const ${fieldType}`;
        }
        lines.push(`pub ${identName}: ${fieldType},`);
        if (fieldHasLen) {
          lines.push(`pub ${identName}_len: usize,`);
        }
      }
      return lines.join("\n");
    })
    .join("\n\n")}
}

impl ${name} {
  /// Create a new typed channel, and return an owned raw channel pointer to it.
  ///
  /// # Safety
  /// We're trusting the caller that the channel will only be used with this type T.
  #[unsafe(no_mangle)]
  pub unsafe extern "C" fn foxglove_channel_create_${snakeName}(
      topic: FoxgloveString,
      context: *const FoxgloveContext,
      channel: *mut *const FoxgloveChannel,
  ) -> FoxgloveError {
      if channel.is_null() {
          tracing::error!("channel cannot be null");
          return FoxgloveError::ValueError;
      }
      unsafe {
          let result = do_foxglove_channel_create::<foxglove::schemas::${name}>(topic, context);
          result_to_c(result, channel)
      }
  }
}

impl BorrowToNative for ${name} {
  type NativeType = foxglove::schemas::${name};

  unsafe fn borrow_to_native(&self, #[allow(unused_mut, unused_variables)] mut arena: Pin<&mut Arena>) -> Result<ManuallyDrop<Self::NativeType>, foxglove::FoxgloveError> {
    ${fields
      .flatMap((field) => {
        const fieldName = escapeId(toSnakeCase(field.name));
        if (
          field.array != undefined &&
          typeof field.array !== "number" &&
          field.type.type === "nested"
        ) {
          return [
            `let ${fieldName} = unsafe { arena.as_mut().map(self.${fieldName}, self.${fieldName}_count)? };`,
          ];
        }
        switch (field.type.type) {
          case "primitive":
            if (field.type.name === "string") {
              return [
                `let ${fieldName} = unsafe { string_from_raw(self.${fieldName}.as_ptr() as *const _, self.${fieldName}.len(), "${field.name}")? };`,
              ];
            }
            return [];
          case "nested":
            return [
              `let ${fieldName} = unsafe { self.${fieldName}.as_ref().map(|m| m.borrow_to_native(arena.as_mut())) }.transpose()?;`,
            ];
          case "enum":
            return [];
        }
      })
      .join("\n    ")}

    Ok(ManuallyDrop::new(foxglove::schemas::${name} {
    ${fields
      .map((field) => {
        const fieldName = escapeId(toSnakeCase(field.name));
        if (field.array != undefined) {
          if (typeof field.array === "number") {
            assert(field.type.type === "primitive", `unsupported array type: ${field.type.type}`);
            return `${fieldName}: ManuallyDrop::into_inner(unsafe { vec_from_raw(self.${fieldName}.as_ptr() as *mut ${primitiveToRust(field.type.name)}, self.${fieldName}.len()) })`;
          } else {
            if (field.type.type === "nested") {
              return `${fieldName}: ManuallyDrop::into_inner(${fieldName})`;
            } else if (field.type.type === "primitive") {
              assert(field.type.name !== "bytes");
              return `${fieldName}: ManuallyDrop::into_inner(unsafe { vec_from_raw(self.${fieldName} as *mut ${primitiveToRust(field.type.name)}, self.${fieldName}_count) })`;
            } else {
              throw Error(`unsupported array type: ${field.type.type}`);
            }
          }
        }
        switch (field.type.type) {
          case "primitive":
            if (field.type.name === "string") {
              return `${fieldName}: ManuallyDrop::into_inner(${fieldName})`;
            } else if (field.type.name === "bytes") {
              return `${fieldName}: ManuallyDrop::into_inner(unsafe { bytes_from_raw(self.${fieldName}, self.${fieldName}_len) })`;
            } else if (field.type.name === "time" || field.type.name === "duration") {
              return `${fieldName}: unsafe { self.${fieldName}.as_ref() }.map(|&m| m.into())`;
            }
            return `${fieldName}: self.${fieldName}`;
          case "enum":
            return `${fieldName}: self.${fieldName} as i32`;
          case "nested":
            return `${fieldName}: ${fieldName}.map(ManuallyDrop::into_inner)`;
        }
      })
      .join(",\n        ")}
    }))
  }
}

/// Log a ${name} message to a channel.
///
/// # Safety
/// The channel must have been created for this type with foxglove_channel_create_${snakeName}.
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_log_${snakeName}(channel: Option<&FoxgloveChannel>, msg: Option<&${name}>, log_time: Option<&u64>) -> FoxgloveError {
  let mut arena = pin!(Arena::new());
  let arena_pin = arena.as_mut();
  // Safety: we're borrowing from the msg, but discard the borrowed message before returning
  match unsafe { ${name}::borrow_option_to_native(msg, arena_pin) } {
    Ok(msg) => {
      // Safety: this casts channel back to a typed channel for type of msg, it must have been created for this type.
      log_msg_to_channel(channel, &*msg, log_time)
    },
    Err(e) => {
      tracing::error!("${name}: {}", e);
      e.into()
    }
  }
}
`;
  });

  const imports = [
    "use std::ffi::c_uchar;",
    "use std::mem::ManuallyDrop;",
    "use std::pin::{pin, Pin};",
    "",
    "use crate::{FoxgloveString, FoxgloveError, FoxgloveChannel, FoxgloveContext, FoxgloveTimestamp, FoxgloveDuration, log_msg_to_channel, result_to_c, do_foxglove_channel_create};",
    "use crate::arena::{Arena, BorrowToNative};",
    "use crate::util::{bytes_from_raw, string_from_raw, vec_from_raw};",
  ];

  const enumDefs = enums.map((enumSchema) => {
    return `
    #[derive(Clone, Copy, Debug)]
    #[repr(i32)]
    pub enum Foxglove${enumSchema.name} {
      ${enumSchema.values.map((value) => `${toTitleCase(value.name)} = ${value.value},`).join("\n")}
    }`;
  });

  const outputSections = [
    "// Generated by https://github.com/foxglove/foxglove-sdk",

    imports.join("\n"),

    enumDefs.join("\n"),

    ...schemaStructs,
    "",
  ];

  return outputSections.join("\n\n");
}
