use std::env;

fn main() {
    // When linking foxglove_agent, we need to allow multiple symbol definitions
    // because the agent library is built with Rust and exports Rust stdlib symbols
    // that conflict with the stdlib symbols in our binary.
    //
    // Note: cargo:rustc-link-arg only affects binaries/tests built by this package,
    // it does NOT propagate from dependencies. That's why this needs to be here
    // rather than in foxglove_agent's build.rs.
    if env::var("CARGO_FEATURE_AGENT_UNSTABLE").is_ok() {
        let target = env::var("TARGET").unwrap_or_default();

        if target.contains("linux") {
            // For GNU ld and LLD
            println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
        } else if target.contains("darwin") {
            // For macOS ld64
            println!("cargo:rustc-link-arg=-Wl,-multiply_defined,suppress");
        } else if target.contains("windows-msvc") {
            // For Windows MSVC linker
            println!("cargo:rustc-link-arg=/FORCE:MULTIPLE");
        } else if target.contains("windows-gnu") {
            // For Windows MinGW
            println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
        }
    }
}
