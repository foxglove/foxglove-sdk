use std::env;
use std::path::PathBuf;

fn main() {
    let project_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Get the target triple
    let target = env::var("TARGET").unwrap();

    // Link the static library.
    // Note: The library is built with Rust and exports Rust stdlib symbols that conflict
    // with the stdlib in any Rust binary that links this. The consuming crate (foxglove)
    // handles this by passing --allow-multiple-definition to the linker in its build.rs.
    println!("cargo:rustc-link-search=native={project_dir}");
    println!("cargo:rustc-link-lib=static=foxglove_agent_sdk_{target}");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        .allowlist_function("foxglove_.*")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(PathBuf::from(project_dir).join("src/bindings.rs"))
        .expect("Couldn't write bindings!");
}
