use std::env;
use std::path::PathBuf;

fn main() {
    let project_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Tell cargo to look for shared libraries in the specified directory
    println!("cargo:rustc-link-search=native={project_dir}");

    // Tell cargo to tell rustc to link the system bzip2
    // shared library.
    println!("cargo:rustc-link-lib=static=foxglove_enterprise");

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // Treat the header as C++ code (required for C++ headers like <cstdarg>)
        .clang_arg("-xc++")
        // Use libstdc++ and ensure system headers can be found
        .clang_arg("-stdlib=libstdc++")
        // Blocklist C++ standard library types that bindgen can't handle properly
        .blocklist_type("std::.*")
        .blocklist_item("std::.*")
        .allowlist_function("foxglove_.*")
        // Disable layout tests that cause issues with C++ types
        .layout_tests(false)
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
