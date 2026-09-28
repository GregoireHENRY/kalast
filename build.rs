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
//!
//! On macOS, kalast's executables -- the UI app and the examples -- are
//! recorded as linked against the macOS 15.5 SDK, whatever SDK the
//! toolchain has. AppKit takes its "linked on or after" behaviour from the
//! main executable's SDK, and for one linked against the macOS 26 SDK,
//! `[NSApp run]` returns once per display refresh: every winit pump, so every
//! `step()`, waited for the display -- 130 frames/s on an empty scene where
//! the same binary recorded as 15.5 drew 3,900. Only the record changes; the
//! code is linked against the SDK installed. The Python module is left alone:
//! there the main executable is Python's.
//! `notes/2026-09-28_macos26_sdk_paces_the_pump.md`.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=res/kalast.rc");
    println!("cargo:rerun-if-changed=res/kalast.ico");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let min = deployment_target(&arch);
        for kind in ["bins", "examples"] {
            println!("cargo:rustc-link-arg-{kind}=-Wl,-platform_version,macos,{min},15.5");
        }
    }
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
    if windows {
        let icon = embed_resource::compile_for("res/kalast.rc", ["kalast"], embed_resource::NONE);
        if let Err(e) = icon.manifest_optional() {
            println!("cargo:warning=kalast.exe keeps the default icon: {e}");
        }
    }
}

/// The macOS version rustc links for, worked out as rustc does:
/// `MACOSX_DEPLOYMENT_TARGET` when set, raised to the architecture's own
/// minimum -- 11.0 on Apple silicon, 10.12 on Intel -- which is also the
/// default. The linker takes the last `-platform_version` it is given, so
/// this one has to repeat rustc's minimum, not only set the SDK.
fn deployment_target(arch: &str) -> String {
    let floor = if arch == "aarch64" { (11, 0) } else { (10, 12) };
    let wanted = std::env::var("MACOSX_DEPLOYMENT_TARGET").ok().and_then(|v| {
        let mut parts = v.trim().split('.').map(|p| p.parse::<u32>());
        match (parts.next(), parts.next()) {
            (Some(Ok(major)), None) => Some((major, 0)),
            (Some(Ok(major)), Some(Ok(minor))) => Some((major, minor)),
            _ => None,
        }
    });
    let (major, minor) = wanted.map_or(floor, |w| w.max(floor));
    format!("{major}.{minor}")
}
