import assert from "assert";
import { FoxgloveEnumSchema, FoxgloveMessageSchema, FoxglovePrimitive } from "./types";

function primitiveToRust(type: FoxglovePrimitive) {
  switch (type) {
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
  return (id === "type") ? `r#${id}` : id;
}

function toSnakeCase(name: string) {
  name = name.replace(/([A-Z])/g, "_$1").toLowerCase();
  return name.startsWith("_") ? name.substring(1) : name;
}

function toTitleCase(name: string) {
  return name.toLowerCase().replace(/(?:^|_)([a-z])/g, (_, letter) => letter.toUpperCase());
}

export function generateRustTypes(schemas: readonly FoxgloveMessageSchema[], enums: readonly FoxgloveEnumSchema[]): string {
  // A schema needs a custom free function if it has a Vec<NestedSchema> field,
  // because we had to allocate the vector to map the C NestedSchema type to the Rust NestedSchema type,
  // as the two don't have the same layout, so we can't just re-use the C array.
  const needsFree = new Set(schemas.map((schema) => {
    const needsFree = schema.fields.some((field) => field.array !== undefined && typeof field.array !== "number" && field.type.type === "nested");
    return needsFree ? schema.name : "";
  }).filter((name) => name.length > 0));

  const schemaStructs = schemas.map(
    (schema) => {
      const { fields, description } = schema;
      const name = schema.name.replace("JSON", "Json");
      const snakeName = toSnakeCase(name);
      return`\
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
            fieldType = "*const Timestamp";
          } else if (field.type.name === "duration") {
            fieldType = "*const Duration";
          } else {
            fieldType = primitiveToRust(field.type.name);
          }
          break;
        case "enum":
          fieldType = `Foxglove${field.type.enum.name}`;
          break;
        case "nested":
          fieldType = `*const ${field.type.schema.name.replace("JSON", "Json")}`;
          break;
      }
      let lines: string[] = [comment];
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
${name.endsWith("Primitive") ? "" : `
  unsafe fn borrow_option_to_native(msg: Option<&Self>) -> Result<ManuallyDrop<foxglove::schemas::${name}>, foxglove::FoxgloveError> {
    let Some(msg) = msg else {
      return Err(foxglove::FoxgloveError::ValueError("msg is required".to_string()));
    };
    unsafe { msg.borrow_to_native() }
  }
`}

  unsafe fn borrow_to_native(&self) -> Result<ManuallyDrop<foxglove::schemas::${name}>, foxglove::FoxgloveError> {
    Ok(ManuallyDrop::new(foxglove::schemas::${name} {
    ${fields
      .map((field) => {
        const srcName = escapeId(toSnakeCase(field.name));
        const dstName = escapeId(toSnakeCase(field.name));
        if (field.array !== undefined) {
          if (typeof field.array === "number") {
            return `${dstName}: unsafe { Vec::from_raw_parts(self.${srcName}.as_ptr() as *mut ${(field.type.type === "primitive") ? primitiveToRust(field.type.name) : "todo!()"}, self.${srcName}.len(), self.${srcName}.len()) }`;
          } else {
            if (field.type.type === "nested") {
              return `${dstName}: unsafe { std::slice::from_raw_parts(self.${srcName}, self.${srcName}_count) }
                .iter().filter_map(|m| unsafe { m.as_ref().map(|m| m.borrow_to_native().map(ManuallyDrop::into_inner)) }).collect::<Result<Vec<_>, _>>()?`;
            } else if (field.type.type === "primitive") {
              assert(field.type.name !== "bytes");
              return `${dstName}: unsafe { Vec::from_raw_parts(self.${srcName} as *mut ${primitiveToRust(field.type.name)}, self.${srcName}_count, self.${srcName}_count) }`;
            } else {
              return `${dstName}: todo!()`;
            }
          }
        }
        switch (field.type.type) {
          case "primitive":
            if (field.type.name === "string") {
              return `${dstName}: unsafe { String::from_utf8(Vec::from_raw_parts(self.${srcName}.as_ptr() as *mut _, self.${srcName}.len(), self.${srcName}.len())) }
                .map_err(|e| foxglove::FoxgloveError::Utf8Error(format!("${srcName} invalid: {}", e)))?`;
            } else if (field.type.name === "bytes") {
              return `${dstName}: unsafe { Bytes::from_static(std::slice::from_raw_parts(self.${srcName}, self.${srcName}_len)) }`;
            } else if (field.type.name === "time" || field.type.name === "duration") {
              return `${dstName}: unsafe { self.${srcName}.as_ref() }.map(|&m| m.into())`;
            }
            return `${dstName}: self.${srcName}`;
          case "enum":
            return `${dstName}: self.${srcName} as i32`;
          case "nested":
            return `${dstName}: unsafe { self.${srcName}.as_ref().map(|m| m.borrow_to_native()) }.transpose()?.map(ManuallyDrop::into_inner)`;
        }
      })
      .join(",      \n")}
    }))
  }
}

${name.endsWith("Primitive") ? "" : `
#[unsafe(no_mangle)]
pub extern "C" fn foxglove_channel_log_${snakeName}(channel: Option<&FoxgloveChannel>, msg: Option<&${name}>, log_time: Option<&u64>) -> FoxgloveError {
  // Safety: we're borrowing from the msg, but discard the borrowed message before returning
  match unsafe { ${name}::borrow_option_to_native(msg) } {
    Ok(msg) => {
      ${needsFree.has(name) ? `let e = log_msg_to_channel(channel, &*msg, log_time);
      free_${snakeName}(msg);
      e` : "log_msg_to_channel(channel, &*msg, log_time)"}
    },
    Err(e) => {
      tracing::error!("${name}: {}", e);
      e.into()
    }
  }
}
`}

${needsFree.has(schema.name) ? `
#[allow(forgetting_copy_types)]
fn free_${snakeName}(mut msg: ManuallyDrop<foxglove::schemas::${name}>) {
    // The only allocations we made were for Vec<Nested> fields, which may also include Vec<Nested> fields in a couple cases.
    ${fields
      .map((field) => {
        const name = escapeId(toSnakeCase(field.name));
        if (field.array !== undefined) {
          if (typeof field.array !== "number" && field.type.type === "nested") {
              const nestedName = escapeId(toSnakeCase(field.type.schema.name));
              return `    for nested in std::mem::take(&mut msg.${name}) {
        ${needsFree.has(field.type.schema.name) ?
        `free_${nestedName}(ManuallyDrop::new(nested));` :
        `std::mem::forget(nested);`
}}`;
          }
        }
        return "";
      })
      .filter((line) => line.length > 0)
      .join("\n")}
}` : ""}
`},
  );

  const imports = [
    "use std::ffi::c_uchar;",
    "use foxglove::bytes::Bytes;",
    "use std::mem::ManuallyDrop;",
    "",
    "use crate::{FoxgloveString, FoxgloveError, FoxgloveChannel, Timestamp, Duration, log_msg_to_channel};"
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
