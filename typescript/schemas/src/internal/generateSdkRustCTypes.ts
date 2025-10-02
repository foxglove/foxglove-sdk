import assert from "assert";
import fs from "node:fs/promises";
import { parse as parseToml, stringify as stringifyToml, TomlTable } from "smol-toml";

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

function rustEnumName(name: string) {
  return `Foxglove${name}`;
}

/**
 * Identical to the name, except:
 * - GeoJSON is renamed to GeoJson in all lanaguages
 * - Timestamp and Duration use existing FoxgloveTimestamp and FoxgloveDuration implementations
 */
function rustMessageSchemaName(schema: FoxgloveMessageSchema): string {
  if (schema.name === "Timestamp") {
    return "FoxgloveTimestamp";
  } else if (schema.name === "Duration") {
    return "FoxgloveDuration";
  } else {
    return schema.name.replace("JSON", "Json");
  }
}

export function generateRustTypes(
  schemas: readonly FoxgloveMessageSchema[],
  enums: readonly FoxgloveEnumSchema[],
): string {
  const schemaStructs = schemas.filter(shouldGenerateRustType).map((schema) => {
    const { fields, description } = schema;
    const name = rustMessageSchemaName(schema);
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
          } else {
            fieldType = primitiveToRust(field.type.name);
          }
          break;
        case "enum":
          fieldType = rustEnumName(field.type.enum.name);
          break;
        case "nested":
          fieldType = rustMessageSchemaName(field.type.schema);
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

#[cfg(not(target_family = "wasm"))]
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
            if (field.type.schema.name === "Timestamp" || field.type.schema.name === "Duration") {
              return [];
            }
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
            }
            return `${fieldName}: self.${fieldName}`;
          case "enum":
            return `${fieldName}: self.${fieldName} as i32`;
          case "nested":
            if (field.type.schema.name === "Timestamp" || field.type.schema.name === "Duration") {
              return `${fieldName}: unsafe { self.${fieldName}.as_ref() }.map(|&m| m.into())`;
            }
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
#[cfg(not(target_family = "wasm"))]
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_log_${snakeName}(channel: Option<&FoxgloveChannel>, msg: Option<&${name}>, log_time: Option<&u64>, sink_id: FoxgloveSinkId) -> FoxgloveError {
  let mut arena = pin!(Arena::new());
  let arena_pin = arena.as_mut();
  // Safety: we're borrowing from the msg, but discard the borrowed message before returning
  match unsafe { ${name}::borrow_option_to_native(msg, arena_pin) } {
    Ok(msg) => {
      // Safety: this casts channel back to a typed channel for type of msg, it must have been created for this type.
      log_msg_to_channel(channel, &*msg, log_time, sink_id)
    },
    Err(e) => {
      tracing::error!("${name}: {}", e);
      e.into()
    }
  }
}

/// Get the ${name} schema.
///
/// All buffers in the returned schema are statically allocated.
#[allow(clippy::missing_safety_doc, reason="no preconditions and returned lifetime is static")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_${snakeName}_schema() -> FoxgloveSchema {
    let native = foxglove::schemas::${name}::get_schema().expect("${name} schema is Some");
    let name: &'static str = "foxglove.${schema.name}";
    let encoding: &'static str = "protobuf";
    assert_eq!(name, &native.name);
    assert_eq!(encoding, &native.encoding);
    let std::borrow::Cow::Borrowed(data) = native.data else {
      unreachable!("${name} schema data is static");
    };
    FoxgloveSchema {
      name: name.into(),
      encoding: encoding.into(),
      data: data.as_ptr(),
      data_len: data.len(),
    }
}

/// Encode a ${name} message as protobuf to the buffer provided.
///
/// On success, writes the encoded length to *encoded_len.
/// If the provided buffer has insufficient capacity, writes the required capacity to *encoded_len and
/// returns FOXGLOVE_ERROR_BUFFER_TOO_SHORT.
/// If the message cannot be encoded, logs the reason to stderr and returns FOXGLOVE_ERROR_ENCODE.
///
/// # Safety
/// ptr must be a valid pointer to a memory region at least len bytes long.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn foxglove_${snakeName}_encode(
    msg: Option<&${name}>,
    ptr: *mut u8,
    len: usize,
    encoded_len: Option<&mut usize>,
) -> FoxgloveError {
    let mut arena = pin!(Arena::new());
    let arena_pin = arena.as_mut();
    // Safety: we're borrowing from the msg, but discard the borrowed message before returning
    match unsafe { ${name}::borrow_option_to_native(msg, arena_pin) } {
        Ok(msg) => {
            if len == 0 || ptr.is_null() {
                if let Some(encoded_len) = encoded_len {
                    *encoded_len = msg.encoded_len().expect("foxglove schemas return Some(len)");
                }
                return FoxgloveError::BufferTooShort;
            }
            let mut buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
            if let Err(encode_error) = msg.encode(&mut buf) {
                if let Some(encoded_len) = encoded_len {
                    *encoded_len = encode_error.required_capacity();
                }
                return FoxgloveError::BufferTooShort;
            }
            if let Some(encoded_len) = encoded_len {
                *encoded_len = len - buf.len();
            }
            FoxgloveError::Ok
        }
        Err(e) => {
            tracing::error!("${name}: {}", e);
            FoxgloveError::EncodeError
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
    "use foxglove::Encode;",
    "",
    "use crate::{FoxgloveSchema, FoxgloveString, FoxgloveError, FoxgloveTimestamp, FoxgloveDuration};",
    `#[cfg(not(target_family = "wasm"))]`,
    "use crate::{FoxgloveChannel, FoxgloveContext, log_msg_to_channel, result_to_c, do_foxglove_channel_create, FoxgloveSinkId};",
    "use crate::arena::{Arena, BorrowToNative};",
    "use crate::util::{bytes_from_raw, string_from_raw, vec_from_raw};",
  ];

  const enumDefs = enums.map((enumSchema) => {
    return `
    #[derive(Clone, Copy, Debug)]
    #[repr(i32)]
    pub enum ${rustEnumName(enumSchema.name)} {
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

function assertValidBindgen(
  bindgen: TomlTable,
): asserts bindgen is { export: { rename: TomlTable } } {
  if (
    typeof bindgen.export !== "object" ||
    !("rename" in bindgen.export) ||
    typeof bindgen.export.rename !== "object"
  ) {
    throw new Error("Invalid bindgen.toml file (export.rename definitions missing)");
  }
}

/**
 * Builds the content of the cbindgen.toml config file, based on a manually-written prelude
 * and export renames for generated schema types.
 */
export async function generateBindgenConfig(
  schemas: readonly FoxgloveMessageSchema[],
  enums: readonly FoxgloveEnumSchema[],
  preludeFile: string,
): Promise<string> {
  const comment = `
#
# Generated by https://github.com/foxglove/foxglove-sdk
#
# Do not edit this file directly. Edit the prelude file instead.
#
`;
  const prelude = await fs.readFile(preludeFile, "utf-8");

  const bindgenToml = parseToml(prelude);
  assertValidBindgen(bindgenToml);

  schemas.forEach((schema) => {
    const sourceName = rustMessageSchemaName(schema);
    const prefix = sourceName.startsWith("Foxglove") ? "" : "foxglove_";
    const exportName = `${prefix}${toSnakeCase(sourceName)}`;
    if (sourceName in bindgenToml.export.rename) {
      throw new Error(`Duplicate name in rename section: ${sourceName}`);
    }
    bindgenToml.export.rename[sourceName] = exportName;
  });

  enums.forEach((enumSchema) => {
    const sourceName = rustEnumName(enumSchema.name);
    const exportName = toSnakeCase(sourceName);
    if (sourceName in bindgenToml.export.rename) {
      throw new Error(`Duplicate name in rename section: ${sourceName}`);
    }
    bindgenToml.export.rename[sourceName] = exportName;
  });

  return comment + "\n" + stringifyToml(bindgenToml);
}
