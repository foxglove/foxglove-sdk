use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, rename},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use itertools::Itertools;
use prost::Message;
use prost_types::field_descriptor_proto::Type;
use prost_types::{DescriptorProto, FieldDescriptorProto, FileDescriptorProto, FileDescriptorSet};
use regex::Regex;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

const DOC_REF: &str = "<https://docs.foxglove.dev/docs/visualization/message-schemas/introduction>";

/// Recursively builds a file descriptor set for a file descriptor and its dependencies.
fn build_fds(
    fd: &FileDescriptorProto,
    fd_map: &HashMap<String, &FileDescriptorProto>,
) -> FileDescriptorSet {
    let mut fds = FileDescriptorSet::default();
    let mut seen: HashSet<String> = HashSet::new();
    build_fds_inner(fd, fd_map, &mut fds, &mut seen);
    fds
}

/// Recursive step for `build_fds`.
fn build_fds_inner(
    fd: &FileDescriptorProto,
    fd_map: &HashMap<String, &FileDescriptorProto>,
    fds: &mut FileDescriptorSet,
    seen: &mut HashSet<String>,
) {
    let mut dependencies = fd.dependency.iter().map(|d| d.as_str()).collect::<Vec<_>>();
    dependencies.sort_unstable();
    for name in dependencies {
        if seen.insert(name.to_string())
            && let Some(dep_fd) = fd_map.get(name)
        {
            build_fds_inner(dep_fd, fd_map, fds, seen);
        }
    }
    fds.file.push(FileDescriptorProto {
        source_code_info: None,
        ..fd.clone()
    });
}

/// Helper function to convert `CamelCase` to `CONSTANT_CASE`.
fn camel_case_to_constant_case(camel: &str) -> String {
    let mut output = String::new();
    let mut first = true;
    let mut upper_count = 0;
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            if !first && upper_count == 0 {
                output.push('_');
            }
            output.push(c);
            upper_count += 1;
        } else {
            if upper_count > 1 {
                output.insert(output.len() - 1, '_');
            }
            output.extend(c.to_uppercase());
            upper_count = 0;
        }
        first = false;
    }
    output
}

/// Helper function to convert `CamelCase` to `kebab-case`.
fn camel_case_to_kebab_case(camel: &str) -> String {
    let const_case = camel_case_to_constant_case(camel);
    const_case.replace('_', "-").to_lowercase()
}

/// Helper function to convert `CamelCase` to `snake_case`.
fn camel_case_to_snake_case(camel: &str) -> String {
    camel_case_to_constant_case(camel).to_lowercase()
}

/// Information about an enum field for serde generation.
#[derive(Debug)]
struct EnumFieldInfo {
    /// Full protobuf path to the field (e.g., ".foxglove.Log.level")
    field_path: String,
    /// Rust path to the enum type (e.g., "log::Level")
    enum_rust_path: String,
    /// Snake case name for the serde module (e.g., "log_level")
    serde_module_name: String,
}

/// Collect bytes fields and enum fields from the file descriptor set.
fn collect_bytes_and_enum_fields(fds: &FileDescriptorSet) -> (Vec<String>, Vec<EnumFieldInfo>) {
    let mut bytes_fields = Vec::new();
    let mut enum_fields = Vec::new();

    for fd in &fds.file {
        let package = fd.package.as_deref().unwrap_or("");
        for msg in &fd.message_type {
            collect_fields_from_message(package, "", msg, &mut bytes_fields, &mut enum_fields);
        }
    }

    (bytes_fields, enum_fields)
}

fn collect_fields_from_message(
    package: &str,
    parent_path: &str,
    msg: &DescriptorProto,
    bytes_fields: &mut Vec<String>,
    enum_fields: &mut Vec<EnumFieldInfo>,
) {
    let msg_name = msg.name.as_deref().unwrap_or("");
    let msg_path = if parent_path.is_empty() {
        format!(".{package}.{msg_name}")
    } else {
        format!("{parent_path}.{msg_name}")
    };
    let rust_msg_path = if parent_path.is_empty() {
        camel_case_to_snake_case(msg_name)
    } else {
        // Nested message - parent is already snake_case
        let parent_snake = parent_path
            .rsplit('.')
            .next()
            .map(camel_case_to_snake_case)
            .unwrap_or_default();
        format!("{}::{}", parent_snake, msg_name)
    };

    for field in &msg.field {
        collect_field_info(
            &msg_path,
            &rust_msg_path,
            msg,
            field,
            bytes_fields,
            enum_fields,
        );
    }

    // Handle nested messages
    for nested in &msg.nested_type {
        collect_fields_from_message(package, &msg_path, nested, bytes_fields, enum_fields);
    }
}

fn collect_field_info(
    msg_path: &str,
    _rust_msg_path: &str,
    msg: &DescriptorProto,
    field: &FieldDescriptorProto,
    bytes_fields: &mut Vec<String>,
    enum_fields: &mut Vec<EnumFieldInfo>,
) {
    let field_name = field.name.as_deref().unwrap_or("");
    let field_path = format!("{msg_path}.{field_name}");

    // Check for bytes type
    if field.r#type == Some(Type::Bytes as i32) {
        bytes_fields.push(field_path.clone());
    }

    // Check for enum type
    if field.r#type == Some(Type::Enum as i32)
        && let Some(type_name) = &field.type_name
    {
        // type_name is like ".foxglove.Log.Level" or ".foxglove.line_primitive.Type"
        // We need to find the enum within the message's nested enums
        let enum_name = type_name.rsplit('.').next().unwrap_or("");

        // Check if this is a nested enum in the current message
        let is_nested = msg
            .enum_type
            .iter()
            .any(|e| e.name.as_deref() == Some(enum_name));

        if is_nested {
            // For nested enums, use the message's snake_case name as the module
            let msg_snake = msg
                .name
                .as_deref()
                .map(camel_case_to_snake_case)
                .unwrap_or_default();
            let enum_rust_path = format!("{msg_snake}::{enum_name}");
            let serde_module_name =
                format!("{}_{}", msg_snake, camel_case_to_snake_case(enum_name));

            enum_fields.push(EnumFieldInfo {
                field_path,
                enum_rust_path,
                serde_module_name,
            });
        }
    }
}

/// Generates binary file descriptor sets for each foxglove message and well-known types.
fn generate_descriptors(out_dir: &Path, fds: &FileDescriptorSet) -> anyhow::Result<()> {
    let fd_map: HashMap<_, _> = fds
        .file
        .iter()
        .filter_map(|f| f.name.as_ref().map(|n| (n.clone(), f)))
        .collect();

    let descr_dir = out_dir.join("data");
    if descr_dir.exists() {
        fs::remove_dir_all(&descr_dir).context("Failed to remove descriptor directory")?;
    }
    fs::create_dir_all(&descr_dir).context("Failed to create descriptor directory")?;

    let mut descr_map = BTreeMap::new();
    for fd in &fds.file {
        let Some(name) = fd.name.as_ref() else {
            continue;
        };

        // Handle foxglove/ and google/protobuf/ prefixes
        let n = if let Some(n) = name
            .strip_prefix("foxglove/")
            .and_then(|n| n.strip_suffix(".proto"))
        {
            n.to_string()
        } else if let Some(n) = name
            .strip_prefix("google/protobuf/")
            .and_then(|n| n.strip_suffix(".proto"))
        {
            // Capitalize first letter to match PascalCase convention
            let mut chars = n.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect(),
                None => continue,
            }
        } else {
            continue;
        };
        let file_name = format!("{n}.bin");
        let var_name = camel_case_to_constant_case(&n);

        let path = descr_dir.join(&file_name);
        let mut descr_file = File::create(&path).context("Failed to create descriptor file")?;
        let bin = build_fds(fd, &fd_map).encode_to_vec();
        descr_file
            .write_all(&bin)
            .context("Failed to write descriptor")?;
        let is_wkt = name.starts_with("google/protobuf/");
        descr_map.insert(var_name, (file_name, is_wkt));
    }

    let mut module =
        File::create(out_dir.join("descriptors.rs")).context("Failed to create descriptors.rs")?;

    writeln!(module, "// This file is @generated by foxglove_proto_gen")
        .context("Failed to write descriptors.rs")?;

    for (var_name, (file_name, is_wkt)) in descr_map {
        // Well-known types are only used by the derive feature
        if is_wkt {
            writeln!(module, "#[cfg(feature = \"derive\")]")
                .context("Failed to write descriptors.rs")?;
        }
        writeln!(
            module,
            "pub const {var_name}: &[u8] = include_bytes!(\"data/{file_name}\");"
        )
        .context("Failed to write descirptors.rs")?;
    }

    Ok(())
}

fn generate_impls(out_dir: &Path, fds: &FileDescriptorSet) -> anyhow::Result<()> {
    let mut module = File::create(out_dir.join("impls.rs")).context("Failed to create impls.rs")?;

    let mut result = writeln!(module, "// This file is @generated by foxglove_proto_gen");
    result = result.and(writeln!(
        module,
        "use crate::messages::{{descriptors, foxglove::*}};"
    ));
    result = result.and(writeln!(module, "use crate::{{Schema, Decode, Encode}};"));
    result = result.and(writeln!(module, "use bytes::BufMut;"));
    result = result.and(writeln!(module, "\n#[cfg(feature = \"derive\")]"));
    result = result.and(writeln!(module, "use prost::Message as _;"));
    result = result.and(writeln!(module, "#[cfg(feature = \"derive\")]"));
    result = result.and(writeln!(module, "use crate::protobuf::ProtobufField;"));
    result.context("Failed to write impls.rs")?;

    for fd in &fds.file {
        let Some(mut name) = fd
            .name
            .as_ref()
            .and_then(|n| n.strip_prefix("foxglove/"))
            .and_then(|n| n.strip_suffix(".proto"))
        else {
            continue;
        };
        let schema_name = name;
        // Use rust casing for the struct name, but preserve the original casing for the schema.
        if name == "GeoJSON" {
            name = "GeoJson";
        }
        let descriptor_name = camel_case_to_constant_case(name);
        writeln!(
            module,
            "\nimpl Encode for {name} {{
    type Error = ::prost::EncodeError;

    fn get_schema() -> Option<Schema> {{
        Some(Schema::new(
            \"foxglove.{schema_name}\",
            \"protobuf\",
            descriptors::{descriptor_name},
        ))
    }}

    fn get_message_encoding() -> String {{
        \"protobuf\".to_string()
    }}

    fn encode(&self, buf: &mut impl BufMut) -> Result<(), prost::EncodeError> {{
        ::prost::Message::encode(self, buf)
    }}

    fn encoded_len(&self) -> Option<usize> {{ Some(::prost::Message::encoded_len(self)) }}
}}"
        )
        .context("Failed to write trait impl in impls.rs")?;

        writeln!(
            module,
            "\n#[doc(hidden)]
impl Decode for {name} {{
    type Error = ::prost::DecodeError;

    /// Decode a message from a serialized buffer.
    fn decode(buf: impl bytes::Buf) -> Result<Self, ::prost::DecodeError> {{
        ::prost::Message::decode(buf)
    }}
}}"
        )
        .context("Failed to write impl in impls.rs")?;

        // Generate ProtobufField impl (only with derive feature)
        writeln!(
            module,
            "\n#[cfg(feature = \"derive\")]
impl ProtobufField for {name} {{
    fn field_type() -> ::prost_types::field_descriptor_proto::Type {{
        ::prost_types::field_descriptor_proto::Type::Message
    }}

    fn wire_type() -> u32 {{
        ::prost::encoding::WireType::LengthDelimited as u32
    }}

    fn write(&self, buf: &mut impl BufMut) {{
        let len = ::prost::Message::encoded_len(self);
        ::prost::encoding::encode_varint(len as u64, buf);
        ::prost::Message::encode_raw(self, buf);
    }}

    fn type_name() -> Option<String> {{
        Some(\".foxglove.{schema_name}\".to_string())
    }}

    fn file_descriptors() -> Vec<::prost_types::FileDescriptorProto> {{
        let fds = ::prost_types::FileDescriptorSet::decode(descriptors::{descriptor_name})
            .expect(\"invalid file descriptor set\");
        fds.file
    }}

    fn encoded_len(&self) -> usize {{
        let inner_len = ::prost::Message::encoded_len(self);
        ::prost::encoding::encoded_len_varint(inner_len as u64) + inner_len
    }}
}}"
        )
        .context("Failed to write ProtobufField impl in impls.rs")?;
    }

    Ok(())
}

/// Generates protobuf structs and descriptors.
pub fn generate_protos(proto_path: &Path, out_dir: &Path) -> anyhow::Result<()> {
    let proto_path = fs::canonicalize(proto_path).context("Failed to canonicalize proto path")?;

    if let Err(err) = fs::create_dir(out_dir)
        && err.kind() != io::ErrorKind::AlreadyExists
    {
        panic!("Failed to create directory: {err}");
    }

    let mut proto_files: Vec<PathBuf> = vec![];
    for entry in WalkDir::new(&proto_path) {
        let entry = entry.expect("Failed to read entry");
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.path().extension().is_some_and(|ext| ext == "proto") {
            proto_files.push(entry.path().to_path_buf());
        }
    }

    let mut config = prost_build::Config::new();
    config.message_attribute(".", format!("/// {DOC_REF}"));

    // Add conditional serde derives for the "serde" feature.
    config.message_attribute(
        ".",
        "#[cfg_attr(feature = \"serde\", derive(::serde::Serialize, ::serde::Deserialize))]",
    );

    config.extern_path(".google.protobuf.Duration", "crate::messages::Duration");
    config.extern_path(".google.protobuf.Timestamp", "crate::messages::Timestamp");
    config.out_dir(out_dir);
    config.bytes(["."]);

    // Load file descriptors for introspection
    let mut fds = config
        .load_fds(&proto_files, &[proto_path])
        .context("Failed to load protos")?;
    fds.file.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    // Collect bytes and enum fields via introspection
    let (bytes_fields, enum_fields) = collect_bytes_and_enum_fields(&fds);

    // Add serde attributes for bytes fields
    for field_path in &bytes_fields {
        config.field_attribute(
            field_path,
            "#[cfg_attr(feature = \"serde\", serde(with = \"crate::messages::serde_bytes\"))]",
        );
    }

    // Add serde attributes for enum fields. The serde implementations referenced here are appended
    // to the foxglove.rs by generate_serde_enum_module() below.
    for info in &enum_fields {
        config.field_attribute(
            &info.field_path,
            format!(
                "#[cfg_attr(feature = \"serde\", serde(with = \"serde_enum::{}\"))]",
                info.serde_module_name
            ),
        );
    }

    generate_descriptors(out_dir, &fds).context("Failed to generate descriptor files")?;

    generate_impls(out_dir, &fds).context("Failed to generate impls")?;

    config
        .compile_fds(fds.clone())
        .context("Failed to compile protos")?;

    fix_generated_comments(out_dir).context("Failed to fix docstrings")?;

    // Generate serde implementations for enum types.
    generate_serde_enum_module(out_dir, &enum_fields)
        .context("Failed to generate enum serde modules")?;

    Ok(())
}

/// Appends serde implementations for enums to the generated file.
fn generate_serde_enum_module(out_dir: &Path, enum_fields: &[EnumFieldInfo]) -> anyhow::Result<()> {
    let schema_path = out_dir.join("foxglove.rs");
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(&schema_path)
        .context("Failed to open foxglove.rs for appending")?;

    // Collect unique enums (multiple fields may use the same enum)
    let mut seen = HashSet::new();
    let unique_enums: Vec<_> = enum_fields
        .iter()
        .filter(|info| seen.insert(&info.serde_module_name))
        .collect();
    if unique_enums.is_empty() {
        return Ok(());
    }

    writeln!(file, "#[cfg(feature = \"serde\")]")?;
    writeln!(file, "pub(crate) mod serde_enum {{")?;
    writeln!(file, "    use super::*;")?;
    writeln!(file, "    use serde::de::Error as _;")?;
    writeln!(
        file,
        "    use serde::{{Deserialize, Deserializer, Serializer}};"
    )?;
    writeln!(file)?;

    for info in unique_enums {
        writeln!(
            file,
            "    crate::messages::serde_enum_mod!({}, {});",
            info.serde_module_name, info.enum_rust_path
        )?;
    }

    writeln!(file, "}}")?;

    Ok(())
}

/// Convert all documentation code blocks to text to avoid errors when running doc tests (hack)
fn fix_generated_comments(out_dir: &Path) -> anyhow::Result<()> {
    let schema_path = out_dir.join("foxglove.rs");

    let mut tmpfile = NamedTempFile::new_in(out_dir).context("Failed to create tempfile")?;
    let input = File::open(schema_path.clone()).context("Failed to open schema file")?;
    let mut input = io::BufReader::new(input).lines().multipeek();

    let mut in_code_block = false;
    let mut prev_line_was_doc = false;
    let struct_pattern = Regex::new(r"pub struct (?<struct>\w+)").unwrap();

    while let Some(line) = input.next() {
        let mut line = line.context("Failed to read line")?;

        // Replace the intro doc URL with one to specific schema docs.
        // Also add a blank doc line before the URL if it follows other doc content
        // to avoid clippy::doc_lazy_continuation lint errors.
        if line.contains(DOC_REF) {
            if prev_line_was_doc {
                writeln!(tmpfile, "///").context("Failed to write blank doc line")?;
            }
            while let Some(Ok(next_line)) = input.peek() {
                if let Some(captures) = struct_pattern.captures(next_line) {
                    let struct_name = captures.name("struct").context("Unexpected match")?;
                    let doc_slug = camel_case_to_kebab_case(struct_name.as_str());
                    line = line.replace(
                        "message-schemas/introduction",
                        &format!("message-schemas/{doc_slug}"),
                    );
                    break;
                } else if next_line.contains(DOC_REF) {
                    break;
                }
            }
        }

        // Track whether the current line is a doc comment (for detecting when URL follows content)
        prev_line_was_doc = line.starts_with("///") && !line.contains(DOC_REF);

        if line.trim_start().eq("/// ```") {
            if !in_code_block {
                line = format!("{line}text");
            }
            in_code_block = !in_code_block;
        } else if in_code_block {
            // Protoc turns this:
            //
            // ```
            //     [a 0 0]
            // P = [0 b 0]
            //     [0 0 c]
            // ```
            //
            // Into this:
            //
            // ```
            //      \[a 0 0\]
            // P = \[0 b 0\]
            //      \[0 0 c\]
            // ```
            //
            // Remove the escapes, and the extra space added to lines that begin with whitespace.
            line = line.replace('\\', "");
            line = line.replace("///  ", "/// ")
        }
        writeln!(tmpfile, "{line}").context("Failed to write to output file")?;
    }

    rename(tmpfile.path(), schema_path).context("Failed to rename tempfile")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_camel_case_to_constant_case() {
        let cases = [
            ("A", "A"),
            ("a", "A"),
            ("Abc", "ABC"),
            ("abc", "ABC"),
            ("ABC", "ABC"),
            ("AbcDef", "ABC_DEF"),
            ("abcDef", "ABC_DEF"),
            ("abcdef", "ABCDEF"),
            ("AbcDEF", "ABC_DEF"),
            ("ABCDef", "ABC_DEF"),
            ("ABCDEF", "ABCDEF"),
        ];
        for (input, output) in cases {
            dbg!(input, output);
            assert_eq!(camel_case_to_constant_case(input), output);
        }
    }
}
