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
//!
//! And the examples, packed for the bundle's executable to put back in
//! `examples/` when an older updater left that folder as it was
//! (`update::settle_examples`).
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
    println!("cargo:rerun-if-changed=examples");
    let pack = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("examples.pack");
    let packed = pack_examples(std::path::Path::new("examples"));
    // Written only when it changed: an example run writes its bytecode and
    // its output beside it, which reruns this, and a pack rewritten the same
    // would still recompile the crate.
    if std::fs::read(&pack).ok().as_deref() != Some(packed.as_slice()) {
        std::fs::write(&pack, packed).expect("write examples.pack");
    }
    // What says it is there: the crate on crates.io ships without this
    // script, so without `OUT_DIR`, and builds with none.
    println!("cargo:rustc-check-cfg=cfg(kalast_examples)");
    println!("cargo:rustc-cfg=kalast_examples");
}

/// Every file of `examples/` but what a folder collects -- the Finder's and
/// Explorer's records, Python's bytecode -- and the version a bundle writes
/// there. `KALASTEX`, a little-endian u32 count, then per file a u32 length
/// and its path under `examples/` with `/`, a u64 length and its bytes, in
/// path order, so one tree packs to the same bytes. A crate built from
/// crates.io has only `examples/README.md` to pack.
fn pack_examples(root: &std::path::Path) -> Vec<u8> {
    fn walk(dir: &std::path::Path, prefix: &str, files: &mut Vec<(String, std::path::PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), ".DS_Store" | "Thumbs.db" | "desktop.ini" | "__pycache__" | ".kalast-version") {
                continue;
            }
            let rel = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
            match entry.file_type() {
                Ok(t) if t.is_dir() => walk(&entry.path(), &rel, files),
                Ok(t) if t.is_file() => files.push((rel, entry.path())),
                _ => {}
            }
        }
    }
    let mut files = vec![];
    walk(root, "", &mut files);
    files.sort();
    let mut out = b"KALASTEX".to_vec();
    out.extend((files.len() as u32).to_le_bytes());
    for (rel, path) in files {
        let data = std::fs::read(&path).expect("read an example");
        out.extend((rel.len() as u32).to_le_bytes());
        out.extend(rel.as_bytes());
        out.extend((data.len() as u64).to_le_bytes());
        out.extend(data);
    }
    out
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
