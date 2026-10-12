//! The UI app's scripts' thread: Python's half of `app::script`.
//!
//! Started by the UI app's two front doors -- `python -m kalast`
//! (`App.run_editor`) and the `kalast` binary -- before the loop's first
//! turn, it waits for what the main thread asks: a script to run, the python
//! tab to serve, a callback to call. While the UI app is open Python runs
//! here and nowhere else: the main thread never takes the interpreter's
//! lock, so a script holding it -- a long loop, a slow step -- holds this
//! thread alone, and the window goes on.
//!
//! A script's `app.step()` asks the main thread for a frame and waits for it,
//! serving what comes meanwhile: its callbacks, the python tab.

use std::cell::RefCell;
use std::rc::Rc;

use pyo3::prelude::*;

use crate::app::script::{Script, ToMain, ToScript};

/// What makes the runner, `(app, source, path)`, once Python is ready on the
/// thread: the front door's own, or the one it sets up. Asked again until it
/// works, so a setup that failed says so at each try.
pub type Prepare = Box<dyn FnMut(Python<'_>, &Bound<'_, super::App>) -> PyResult<Py<PyAny>> + Send>;

thread_local! {
    /// The thread's end of the link, and the app its scripts are given: on
    /// the scripts' thread only.
    static LINK: RefCell<Option<Rc<Script>>> = const { RefCell::new(None) };
    static APP: RefCell<Option<Py<super::App>>> = const { RefCell::new(None) };
}

/// A value kept on the thread it was made on, said to be `Send` so it can
/// be handed to `Python::detach` -- whose closure runs on this same thread,
/// only with the interpreter let go.
pub struct OnThisThread<T>(pub T);

// SAFETY: only ever moved into a closure run on the thread that made it
// (`Python::detach`), never to another.
unsafe impl<T> Send for OnThisThread<T> {}

impl<T> OnThisThread<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Start the scripts' thread on `link`'s end, its app made of `handles`.
///
/// What comes back says when the thread has let go of what it made, once
/// the main thread has let go of its end of the link (`ended`).
pub fn spawn(link: Script, handles: super::Handles, prepare: Prepare) -> Ended {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("kalast-scripts".into())
        .spawn(move || {
            serve(link, handles, prepare);
            let _ = tx.send(());
        })
        .expect("spawn the scripts' thread");
    Ended(rx)
}

/// The scripts' thread, seen from the front door that started it.
pub struct Ended(std::sync::mpsc::Receiver<()>);

impl Ended {
    /// Wait, up to `limit`, for the thread to let go of what it made, once
    /// the link is dropped: the objects a script made are the thread's, and
    /// freed by another, at the interpreter's exit, each says so. A script
    /// still busy when the window closes is not waited for.
    pub fn wait(self, limit: std::time::Duration) {
        let _ = self.0.recv_timeout(limit);
    }
}

/// The next thing asked, waited for with the interpreter let go; `None` once
/// the main thread has let go of its end, or off the scripts' thread.
fn next() -> Option<ToScript> {
    LINK.with(|link| link.borrow().as_ref().and_then(|link| link.next()))
}

fn send(message: ToMain) {
    LINK.with(|link| {
        if let Some(link) = link.borrow().as_ref() {
            link.send(message);
        }
    });
}

fn serve(link: Script, handles: super::Handles, mut prepare: Prepare) {
    LINK.with(|l| *l.borrow_mut() = Some(Rc::new(link)));
    let mut runner: Option<Py<PyAny>> = None;
    // Python starts with the first thing asked, not with the window: the
    // `kalast` binary's interpreter is started by it, and an `import kalast`
    // at every launch is what that waits to cost.
    while let Some(message) = next() {
        Python::attach(|py| {
            let ready = ready(py, &handles, &mut prepare, &mut runner);
            match message {
                ToScript::Run { path, source } => {
                    let ran = ready.and_then(|(app, runner)| runner.call1(py, (app, source, path.clone())));
                    if let Err(e) = ran {
                        // The library's route, not std's `eprintln!`: in the
                        // UI app stdout is not kalast's alone, and this line
                        // is the one that says why a script did nothing.
                        crate::app::gui::engine_write(format_args!("cannot run {path}: {e}\n"), true);
                    }
                    send(ToMain::Ran);
                }
                ToScript::Console => {
                    match ready {
                        Ok((app, _)) => super::serve_console(py, app),
                        Err(e) => crate::app::gui::console_write(&format!("{e}\n")),
                    }
                    send(ToMain::Served);
                }
                ToScript::Callback { before, dt } => {
                    crate::app::App::run_callback(&handles.shared, before, &handles.simulation, dt);
                    send(ToMain::Called);
                }
                // A step's answer with no step waiting for it.
                ToScript::Frame { .. } => {}
            }
        });
    }
    // The window has closed. What the scripts made goes now, here, where it
    // was made: their callbacks, their namespaces, the thread's app -- and
    // the cycles among them, which the interpreter's last collection, on
    // the main thread, would otherwise free.
    Python::attach(|py| {
        {
            let mut shared = handles.shared.borrow_mut();
            let callbacks = (shared.before_render.take(), shared.after_render.take());
            drop(shared);
            drop(callbacks);
        }
        drop(runner.take());
        APP.with(|a| drop(a.borrow_mut().take()));
        if let Ok(editor) = py.import("kalast.editor") {
            let _ = editor.call_method0("forget_scripts");
        }
        if let Ok(gc) = py.import("gc") {
            let _ = gc.call_method0("collect");
        }
    });
}

/// The thread's app and the runner, made with the first thing asked.
fn ready(
    py: Python<'_>,
    handles: &super::Handles,
    prepare: &mut Prepare,
    runner: &mut Option<Py<PyAny>>,
) -> PyResult<(Py<super::App>, Py<PyAny>)> {
    let app = APP.with(|a| a.borrow().as_ref().map(|a| a.clone_ref(py)));
    let app = match app {
        Some(app) => app,
        None => {
            // On this thread, a GUI's windows are no matplotlib's to open:
            // the platform wants them on the main thread, and macOS takes
            // the process down over one opened elsewhere. Drawn to files,
            // then, unless the user chose otherwise.
            py.import("os")?.getattr("environ")?.call_method1("setdefault", ("MPLBACKEND", "agg"))?;
            let app = Py::new(py, super::App::remote(handles.clone()))?;
            APP.with(|a| *a.borrow_mut() = Some(app.clone_ref(py)));
            app
        }
    };
    if runner.is_none() {
        *runner = Some(prepare(py, app.bind(py))?);
    }
    Ok((app, runner.as_ref().unwrap().clone_ref(py)))
}

/// A script's `app.step()` on the scripts' thread: ask the main thread for
/// a frame and wait for it -- serving its callbacks meanwhile, and the
/// python tab -- then say whether the run goes on, as the engine's
/// `App::step` does.
pub fn step(py: Python<'_>, app: &super::App) -> bool {
    // Another run asked for, or the window closed: the loop ends here, as
    // the engine's own does.
    {
        let shared = app.shared.borrow();
        if shared.superseded() || !shared.running {
            return false;
        }
    }
    let on_thread = LINK.with(|link| link.borrow().is_some());
    if !on_thread {
        // A thread of the script's own: the window is the main thread's,
        // and only the scripts' thread steps it from elsewhere.
        crate::app::gui::engine_write(
            format_args!("app.step() from a thread of the script's own: only the script's own thread steps the UI app\n"),
            true,
        );
        return false;
    }
    send(ToMain::Step);
    loop {
        match py.detach(next) {
            // The UI app has closed.
            None => return false,
            Some(ToScript::Frame { running }) => return running,
            Some(ToScript::Console) => {
                match APP.with(|a| a.borrow().as_ref().map(|a| a.clone_ref(py))) {
                    Some(thread_app) => super::serve_console(py, thread_app),
                    None => super::serve_console(py, app.clone()),
                }
                send(ToMain::Served);
            }
            Some(ToScript::Callback { before, dt }) => {
                crate::app::App::run_callback(&app.shared, before, &app.simulation, dt);
                send(ToMain::Called);
            }
            // Never sent while a script runs; said done if it were.
            Some(ToScript::Run { .. }) => send(ToMain::Ran),
        }
    }
}
