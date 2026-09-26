//! Windows gives the main thread 1 MB of stack where macOS gives 8. A hosted
//! `.rs` example's `main` runs inside the host's frame and drives another
//! frame from inside it -- two frames of egui and wgpu on one stack -- which
//! is the shape a stack overflow takes and the one crash that would be
//! Windows-only. 16 MB for the executable; a library takes its host's.
//!
//! And kalast.exe's icon, the logo, which Explorer, a shortcut and the
//! taskbar show: a Windows resource linked into that binary alone, never the
//! Python module. Optional: without the Windows SDK's resource compiler the
//! build goes on, with Windows' default icon.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/app/gui/assets/kalast.rc");
    println!("cargo:rerun-if-changed=src/app/gui/assets/kalast.ico");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
    if windows {
        let icon = embed_resource::compile_for("src/app/gui/assets/kalast.rc", ["kalast"], embed_resource::NONE);
        if let Err(e) = icon.manifest_optional() {
            println!("cargo:warning=kalast.exe keeps the default icon: {e}");
        }
    }
}
