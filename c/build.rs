use std::env;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let header_path = if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32" {
        "include/foxglove-c/foxglove-c.wasm.h"
    } else {
        "include/foxglove-c/foxglove-c.h"
    };

    cbindgen::generate(crate_dir)
        .expect("Unable to generate bindings")
        .write_to_file(header_path);

    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=src/");
}
