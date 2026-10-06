use std::env;

fn main() {
    // Workaround for livekit/rust-sdks#795: the prebuilt WebRTC binaries in the
    // `livekit` crate define ObjC category methods that require `-ObjC` to be
    // linked. Without this flag, macOS strips the categories at link time,
    // causing an `unrecognized selector` crash at runtime.
    //
    // The workspace-root `.cargo/config.toml` sets this for in-repo cargo
    // builds, but isn't shipped in the sdist — so wheels built from the sdist
    // (including the published macOS wheels) don't get it. Emitting it from
    // build.rs ensures it's applied wherever this crate is built. The flag
    // can't come from a dependency's build script (including webrtc-sys, per
    // livekit/rust-sdks#847), because cargo applies link args only to the
    // emitting package's own targets.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target_os == "macos" {
        println!("cargo:rustc-cdylib-link-arg=-ObjC");
    }
}
