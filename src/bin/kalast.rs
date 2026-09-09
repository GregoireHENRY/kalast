//! The kalast editor as a program.
//!
//! ```sh
//! cargo run --release --bin kalast                        # editor alone
//! cargo run --release --bin kalast -- examples/…/step.rs  # with an example
//! cargo run --release --bin kalast -- examples/…/main.py  # with a script
//! cargo run --release --bin kalast -- some/mesh.obj       # with a mesh
//! ```
//!
//! The same loop `python -m kalast` runs, because it is the same function:
//! `App::run_editor` lives in the engine and both front doors call it. Both
//! also run both kinds of example **in the window you are looking at**: a
//! `.rs` is compiled to a library and loaded, a `.py` is executed by an
//! interpreter embedded here.
//!
//! That interpreter is why `python` is a default feature rather than an
//! optional one. Built without it -- `--no-default-features` -- this is the
//! engine alone, with no pyo3 and no libpython, and a `.py` is handed to
//! `python -m kalast` instead of run here.
//!
//! The embedded interpreter is given **this** program's bindings, through
//! `append_to_inittab`, rather than letting it load the installed
//! `kalast/_rs` extension. Otherwise there would be two copies of the engine
//! in one process -- the script's `App` in one, this window in the other --
//! and the script would configure a simulation nothing draws.

use std::cell::RefCell;
use std::rc::Rc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // An `Rc` rather than a plain `App`, because a Python script is handed a
    // handle onto this same app and `py::App` is built from one.
    let app = Rc::new(RefCell::new(kalast::app::App::new()));

    app.borrow_mut().editor_start(&args);
    loop {
        let tick = app.borrow_mut().editor_tick();
        match tick {
            kalast::app::EditorTick::Closed => break,
            kalast::app::EditorTick::Frame => {}
            // Nothing of ours is borrowed here: the script is free to step
            // the app, load meshes and reconfigure it, and it runs to
            // completion before the next turn.
            kalast::app::EditorTick::Run { path, source } => {
                run_script(&app, &path, &source)
            }
        }
    }
}

/// Execute a `.py` against the app already on screen.
#[cfg(feature = "python")]
fn run_script(app: &Rc<RefCell<kalast::app::App>>, path: &str, source: &str) {
    use pyo3::prelude::*;

    // Before the interpreter starts, and only once: an inittab entry added
    // afterwards is never seen. `Python::attach` starts it, through pyo3's
    // `auto-initialize`.
    static READY: std::sync::Once = std::sync::Once::new();
    READY.call_once(|| {
        use kalast::py::python_module;
        pyo3::append_to_inittab!(python_module);
    });

    let handle = kalast::py::app::App::wrap(app.clone());
    let result = Python::attach(|py| -> PyResult<()> {
        // `import kalast._rs` looks for a submodule of the package, which
        // inittab's flat name is not -- so it is placed there by hand, before
        // anything imports `kalast`. The package's own `.so` is then never
        // reached, which is the point.
        let sys = py.import("sys")?;

        // The interpreter embedded here is the one pyo3 linked against, not
        // whatever virtualenv is active, so its `sys.path` has neither this
        // repository nor the environment kalast's dependencies live in.
        // Without both, `import kalast` fails on the package, and then on
        // numpy.
        let sys_path = sys.getattr("path")?;
        let mut roots: Vec<String> = vec![".".to_string()];
        if let Ok(venv) = std::env::var("VIRTUAL_ENV") {
            // `version_info` is a five-field named tuple; take the two
            // that name the directory.
            let info = sys.getattr("version_info")?;
            let (major, minor): (u8, u8) =
                (info.get_item(0)?.extract()?, info.get_item(1)?.extract()?);
            roots.push(format!("{venv}/lib/python{major}.{minor}/site-packages"));
        }
        for root in roots {
            if !sys_path.contains(&root)? {
                sys_path.call_method1("insert", (0, root))?;
            }
        }

        let modules = sys.getattr("modules")?;
        if !modules.contains("kalast._rs")? {
            let bindings = py.import("_rs")?;
            modules.set_item("kalast._rs", bindings)?;
        }

        // The same call `python -m kalast` makes: the script runs against
        // this app, with `start()` and `close()` neutralised, because the
        // editor owns the loop it is already inside.
        py.import("kalast.editor")?
            .call_method1("run_toplevel", (handle, source, path))?;
        Ok(())
    });

    if let Err(e) = result {
        eprintln!("cannot run {path}: {e}");
    }
}

/// Without an interpreter, hand the script to one.
///
/// The window closes and a Python-hosted editor opens with the script in it.
/// `KALAST_PYTHON` names the interpreter when a virtualenv is not on `PATH`.
#[cfg(not(feature = "python"))]
fn run_script(app: &Rc<RefCell<kalast::app::App>>, path: &str, _source: &str) {
    if !path.ends_with(".py") {
        return;
    }
    let interpreters = match std::env::var("KALAST_PYTHON") {
        Ok(p) if !p.is_empty() => vec![p],
        _ => vec!["python".to_string(), "python3".to_string()],
    };
    let mut tried = Vec::new();
    for exe in interpreters {
        println!("$ {exe} -m kalast {path}");
        match std::process::Command::new(&exe)
            .args(["-m", "kalast", path])
            .spawn()
        {
            Ok(_) => {
                app.borrow_mut().close();
                return;
            }
            Err(e) => tried.push(format!("{exe}: {e}")),
        }
    }
    eprintln!(
        "cannot run {path}: this build has no interpreter, and none was found \
         to hand it to ({}).\n  \
         build with the default features to run Python here, or set \
         KALAST_PYTHON.",
        tried.join("; ")
    );
}
