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
    let Some(built) = modified(&binary(name, release)) else {
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
