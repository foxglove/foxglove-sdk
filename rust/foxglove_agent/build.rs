use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let project_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    // Get the target triple
    let target = env::var("TARGET").unwrap();

    let original_lib =
        PathBuf::from(&project_dir).join(format!("libfoxglove_agent_sdk_{target}.a"));

    // For Linux, we can use objcopy to localize symbols that don't match our exported foxglove symbols
    // This prevents duplicate symbol errors by making them local to the archive
    if target.contains("linux") {
        let localized_lib =
            PathBuf::from(&out_dir).join(format!("libfoxglove_agent_sdk_{target}_localized.a"));

        // Copy the library first
        std::fs::copy(&original_lib, &localized_lib).expect("Failed to copy static library");

        // Localize Rust std library symbols explicitly
        // This approach only touches symbols we know conflict, leaving foxglove_* intact
        if !Command::new("objcopy")
            .arg("--wildcard")
            .arg("--localize-symbol=!foxglove_*")
            .arg("--localize-symbol=*")
            .arg(&localized_lib)
            .status()
            .expect("Failed to localize Rust std symbols")
            .success()
        {
            panic!("Failed to localize Rust std symbols");
        }

        // Use the localized library
        println!("cargo:rustc-link-search=native={out_dir}");
        println!("cargo:rustc-link-lib=static=foxglove_agent_sdk_{target}_localized");
    } else {
        // For other platforms, use the original library with appropriate linker flags
        println!("cargo:rustc-link-search=native={project_dir}");
        println!("cargo:rustc-link-lib=static=foxglove_agent_sdk_{target}");

        // Untested if these are needed or help
        //
        // if target.contains("darwin") {
        //     // For macOS (ld64), allow multiply-defined symbols
        //     println!("cargo:rustc-link-arg=-Wl,-flat_namespace");
        // } else if target.contains("windows") {
        //     // For Windows MSVC linker
        //     println!("cargo:rustc-link-arg=/FORCE:MULTIPLE");
        // }
    }

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
