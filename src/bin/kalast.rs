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

    // Before the interpreter is started, which the first `Python::attach`
    // does and cannot be undone.
    #[cfg(feature = "python")]
    point_at_the_bundled_interpreter();

    // Before anything opens a window or asks for an adapter: these modes
    // have neither, and they run on release runners with no display.
    if args.iter().any(|a| a == "--precompile") {
        std::process::exit(precompile(&args));
    }
    #[cfg(feature = "python")]
    if args.iter().any(|a| a == "--python-check") {
        std::process::exit(python_check());
    }

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

/// Tell the interpreter linked into this binary where its standard library
/// is, when this binary is one a release bundle shipped.
///
/// A bundle carries its own CPython, and the executable is linked against
/// *that* one -- which is what lets a `.py` run in the window already open
/// instead of being handed to some interpreter found on `PATH`. But a linked
/// interpreter finds its standard library from a prefix compiled into it,
/// and the prefix compiled in is wherever the release runner unpacked it.
/// `PYTHONHOME` is the only thing that can say otherwise, and it has to be
/// set before `Py_Initialize`, which pyo3 runs at the first
/// `Python::attach`.
///
/// Only when the bundle's interpreter is actually there: set from a clone,
/// where the linked interpreter is the developer's own and correctly
/// configured already, it would send it looking in a directory that does
/// not exist.
#[cfg(feature = "python")]
fn point_at_the_bundled_interpreter() {
    let Some(home) = kalast::app::bundled_python_dir() else {
        return;
    };
    // SAFETY: single-threaded here -- this is the first statement of `main`
    // and nothing has been spawned.
    unsafe { std::env::set_var("PYTHONHOME", &home) };
}

/// Start the embedded interpreter, import kalast through it, print what was
/// imported, and exit. No window, no adapter.
///
/// It exists for the release workflow, which cannot open a window on a
/// runner and so cannot otherwise tell whether the interpreter linked into
/// the executable actually works. That is worth checking on every platform:
/// it is a relocated CPython reached through `PYTHONHOME`, and the ways it
/// can fail -- wrong prefix, a library the loader cannot find, an
/// `abi3` mismatch -- all look like a bundle that opens fine and then does
/// nothing when handed a script.
#[cfg(feature = "python")]
fn python_check() -> i32 {
    use pyo3::prelude::*;

    static READY: std::sync::Once = std::sync::Once::new();
    READY.call_once(|| {
        use kalast::py::python_module;
        pyo3::append_to_inittab!(python_module);
    });

    let result = Python::attach(|py| -> PyResult<String> {
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        if !modules.contains("kalast._rs")? {
            modules.set_item("kalast._rs", py.import("_rs")?)?;
        }
        let kalast = py.import("kalast")?;
        let version: String = py
            .import("importlib.metadata")?
            .call_method1("version", ("kalast",))?
            .extract()?;
        let prefix: String = sys.getattr("prefix")?.extract()?;
        let file: String = kalast.getattr("__file__")?.extract()?;
        Ok(format!("kalast {version}\n  sys.prefix {prefix}\n  package   {file}"))
    });
    match result {
        Ok(what) => {
            println!("embedded interpreter OK: {what}");
            0
        }
        Err(e) => {
            eprintln!("embedded interpreter failed: {e}");
            1
        }
    }
}

/// Build the hosted library for each `.rs` named, then exit.
///
/// ```sh
/// kalast --precompile examples/crater_self_shadow/step.rs
/// ```
///
/// **This is how a release bundle arrives with its Rust examples already
/// built.** The workflow runs the executable it has just built over the
/// examples it is about to ship, so the libraries come out of the same
/// `write_wrapper`, the same feature set and the same target directory the
/// editor will look in. A second recipe written in YAML would drift from
/// this one, and the way that would show up is the editor silently
/// recompiling every example it was handed -- which is the whole thing this
/// is here to avoid.
///
/// Release rather than debug, because `rust_release` in the editor defaults
/// to true and a debug library is one it would not look for.
fn precompile(args: &[String]) -> i32 {
    let examples: Vec<&String> = args.iter().filter(|a| a.ends_with(".rs")).collect();
    if examples.is_empty() {
        eprintln!("--precompile: name at least one .rs example to build");
        return 2;
    }
    let (mut built, mut current, mut failed) = (0, 0, 0);
    for example in &examples {
        // `is_current` first, which makes this idempotent and makes it the
        // check as well as the build: run in an assembled bundle it must
        // report everything up to date and compile nothing, and if it does
        // compile something then what was shipped is something the editor
        // would have ignored.
        if kalast::app::cargo::is_current(true, example) {
            println!("up to date {example}");
            current += 1;
            continue;
        }
        println!("--- {example}");
        match kalast::app::cargo::build_hosted_blocking(std::path::Path::new(example), true) {
            Ok(path) => {
                println!("built {}", path.display());
                built += 1;
            }
            Err(e) => {
                eprintln!("cannot precompile {example}: {e}");
                failed += 1;
            }
        }
    }
    println!("precompiled: {current} up to date, {built} built, {failed} failed");
    i32::from(failed > 0)
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
///
/// **A release bundle carries its own interpreter**, with kalast and the
/// packages the examples import already installed in it, so there is nothing
/// for the user to install and nothing written to their machine on first run
/// -- see `kalast::app::bundled_python_dir`. The `python` on `PATH` is for a build that is not
/// in a bundle, and `KALAST_PYTHON` overrides both.
#[cfg(not(feature = "python"))]
fn run_script(app: &Rc<RefCell<kalast::app::App>>, path: &str, _source: &str) {
    if !path.ends_with(".py") {
        return;
    }

    let mut interpreters: Vec<String> = Vec::new();
    match std::env::var("KALAST_PYTHON") {
        // Named deliberately, so it is the *only* one tried. Falling back to
        // some other interpreter would hide a typo in the variable behind a
        // run that silently used something else.
        Ok(p) if !p.is_empty() => interpreters.push(p),
        _ => {
            let bundled = kalast::app::bundled_python_dir().map(|dir| {
                if cfg!(windows) {
                    dir.join("python.exe")
                } else {
                    dir.join("bin").join("python3")
                }
            });
            interpreters.extend(bundled.map(|p| p.to_string_lossy().into_owned()));
            interpreters.push("python".to_string());
            interpreters.push("python3".to_string());
        }
    }

    let mut tried = Vec::new();
    for exe in interpreters {
        // Ask whether the package is there before handing the script over.
        // Spawning and walking away leaves the interpreter to say "No module
        // named kalast" into a terminal the user may not be watching, after
        // this window has already closed -- and it is the wrong question
        // answered: the interpreter exists, the package is what is missing.
        match std::process::Command::new(&exe)
            .args(["-c", "import kalast"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(s) if s.success() => {}
            // Killed by a signal rather than exiting: `code()` is `None`.
            // Worth its own message because there is one realistic way to
            // get here and it is invisible otherwise -- a clone of this
            // repository as the working directory puts its own `kalast/` on
            // `sys.path` ahead of the installed one, and that package's
            // extension is linked against whichever libpython built it, so
            // loading it into a different interpreter segfaults.
            Ok(s) if s.code().is_none() => {
                tried.push(format!(
                    "{exe}: crashed importing kalast -- if the working \
                     directory is a kalast clone, its own kalast/ is \
                     shadowing the installed package"
                ));
                continue;
            }
            Ok(_) => {
                tried.push(format!("{exe}: found, but it has no kalast package"));
                continue;
            }
            Err(e) => {
                tried.push(format!("{exe}: {e}"));
                continue;
            }
        }
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
        "cannot run {path}: this build has no interpreter of its own, and no \
         usable one was found to hand it to ({}).\n  \
         A release bundle ships one beside the executable and needs nothing \
         installed; this is not one. Otherwise `pip install kalast` into the \
         interpreter you want used, and set KALAST_PYTHON to it if it is not \
         the `python` on PATH.",
        tried.join("; ")
    );
}
