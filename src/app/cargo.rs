//! Building and launching a Rust example from the editor.
//!
//! A `.py` script runs *inside* the editor: the same process, the same
//! window, the scene appearing in the viewport. A `.rs` example cannot --
//! it is a separate program that links kalast as a library, so it has to be
//! compiled and then launched, and it opens a window of its own.
//!
//! So the editor is a different kind of thing for Rust: an editor, a build
//! button and a launcher, rather than a host. What it does keep is the log
//! -- cargo and the example both inherit the redirected stdout, so their
//! output lands in the Log panel beside everything else.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Which `[[example]]` a path belongs to, read from `Cargo.toml`.
///
/// Examples in this repo are named explicitly rather than auto-discovered,
/// because each lives beside a Python script of the same name in a directory
/// cargo does not look into. That table is also the only thing that maps a
/// file back to the `--example` argument that builds it.
pub fn example_for(path: &str) -> Option<String> {
    let manifest = std::fs::read_to_string("Cargo.toml").ok()?;
    let wanted = std::path::Path::new(path);

    let (mut name, mut found) = (None, None);
    for line in manifest.lines() {
        let line = line.trim();
        if line == "[[example]]" {
            (name, found) = (None, None);
            continue;
        }
        // A new table ends the one being read.
        if line.starts_with('[') && line != "[[example]]" {
            (name, found) = (None, None);
            continue;
        }
        if let Some(v) = field(line, "name") {
            name = Some(v);
        }
        if let Some(v) = field(line, "path") {
            found = Some(v);
        }
        if let (Some(n), Some(p)) = (&name, &found) {
            if std::path::Path::new(p) == wanted {
                return Some(n.clone());
            }
        }
    }
    None
}

fn field(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?.trim_start().strip_prefix('=')?;
    Some(rest.trim().trim_matches('"').to_string())
}

/// The cdylib target that goes with an example, by convention.
///
/// Two cargo targets point at one file: the bin for `cargo run --example`,
/// and this for the editor to load into itself.
pub fn dylib_target(name: &str) -> String {
    format!("{name}_lib")
}

/// Whether a target of this name is declared.
pub fn has_target(name: &str) -> bool {
    let Ok(manifest) = std::fs::read_to_string("Cargo.toml") else {
        return false;
    };
    manifest
        .lines()
        .filter_map(|l| field(l.trim(), "name"))
        .any(|n| n == name)
}

/// The cdylib target the editor can load for this file, if there is one.
///
/// Two ways to arrive: the file *is* the cdylib's path, in which case that
/// target is the answer already, or it is the bin's and the cdylib is the
/// same name with `_lib`. Both, because either file of a loadable example is
/// a reasonable thing to open -- and appending `_lib` to a name that already
/// ends in it asks cargo for `crater_main_lib_lib`.
///
/// `None` means the example is not loadable: one that owns its loop --
/// `while app.is_running()` -- is a program, not a scene handed an app, and
/// has only a bin target.
pub fn dylib_for(path: &str) -> Option<String> {
    let name = example_for(path)?;
    if name.ends_with("_lib") && has_target(&name) {
        return Some(name);
    }
    let candidate = dylib_target(&name);
    has_target(&candidate).then_some(candidate)
}

/// Where cargo puts that cdylib, with the platform's prefix and suffix.
pub fn dylib_path(target: &str, release: bool) -> std::path::PathBuf {
    let profile = if release { "release" } else { "debug" };
    let file = format!(
        "{}{}{}",
        std::env::consts::DLL_PREFIX,
        target,
        std::env::consts::DLL_SUFFIX
    );
    std::path::Path::new("target")
        .join(profile)
        .join("examples")
        .join(file)
}

/// Build that cdylib, **with this build's own feature set**.
///
/// Not optional. The `python` feature changes the layout of `Shared` and
/// `Tick`, so a guest built without it and handed an `App` from a host with
/// it reads the wrong bytes. The host is the only thing that knows which it
/// is, so it says so on the command line.
pub fn build_dylib(target: &str, release: bool, busy: Arc<AtomicBool>) {
    let target = target.to_string();
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("cargo");
        cmd.args(["build", "--color=never", "--example", &target]);
        if release {
            cmd.arg("--release");
        }
        if cfg!(feature = "python") {
            cmd.args(["--features", "python"]);
        }
        println!("$ {}", show(&cmd));
        match cmd.status() {
            Ok(s) if s.success() => println!("built {target}"),
            Ok(s) => println!("build failed: cargo exited with {s}"),
            Err(e) => println!("build failed: could not run cargo: {e}"),
        }
        busy.store(false, Ordering::SeqCst);
    });
}

/// Load a built example into this process and run its `scene`.
///
/// The returned `Library` **must outlive every callback the example
/// installed**: those are function pointers into its code, and unloading it
/// while one is armed unmaps the instructions the next frame will call.
///
/// # Safety
///
/// Calls `dlopen` on a file cargo produced from this crate, and hands it a
/// pointer to the live `App`. The fingerprint check below is what makes that
/// defensible; see `kalast::app::abi_fingerprint`.
pub fn load_example(
    target: &str,
    release: bool,
    app: &mut crate::app::App,
) -> Result<libloading::Library, String> {
    let path = dylib_path(target, release);
    if !path.is_file() {
        return Err(format!(
            "{} is not built yet -- press compile first",
            path.display()
        ));
    }

    unsafe {
        let library = libloading::Library::new(&path)
            .map_err(|e| format!("could not load {}: {e}", path.display()))?;

        let abi: libloading::Symbol<extern "C" fn() -> u64> = library
            .get(b"kalast_abi")
            .map_err(|_| format!("{} exports no kalast_abi", path.display()))?;
        let (theirs, ours) = (abi(), crate::app::abi_fingerprint());
        if theirs != ours {
            return Err(format!(
                "{} was built against a different kalast ({theirs:x} against {ours:x}).\n                   rebuild it with the same features as this program -- press compile.",
                path.display()
            ));
        }

        let scene: libloading::Symbol<unsafe extern "C" fn(*mut crate::app::App)> = library
            .get(b"kalast_example")
            .map_err(|_| format!("{} exports no kalast_example", path.display()))?;
        println!("loaded {}", path.display());
        scene(app as *mut _);

        // Dropped by the caller, not here: the symbols are gone from scope but
        // the callbacks the example just installed are not.
        drop(scene);
        drop(abi);
        Ok(library)
    }
}

/// Where cargo puts the built example.
pub fn binary(name: &str, release: bool) -> std::path::PathBuf {
    let profile = if release { "release" } else { "debug" };
    std::path::Path::new("target")
        .join(profile)
        .join("examples")
        .join(name)
}

/// Whether a built binary exists *and* is newer than the source it came from.
///
/// "Built" is not enough on its own: launching a binary older than the file
/// shown in the panel would run code the panel is not displaying, which is a
/// worse lie than an empty viewport. When this is false the editor stays put
/// and the compile button is the next move.
pub fn is_current(name: &str, release: bool, source: &str) -> bool {
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let Some(built) = modified(&dylib_path(name, release)) else {
        return false;
    };
    match modified(std::path::Path::new(source)) {
        Some(edited) => built >= edited,
        // No source to be older than -- a path that cannot be read is a
        // problem for whoever opens it, not a reason to refuse to launch.
        None => true,
    }
}

/// Run `cargo build --example <name>` on a thread, so the frame keeps going.
///
/// `busy` is held for the life of the build and cleared however it ends,
/// including a cargo that could not be started at all -- a button that stays
/// disabled forever is worse than a build that failed.
///
/// Colour is off because the Log renders text, not terminal escapes.
pub fn build(name: &str, release: bool, busy: Arc<AtomicBool>) {
    let name = name.to_string();
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("cargo");
        cmd.args(["build", "--color=never", "--example", &name]);
        if release {
            cmd.arg("--release");
        }
        // Echo it exactly as run, so the log says what happened and the
        // line can be pasted into a terminal to see it happen again.
        println!("$ {}", show(&cmd));
        match cmd.status() {
            Ok(s) if s.success() => println!("built {}", binary(&name, release).display()),
            Ok(s) => println!("build failed: cargo exited with {s}"),
            Err(e) => println!("build failed: could not run cargo: {e}"),
        }
        busy.store(false, Ordering::SeqCst);
    });
}

/// A command as it would be typed.
fn show(cmd: &std::process::Command) -> String {
    let mut out = cmd.get_program().to_string_lossy().into_owned();
    for a in cmd.get_args() {
        out.push(' ');
        out.push_str(&a.to_string_lossy());
    }
    out
}

/// Launch a built example as its own process.
///
/// Not waited on: it owns a window and a run loop, and this one has its own
/// frame to get back to.
pub fn launch(
    name: &str,
    source: &str,
    release: bool,
    out: Option<std::process::Stdio>,
    err: Option<std::process::Stdio>,
) -> Result<(), String> {
    let bin = binary(name, release);
    if !bin.is_file() {
        return Err(format!(
            "{} is not built yet -- press build first",
            bin.display()
        ));
    }
    println!("$ {}", bin.display());
    let mut cmd = std::process::Command::new(&bin);
    if let (Some(out), Some(err)) = (out, err) {
        cmd.stdout(out).stderr(err);
    }
    cmd
        // A launched example draws the editor around its own scene, so
        // pressing Play gives the same window as a Python script does rather
        // than a bare renderer. It is still a separate process with a window
        // of its own -- it links kalast as a library and cannot be hosted --
        // but it is not a different kind of thing to look at.
        //
        // An environment variable rather than an argument, because the
        // example owns its own `main` and may take arguments of its own.
        // Running the same binary from a terminal is unaffected.
        .env("KALAST_EDITOR", "1")
        // ...and which file it was built from, so its Script panel shows the
        // source of what is running rather than an empty box.
        .env("KALAST_SCRIPT", source)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not launch {}: {e}", bin.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not every example can be loaded into the editor, and the difference is
    /// whether a cdylib target was declared for it. Asking cargo for one that
    /// was not is "no example target named ...", which explains nothing.
    /// Either file of a loadable example resolves to the same cdylib, and
    /// one that owns its loop resolves to none.
    #[test]
    fn only_examples_with_a_cdylib_target_are_loadable() {
        // The bin's path, and the cdylib's own -- both are reasonable things
        // to open, and appending `_lib` blindly to the second would ask cargo
        // for `crater_main_lib_lib`.
        assert_eq!(
            dylib_for("examples/crater_self_shadow/run.rs").as_deref(),
            Some("crater_main_lib")
        );
        assert_eq!(
            dylib_for("examples/crater_self_shadow/main.rs").as_deref(),
            Some("crater_main_lib")
        );
        assert_eq!(
            dylib_for("examples/crater_self_shadow/step.rs"),
            None,
            "the one that owns its loop is a program, not a scene"
        );
        assert_eq!(dylib_for("examples/nothing/here.rs"), None);
    }

    /// `is_current` compares against the file that was *opened*, which for a
    /// loadable example may be either of its two.
    #[test]
    fn currency_is_judged_against_the_opened_file() {
        let target = dylib_for("examples/crater_self_shadow/run.rs").unwrap();
        let built = dylib_path(&target, true);
        if !built.is_file() {
            return; // nothing built here; the other tests still hold
        }
        for f in [
            "examples/crater_self_shadow/run.rs",
            "examples/crater_self_shadow/main.rs",
        ] {
            assert!(
                is_current(&target, true, f),
                "{f} is older than {}, so the built library is current for it",
                built.display()
            );
        }
    }

    #[test]
    fn an_example_is_found_by_the_path_it_was_declared_with() {
        assert_eq!(
            example_for("examples/crater_self_shadow/run.rs").as_deref(),
            Some("crater_main")
        );
        assert_eq!(
            example_for("examples/crater_self_shadow/main.rs").as_deref(),
            Some("crater_main_lib")
        );
        assert_eq!(example_for("examples/nothing/here.rs"), None);
    }
}
