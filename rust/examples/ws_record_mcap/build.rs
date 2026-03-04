fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());

    // ws_record_mcap lives at rust/examples/ws_record_mcap/; the workspace root is 3 levels up.
    let workspace_root = std::path::Path::new(&manifest_dir)
        .join("../../..")
        .canonicalize()
        .unwrap();

    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| workspace_root.join("target"));

    let mut exe = target_dir.join(&profile).join("example_ws_stream_mcap");

    if cfg!(target_os = "windows") {
        exe.set_extension("exe");
    }

    println!("cargo:rustc-env=STREAM_SERVER_EXE={}", exe.display());
    println!("cargo:rerun-if-changed=build.rs");
}
