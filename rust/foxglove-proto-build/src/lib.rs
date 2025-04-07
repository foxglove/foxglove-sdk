use heck::ToUpperCamelCase;
use quote::quote;
use std::{
    collections::HashMap,
    env, fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use prost::Message;
use prost_build::Module;

#[derive(Debug)]
pub enum CompileError {
    IoError(Error),
    ProtoxError(protox::Error),
    SynError(syn::Error),
}

impl From<Error> for CompileError {
    fn from(err: Error) -> Self {
        CompileError::IoError(err)
    }
}

impl From<protox::Error> for CompileError {
    fn from(err: protox::Error) -> Self {
        CompileError::ProtoxError(err)
    }
}

impl From<syn::Error> for CompileError {
    fn from(err: syn::Error) -> Self {
        CompileError::SynError(err)
    }
}

pub type Result<T> = std::result::Result<T, CompileError>;

pub fn compile(protos: &[impl AsRef<Path>], includes: &[impl AsRef<Path>]) -> Result<()> {
    // fixme - we want to load the fds for each proto one at a time so we can make a schema per proto
    // that is isolated to the types in that proto and includes rather than all protos across the entire
    // project

    // Iterate the provided proto files and generate a source file for each
    for proto in protos {
        compile_proto(&proto, includes)?
    }

    Ok(())
}

fn compile_proto(proto: &impl AsRef<Path>, includes: &[impl AsRef<Path>]) -> Result<()> {
    // native rust protox compiler
    let file_descriptor_set = protox::compile([proto], includes)?;

    // Alternative is to build with protoc
    // let file_descriptor_set = config.load_fds(&[proto], includes)?;

    let mut config = prost_build::Config::new();

    // We need the full type name available on the type so we can provide it when implementing
    // foxglove::Encode::get_schema
    config.enable_type_names();

    let file_descriptor_set_bytes = file_descriptor_set.encode_to_vec();

    //config.compile_fds(file_descriptor_set).unwrap();

    let target: PathBuf = env::var_os("OUT_DIR")
        .ok_or_else(|| Error::new(ErrorKind::Other, "OUT_DIR environment variable is not set"))
        .map(Into::into)?;

    // make a vector of string type names
    let mut type_names = Vec::new();

    file_descriptor_set.file.iter().for_each(|descriptor| {
        descriptor.message_type.iter().for_each(|message_type| {
            let ident = message_type.name().to_upper_camel_case();
            type_names.push(ident);
        })
    });

    let requests = file_descriptor_set
        .file
        .into_iter()
        .map(|descriptor| {
            (
                Module::from_protobuf_package_name(descriptor.package()),
                descriptor,
            )
        })
        .collect::<Vec<_>>();

    let file_names = requests
        .iter()
        .map(|req| (req.0.clone(), req.0.to_file_name_or("_")))
        .collect::<HashMap<Module, String>>();

    let modules = config.generate(requests)?;
    for (module, content) in &modules {
        let file_name = file_names
            .get(module)
            .expect("every module should have a filename");

        let fds_file_name = file_name.to_string() + ".fds.bin";

        let fds_path = target.join(&fds_file_name);
        write_file_if_changed(&fds_path, &file_descriptor_set_bytes)?;

        let output_path = target.join(file_name);

        let mut content = content.clone();

        // For each type name, generate the Encode impl and append to content
        for type_name in &type_names {
            let ident = syn::parse_str::<syn::Type>(type_name).unwrap();

            let tokens = quote!(
                const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!(#fds_file_name);

                impl foxglove::Encode for #ident {
                    type Error = prost::EncodeError;
                    fn get_schema() -> Option<foxglove::Schema> {
                        use prost::Name;
                        let full_name = format!("{}.{}", #ident::PACKAGE, #ident::NAME);

                        Some(foxglove::Schema::new(
                            full_name,
                            "protobuf".to_string(),
                            ::std::borrow::Cow::Owned(FILE_DESCRIPTOR_SET.to_vec()),
                        ))
                    }

                    fn get_message_encoding() -> String {
                        "protobuf".to_string()
                    }

                    fn encode(&self, buf: &mut impl prost::bytes::BufMut) -> Result<(), Self::Error> {
                        ::prost::Message::encode(self, buf)
                    }
                }
            );

            let syntax_tree = syn::parse2(tokens)?;
            let formatted = prettyplease::unparse(&syntax_tree);

            content.push('\n');
            content.push_str(&formatted.to_string());
        }

        write_file_if_changed(&output_path, content.as_bytes())?;
    }

    Ok(())
}

/// Overwrite the file at the specified path with the specified content if the content is different
/// from the existing content or if the file does not exist.
fn write_file_if_changed(path: &Path, content: &[u8]) -> std::io::Result<()> {
    let previous_content = fs::read(path);

    if previous_content
        .map(|previous_content| previous_content == content)
        .unwrap_or(false)
    {
        return Ok(());
    }

    fs::write(path, content)
}
