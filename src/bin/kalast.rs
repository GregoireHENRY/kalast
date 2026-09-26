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

// No console window on Windows: double-clicking `kalast.exe` opened one
// beside the UI app, and the UI app has its own log panel. A run started
// *from* a terminal still prints there -- `attach_parent_console` below.
#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::rc::Rc;

/// Windows only: a program built for the windows subsystem has no console,
/// so from a terminal its output would vanish. Attaching to the parent's
/// console, when there is one, puts `kalast.exe script.py` back on the
/// terminal; double-clicked from Explorer there is none, and nothing opens.
#[cfg(windows)]
fn attach_parent_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // Before any use of stdout/stderr, so their handles are looked up on
    // the attached console.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
#[cfg(not(windows))]
fn attach_parent_console() {}

/// Windows only: tell the C runtime this is a windows-subsystem program.
///
/// Rust starts a Windows program through `mainCRTStartup`, the console entry
/// point, whatever its subsystem -- so the C runtime took kalast.exe for a
/// console program and mirrored its descriptors 0-2 into the process's
/// standard handles: closing descriptor 1 set stdout to NULL, and the next
/// file to land on descriptor 1 became stdout. Shown in the bundle: with
/// descriptor 1 closed, the CSV a script opened next was stdout, and the
/// engine's `loading model` line was written into it. (Suspected, then
/// ruled out, as what emptied the kalast tab of a double-clicked kalast.exe:
/// that was the shell's `STARTF_HASSHELLDATA` -- see `gui::StdioCapture`.) A
/// windows-subsystem program's descriptors are the C runtime's own business,
/// which is what `_crt_gui_app` says. Process-wide, so it covers the Python
/// and the hosted examples running in here too.
#[cfg(windows)]
fn gui_c_runtime() {
    unsafe extern "C" {
        fn _set_app_type(app_type: i32);
    }
    const CRT_GUI_APP: i32 = 2;
    // SAFETY: a setting the C runtime's own startup code makes; first thing
    // in `main`, before anything else has touched a descriptor.
    unsafe { _set_app_type(CRT_GUI_APP) };
}
#[cfg(not(windows))]
fn gui_c_runtime() {}

fn main() {
    gui_c_runtime();
    attach_parent_console();
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Python's options -- `-c`, `-m`, `-I` -- from a tool that took a
    // bundle's `kalast.exe` for its Python, `sys.executable` being this
    // program there. Each such run opened a kalast window saying it did not
    // know what to do with the code; kalast's own flags are `--` ones.
    if args.first().is_some_and(|a| a.starts_with('-') && !a.starts_with("--")) {
        eprintln!(
            "kalast: {} is Python's option, not kalast's: kalast runs a script \
             given by its path, `kalast script.py`, and is no Python interpreter",
            args[0]
        );
        std::process::exit(2);
    }

    // Before anything resolves a relative path.
    move_into_the_bundle(&args);

    // Before the interpreter is started, which the first `Python::attach`
    // does and cannot be undone.
    #[cfg(feature = "embed")]
    point_at_the_bundled_interpreter();
    // Scripts' `sys.executable` is this program, then, which is no `python`
    // for the editor's language server to run. See `set_python`.
    #[cfg(feature = "embed")]
    kalast::app::gui::script::set_python_embedded();

    // Before anything opens a window or asks for an adapter: these modes
    // have neither, and they run on release runners with no display.
    if args.iter().any(|a| a == "--precompile") {
        std::process::exit(precompile(&args));
    }
    #[cfg(feature = "embed")]
    if args.iter().any(|a| a == "--python-check") {
        std::process::exit(python_check());
    }
    // Check for a newer release and install it, printing what the log
    // panel would show. No window either.
    if args.iter().any(|a| a == "--update") {
        std::process::exit(kalast::app::update::run_now());
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
            kalast::app::EditorTick::Console => console_serve(&app),
        }
    }

    // Not left to `Drop`, which does not run once a script has held the app:
    // its globals keep it alive through Python. Hands stdout and stderr back
    // and lets the log's reader put the last lines on the terminal.
    app.borrow_mut().flush_output();
}

/// Work from the bundle's own folder when it was double-clicked -- see
/// `kalast::app::bundle_working_dir`, which decides. Said on the terminal,
/// since a script opened afterwards has its relative paths read from here.
fn move_into_the_bundle(args: &[String]) {
    let (Ok(exe), Ok(cwd)) = (std::env::current_exe(), std::env::current_dir()) else {
        return;
    };
    let Some(dir) = exe
        .parent()
        .and_then(|exe_dir| kalast::app::bundle_working_dir(args, exe_dir, &cwd))
    else {
        return;
    };
    if std::env::set_current_dir(&dir).is_ok() {
        println!("started in {}, working in {}", cwd.display(), dir.display());
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
#[cfg(feature = "embed")]
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
#[cfg(feature = "embed")]
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
/// kalast --precompile examples/crater_self_shadow/main.rs
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
    // `--force` builds whether or not the library looks current. The release
    // workflow passes it: with the target directory restored from cache, a
    // library from a *previous* run can carry a matching fingerprint and a
    // newer mtime while the engine underneath it has moved on.
    let force = args.iter().any(|a| a == "--force");
    let (mut built, mut current, mut failed) = (0, 0, 0);
    for example in &examples {
        let source = match std::fs::read_to_string(example) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cannot read {example}: {e}");
                failed += 1;
                continue;
            }
        };
        // `is_current` first, which makes this idempotent and makes it the
        // check as well as the build: run in an assembled bundle it must
        // report everything up to date and compile nothing, and if it does
        // compile something then what was shipped is something the editor
        // would have ignored.
        if !force && kalast::app::cargo::is_current(true, example, &source) {
            println!("up to date {example}");
            current += 1;
            continue;
        }
        println!("--- {example}");
        match kalast::app::cargo::build_hosted_blocking(std::path::Path::new(example), &source, true) {
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

/// Before the embedded interpreter starts, and only once: `kalast._rs` has
/// to be in the inittab by then -- an entry added afterwards is never seen.
/// `Python::attach` starts it, through pyo3's `auto-initialize`.
#[cfg(feature = "embed")]
fn start_python() {
    static READY: std::sync::Once = std::sync::Once::new();
    READY.call_once(|| {
        use kalast::py::python_module;
        pyo3::append_to_inittab!(python_module);
    });
}

/// Make `import kalast` work in the embedded interpreter and point its
/// `sys.stdout` at the log, for whichever comes first: a script, or a line
/// typed at the console. Each step is skipped once it is done.
#[cfg(feature = "embed")]
fn prepare_python(py: pyo3::Python<'_>, app: &Rc<RefCell<kalast::app::App>>) -> pyo3::PyResult<()> {
    use pyo3::prelude::*;

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

    // `sys.stdout` and `sys.stderr` to the log's script tab, as
    // `python -m kalast` sets them: at once, where a pipe -- stdout by
    // the time this interpreter starts -- is block-buffered, and apart
    // from the engine's own output. Once: it also registers an exit hook.
    static LINE_BUFFERED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    if !LINE_BUFFERED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        py.import("kalast.editor")?
            .call_method1("capture_output", (kalast::py::app::App::wrap(app.clone()),))?;
    }
    Ok(())
}

/// Execute a `.py` against the app already on screen.
#[cfg(feature = "embed")]
fn run_script(app: &Rc<RefCell<kalast::app::App>>, path: &str, source: &str) {
    use pyo3::prelude::*;

    start_python();
    let handle = kalast::py::app::App::wrap(app.clone());
    let result = Python::attach(|py| -> PyResult<()> {
        prepare_python(py, app)?;
        // The same call `python -m kalast` makes: the script runs against
        // this app, with `start()` and `close()` neutralised, because the
        // editor owns the loop it is already inside.
        py.import("kalast.editor")?
            .call_method1("run_toplevel", (handle, source, path))?;
        Ok(())
    });

    if let Err(e) = result {
        // The library's route, not std's `eprintln!`: in the UI app stdout
        // is not kalast's alone, and this line is the one that says why a
        // script did nothing.
        kalast::app::gui::engine_write(format_args!("cannot run {path}: {e}\n"), true);
    }
}

/// The log's python tab -- lines typed, a Tab -- served by the embedded
/// interpreter among the running script's variables; see
/// `kalast.editor.serve_console`.
#[cfg(feature = "embed")]
fn console_serve(app: &Rc<RefCell<kalast::app::App>>) {
    use pyo3::prelude::*;

    start_python();
    let handle = kalast::py::app::App::wrap(app.clone());
    let result = Python::attach(|py| -> PyResult<()> {
        prepare_python(py, app)?;
        py.import("kalast.editor")?.call_method1("serve_console", (handle,))?;
        Ok(())
    });
    if let Err(e) = result {
        kalast::app::gui::console_write(&format!("{e}\n"));
    }
}

/// Without an interpreter there is nothing to run a line with.
#[cfg(not(feature = "embed"))]
fn console_serve(_app: &Rc<RefCell<kalast::app::App>>) {
    while kalast::app::gui::console_take().is_some() {
        kalast::app::gui::console_write("this build of kalast has no Python to run the line with\n");
    }
    let _ = kalast::app::gui::console_take_completion();
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
#[cfg(not(feature = "embed"))]
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
