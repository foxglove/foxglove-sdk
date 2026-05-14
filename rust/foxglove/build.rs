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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CUDA_HOME");
    println!("cargo:rerun-if-env-changed=FOXGLOVE_REMOTE_ACCESS_QUIET");

    if env::var_os("CARGO_FEATURE_REMOTE_ACCESS").is_none() {
        return;
    }

    // docs.rs builds with --all-features; don't surface build environment
    // warnings there.
    if env::var_os("DOCS_RS").is_some() {
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

    let cuda_home_display = cuda_home.display();
    let header_display = cuda_header.display();
    let lines: [String; 13] = [
        format!(
            "remote-access feature enabled, but CUDA headers were not found at {header_display}."
        ),
        "  libwebrtc will fall back to software H.264 encoding for live video, which uses"
            .to_string(),
        "  significantly more CPU and produces lower-quality output than NVENC on NVIDIA GPUs."
            .to_string(),
        "  To enable hardware-accelerated H.264/H.265 encoding via NVENC:".to_string(),
        "    1. Install the CUDA Toolkit headers (only the headers are needed at build time):"
            .to_string(),
        "         Ubuntu/Debian: sudo apt install nvidia-cuda-toolkit".to_string(),
        "         Or download from https://developer.nvidia.com/cuda-downloads".to_string(),
        format!("    2. Verify {header_display} exists, or set CUDA_HOME to your install prefix."),
        "    3. Force a rebuild of webrtc-sys so it picks up the new headers:".to_string(),
        "         cargo clean -p webrtc-sys".to_string(),
        format!(
            "         CUDA_HOME={cuda_home_display} cargo build --release --features remote-access"
        ),
        "  At runtime the binary dlopen()s libcuda.so.1 and libnvcuvid.so.1; install the"
            .to_string(),
        "  matching NVIDIA driver on the deployment host.".to_string(),
    ];
    for line in lines {
        println!("cargo:warning={line}");
    }
}
