//! Build script for the `foxglove` crate.
//!
//! When the `remote-access` feature is enabled, the crate pulls in
//! `livekit` / `libwebrtc` / `webrtc-sys`. On targets where `webrtc-sys`
//! tries to compile NVIDIA NVENC support (Linux on x86_64 / aarch64 / arm),
//! its build script falls back to software H.264/H.265 encoding if `cuda.h`
//! is not found at build time. Upstream does emit a `cargo:warning=` for the
//! fallback, but cargo hides build-script warnings from non-path (registry)
//! dependencies, so the fallback silently slips through in practice. The
//! result is dramatically higher CPU usage and lower video quality for live
//! remote access.
//!
//! When the `require-cuda` feature is enabled, this script mirrors the set
//! of targets `webrtc-sys`'s own `build.rs` builds NVENC support for and
//! performs the same `cuda.h` lookup (`$CUDA_HOME/include/cuda.h`,
//! defaulting to `/usr/local/cuda/include/cuda.h`). If the header is
//! missing on a target where webrtc-sys would otherwise have built NVENC
//! support, the build fails with a reference to the docs.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CUDA_HOME");

    // The `require-cuda` feature is what opts in to the cuda.h check.
    // Without it we do nothing.
    if env::var_os("CARGO_FEATURE_REQUIRE_CUDA").is_none() {
        return;
    }

    // The require-cuda check is only meaningful when remote-access is also enabled,
    // since that's the only thing that pulls in webrtc-sys / NVENC support.
    if env::var_os("CARGO_FEATURE_REMOTE_ACCESS").is_none() {
        panic!(
            "The `require-cuda` feature is enabled, but the `remote-access` feature is not enabled.\n\
             Enable the `remote-access` feature or disable the `require-cuda` feature.\n\
             Learn more: https://docs.rs/foxglove/latest/foxglove/#nvenc-hardware-acceleration"
        );
    }

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // webrtc-sys only compiles NVENC support on Linux for x86 / aarch64 / arm
    // (see webrtc-sys/build.rs). Match the same set of targets, expressed in
    // cargo's `CARGO_CFG_TARGET_ARCH` vocabulary (which uses "x86" rather
    // than the "i686" upstream parses out of the target triple).
    let cuda_supported_arch = target_os == "linux"
        && (matches!(target_arch.as_str(), "x86_64" | "x86" | "aarch64")
            || target_arch.contains("arm"));
    if !cuda_supported_arch {
        panic!(
            "The `require-cuda` feature is enabled, but NVENC is only built by webrtc-sys on Linux \n\
            for x86_64, x86, aarch64, and arm targets, not `{target_os}-{target_arch}`.\n\
            Disable the `require-cuda` feature for this target.\n\
            Learn more: https://docs.rs/foxglove/latest/foxglove/#nvenc-hardware-acceleration"
        );
    }

    let cuda_home = env::var_os("CUDA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/local/cuda"));
    let cuda_header = cuda_home.join("include").join("cuda.h");

    if cuda_header.exists() {
        return;
    }

    let header_display = cuda_header.display();
    panic!(
        "The `require-cuda` feature is enabled but cuda.h was not found at {header_display}.\n\
         Install the CUDA toolkit (e.g. `apt install nvidia-cuda-dev` on Ubuntu, \
         then export CUDA_HOME=/usr) or disable the `require-cuda` feature.\n\
         Learn more: https://docs.rs/foxglove/latest/foxglove/#nvenc-hardware-acceleration"
    );
}
