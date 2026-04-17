use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    cbindgen::generate(crate_dir)
        .expect("Unable to generate bindings")
        .write_to_file("include/foxglove-c/foxglove-c.h");

    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/");

    // Embed a soname so that consumers record "libfoxglove.so" as the NEEDED
    // entry rather than the build-time absolute path.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "linux" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libfoxglove.so");
    } else if target_os == "macos" {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libfoxglove.dylib");
    }
}
