//! Build script for the `foxglove` crate.
//!
//! When the `remote-access` feature is enabled, the crate pulls in
//! `livekit` / `libwebrtc` / `webrtc-sys`. On targets where `webrtc-sys`
//! tries to compile NVIDIA NVENC support (Linux on x86_64 / aarch64 / arm),
//! its build script silently falls back to software H.264/H.265 encoding if
//! `cuda.h` is not found at build time. That fallback is easy to miss and
//! results in dramatically higher CPU usage and lower video quality for live
//! remote access.
//!
//! This script mirrors the detection performed by `webrtc-sys`'s own
//! `build.rs` (look for `$CUDA_HOME/include/cuda.h`, defaulting to
//! `/usr/local/cuda`) and emits a `cargo:warning=` with installation
//! instructions when the headers are missing on a target where webrtc-sys
//! would otherwise have built NVENC support.

use std::env;
use std::path::PathBuf;

enum NvencRequirement {
    Required,
    Warn,
    Off,
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    println!("cargo:rerun-if-env-changed=FOXGLOVE_REMOTE_ACCESS_NVENC");
    println!("cargo:rerun-if-env-changed=PROFILE");

    // We are only interested in nvenv support if we're compiling in remote access support
    if env::var_os("CARGO_FEATURE_REMOTE_ACCESS").is_none() {
        return;
    }

    // Don't surface warnings if we're building docs.
    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    let nvenc_requirement = match env::var("FOXGLOVE_REMOTE_ACCESS_NVENC")
        .unwrap_or_else(|_| "warn".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "required" => NvencRequirement::Required,
        "off" => NvencRequirement::Off,
        _ => NvencRequirement::Warn,
    };
    if matches!(nvenc_requirement, NvencRequirement::Off) {
        return;
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // webrtc-sys only compiles NVENC support on Linux for x86 / aarch64 / arm
    // (see webrtc-sys/build.rs). Stay silent on platforms where it would not
    // have been built regardless of CUDA availability.
    if target_os != "linux" {
        return;
    }
    let cuda_supported_arch =
        matches!(target_arch.as_str(), "x86_64" | "x86" | "aarch64") || target_arch.contains("arm");
    if !cuda_supported_arch {
        return;
    }

    let cuda_home = env::var_os("CUDA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/local/cuda"));
    let cuda_header = cuda_home.join("include").join("cuda.h");

    if cuda_header.exists() {
        return;
    }

    let header_display = cuda_header.display();
    let warning = format!(
        "cuda.h was not found at {header_display}\nH.264 software encoding will be used for video encoding instead of nvenc\nLearn more: https://docs.rs/foxglove/latest/foxglove/#remote-access-gateway"
    );
    match nvenc_requirement {
        NvencRequirement::Warn => {
            for line in warning.split('\n') {
                println!("cargo:warning={line}");
            }
        }
        NvencRequirement::Required => {
            panic!("{warning}");
        }
        NvencRequirement::Off => {}
    }
}
