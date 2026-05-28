use std::io::Result;
fn main() -> Result<()> {
    // Generate the file descriptor set in addition to the Rust source
    prost_build::Config::new()
        .out_dir("generated")
        .file_descriptor_set_path("generated/apple.fdset")
        .compile_protos(&["src/apple.proto"], &["src/"])?;

    Ok(())
}
