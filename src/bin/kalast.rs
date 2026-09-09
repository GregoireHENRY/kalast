//! The kalast editor, without Python.
//!
//! ```sh
//! cargo run --release --bin kalast                          # editor alone
//! cargo run --release --bin kalast -- examples/…/step.rs    # with a script
//! cargo run --release --bin kalast -- some/mesh.obj         # with a mesh
//! ```
//!
//! The same loop `python -m kalast` runs, because it is the same function:
//! `App::run_editor` lives in the engine and both front doors call it.
//!
//! Both front doors also run both kinds of example, by handing the window
//! over rather than by growing a second implementation:
//!
//! - a `.rs` is a separate binary that links kalast, so the editor builds it
//!   with cargo and launches it -- the same from either door;
//! - a `.py` needs an interpreter. This program does not link one, on purpose
//!   (`cargo run --bin kalast` starts with Python off `PATH` entirely), so it
//!   spawns `python -m kalast <script>` and closes. What opens is the editor
//!   again, with the script running in it.
//!
//! Embedding CPython would avoid the second window, and would put back the
//! Python dependency this binary exists to prove is not needed.

/// Interpreters to try, in order. `KALAST_PYTHON` wins when it is set, which
/// is what a virtualenv that is not on `PATH` needs.
fn interpreters() -> Vec<String> {
    match std::env::var("KALAST_PYTHON") {
        Ok(p) if !p.is_empty() => vec![p],
        _ => vec!["python".to_string(), "python3".to_string()],
    }
}

/// Open a `.py` example in a Python-hosted editor.
///
/// Not waited on: it takes the window from here, and this process is about to
/// end. Errors are returned rather than printed so the caller can decide --
/// failing to find an interpreter must not close the editor over a script
/// that never started.
fn hand_over_to_python(path: &str) -> Result<(), String> {
    let mut tried = Vec::new();
    for exe in interpreters() {
        println!("$ {exe} -m kalast {path}");
        match std::process::Command::new(&exe)
            .args(["-m", "kalast", path])
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => tried.push(format!("{exe}: {e}")),
        }
    }
    Err(format!(
        "cannot run {path}: no Python interpreter found ({}).\n  \
         set KALAST_PYTHON to one, or run `python -m kalast {path}` yourself.",
        tried.join("; ")
    ))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut app = kalast::app::App::new();

    app.run_editor(&args, |app, path, _source| {
        if !path.ends_with(".py") {
            return;
        }
        match hand_over_to_python(path) {
            // One kalast window at a time, the same as a `.rs` example: what
            // opens is the same editor with a scene in it.
            Ok(()) => app.close(),
            // Reported rather than ignored: pressing Play and having nothing
            // happen looks like a broken button.
            Err(e) => {
                eprintln!("{e}");
                app.log(&e);
            }
        }
    });
}
