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
//! The one difference is what happens when the Play button asks for a `.py`
//! file to run, which needs an interpreter this program does not have. A
//! `.rs` example is unaffected -- it is a separate binary that links kalast,
//! so the editor builds and launches it through cargo either way, which is
//! the normal way to work in Rust here.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut app = kalast::app::App::new();

    app.run_editor(&args, |_app, path, _source| {
        // Reported rather than ignored: pressing Play and having nothing at
        // all happen looks like a broken button.
        if path.ends_with(".py") {
            eprintln!(
                "cannot run {path} from the Rust binary: executing a Python \
                 script needs the interpreter.\n  \
                 use `python -m kalast {path}`, or open a .rs example here and \
                 press Build."
            );
        }
    });
}
