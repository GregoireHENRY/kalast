//! Windows gives the main thread 1 MB of stack where macOS gives 8. A hosted
//! `.rs` example's `main` runs inside the host's frame and drives another
//! frame from inside it -- two frames of egui and wgpu on one stack -- which
//! is the shape a stack overflow takes and the one crash that would be
//! Windows-only. 16 MB for the executable; a library takes its host's.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
}
