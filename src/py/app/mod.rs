pub mod body;
pub mod frame;
pub mod config;
pub mod gpu;
pub mod simulation;

use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

#[pyclass(unsendable)]
#[derive(Clone)]
pub struct App {
    pub inner: Rc<RefCell<crate::app::App>>,
    /// Everything a script may touch while the loop runs.
    ///
    /// Held beside `inner`, never through it: `start()` borrows `inner` for
    /// the whole run, so a setter reaching through it would panic.
    pub shared: Rc<RefCell<crate::app::Shared>>,
    /// The application's own settings. `App` owns this one.
    pub config: Rc<RefCell<crate::app::config::AppConfig>>,
    /// The simulation's settings. `Simulation` owns it; this is a handle.
    ///
    /// Held alongside `inner`, not fetched through it. `start()` borrows the
    /// app mutably for the whole run loop, so a getter that went through
    /// `inner` would panic from inside a callback -- which is exactly where
    /// changing a setting is wanted.
    pub sim_config: Rc<RefCell<crate::app::config::Config>>,
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl App {
    #[new]
    fn new() -> Self {
        let inner = Rc::new(RefCell::new(crate::app::App::new()));
        let (config, simulation, shared, sim_config) = {
            let app = inner.borrow();
            (
                app.config.clone(),
                app.simulation.clone(),
                app.shared.clone(),
                app.simulation.borrow().config.clone(),
            )
        };
        Self {
            inner,
            shared,
            config,
            sim_config,
            simulation,
        }
    }

    #[getter]
    /// The application's own settings: window size, and panel layout and
    /// colours as the editor grows.
    ///
    /// Not the simulation's -- that is `app.simulation.config`, and it is
    /// where shading, shadows, axes, the colour bar and export live.
    fn config(&self) -> config::AppConfig {
        config::AppConfig {
            config: self.config.clone(),
        }
    }

    #[getter]
    /// The scene: bodies, camera, Sun, iteration state and HUDs.
    fn get_simulation(&self) -> simulation::Simulation {
        simulation::Simulation {
            inner: self.simulation.clone(),
            config: self.sim_config.clone(),
        }
    }

    /// Create the window and run the render loop.
    ///
    /// **Blocks until the window closes**, so set everything up before calling it
    /// and do per-frame work in `before_render`/`after_render`.
    ///
    /// Use `step()` instead to keep the loop in your own script.
    fn start(&self) {
        self.inner.borrow_mut().start();
    }

    /// Open the editor: the renderer as one panel in a Blender-style layout.
    ///
    /// **Blocks until the window closes**, like `start()`, and runs the same
    /// loop. The difference is where the scene lands -- into an offscreen
    /// texture at the viewport panel's size, which the UI samples into the
    /// centre of the layout, rather than blitted to the swapchain.
    ///
    /// `start()` and `step()` are untouched by this: a script run from a
    /// terminal draws straight to the window as it always has.
    ///
    /// ```python
    /// app = App()
    /// app.simulation.load_mesh(path=..., mat=numpy.eye(4), flatten=True)
    /// app.start_editor()
    /// ```
    ///
    /// `python -m kalast` opens it on an empty scene.
    fn start_editor(&self) {
        self.inner.borrow_mut().start_editor();
    }

    /// Install what the editor's `Run` button calls.
    ///
    /// A callable taking `(app, source, path)`. `kalast.editor.make_runner()`
    /// builds the standard one, which executes a script against the app it is
    /// handed rather than letting it construct a second.
    ///
    /// The app arrives as an argument rather than being captured when the
    /// runner is installed: a script must reach the app through a handle of
    /// its own, not through the object whose `start_editor` is running.
    #[setter]
    /// :pytype: Callable[[str, str], None]
    fn set_script_runner(&self, callback: Py<PyAny>) {
        let app = self.clone();
        self.shared.borrow_mut().script_runner =
            Some(crate::app::ScriptRunner { callback, app });
    }

    /// Put a script in the editor's buffer, and name the file it came from.
    ///
    /// Settable before `start_editor()`, which is when a launcher does it.
    fn set_script(&self, path: &str, source: &str) {
        // Straight into `pending_script`: the editor may not exist yet, and
        // if it does the frame picks this up on its next pass.
        self.shared.borrow_mut().pending_script =
            Some((path.to_string(), source.to_string()));
    }

    /// Run the script in the editor's buffer, as the Play button does.
    ///
    /// Takes effect on the next frame, and starts the simulation. Works
    /// before `start_editor()` as well, which is how `python -m kalast
    /// script.py --run` opens straight into a running scene.
    fn run_script(&self) {
        self.shared.borrow_mut().run_requested = true;
    }

    /// Read the file named by `set_script`'s path, as the panel's `open`
    /// button does -- which also builds the scene and holds it at iteration
    /// 0, so there is something to look at and Step has something to step.
    fn open_script(&self) {
        self.shared.borrow_mut().open_requested = true;
    }

    /// Rebuild the scene from the script and stop at the start, as the
    /// Restart button does.
    ///
    /// The difference from `run_script()` is only what happens afterwards:
    /// this leaves the simulation paused.
    fn restart_script(&self) {
        self.shared.borrow_mut().restart_requested = true;
    }

    /// Open the editor and run its loop until the window closes.
    ///
    /// The loop is `App::run_editor` in the engine -- the same one the
    /// `kalast` binary runs. This is a binding onto it, not a second
    /// implementation.
    ///
    /// `args` are taken as typed on the command line: `.py` and `.rs` open in
    /// the script panel, `.obj` loads as a mesh.
    ///
    /// `run_script(app, source, path)` is called whenever Play or Restart
    /// asks for a script, and is the one piece Rust cannot do for itself --
    /// executing Python needs CPython. Everything else about the loop is in
    /// the engine.
    #[pyo3(signature = (args, run_script))]
    fn run_editor(&self, py: Python<'_>, args: Vec<String>, run_script: Py<PyAny>) -> PyResult<()> {
        // The callback takes the *Python* app, which the caller already holds,
        // rather than being handed one back: `self` here is a handle onto the
        // same `Rc<RefCell<App>>`, so a second wrapper would be a second name
        // for the thing the script is about to reconfigure.
        let this = self.clone();

        // Driven a turn at a time rather than handing the loop to
        // `run_editor`, because that would hold `inner` borrowed for the whole
        // session -- and a script is free to call back into the app, which is
        // the same `RefCell`. A `while app.step():` script did exactly that
        // and could not: the borrow was the obstacle, not the frame.
        //
        // The policy is still the engine's; only the handle is turned here.
        self.inner.borrow_mut().editor_start(&args);
        loop {
            let tick = self.inner.borrow_mut().editor_tick();
            match tick {
                crate::app::EditorTick::Closed => break,
                crate::app::EditorTick::Frame => {}
                // Nothing of ours is borrowed here. The script may step the
                // app, load meshes, reconfigure it -- all of which reach
                // `inner` -- and it runs to completion before the next turn.
                crate::app::EditorTick::Run { path, source } => {
                    run_script.call1(py, (this.clone(), source, path))?;
                }
            }
        }
        Ok(())
    }

    /// Take a script the editor's Play button has asked to run, if any.
    ///
    /// Returns `(path, source, paused)` once per request, or `None`.
    /// `paused` is true when Restart asked -- rebuild the scene and stop at
    /// the start -- and false for Play, which rebuilds and runs. Call it
    /// **between** frames and execute what comes back:
    ///
    /// ```python
    /// while app.step():
    ///     asked = app.take_script_request()
    ///     if asked:
    ///         path, source, paused = asked
    ///         app.simulation.reset()
    ///         app.simulation.state.is_paused = paused
    ///         kalast.editor.run_toplevel(app, source, path)
    /// ```
    ///
    /// The frame cannot run a script itself: one that drives its own
    /// `while app.step():` would be a loop nested inside the frame it is
    /// trying to drive. Between frames it runs as the program it is,
    /// whatever shape it has.
    fn take_script_request(&self) -> Option<(String, String, bool)> {
        self.shared.borrow_mut().script_pending.take()
    }

    /// Where the UI last saw the pointer, in egui points, or `None`.
    ///
    /// `None` is the usual state of an unfocused window: macOS delivers
    /// mouse-moved events only to the front application.
    #[getter]
    fn pointer(&self) -> Option<(f32, f32)> {
        self.shared.borrow().pointer
    }

    /// The size the pointer is measured against, in egui points.
    #[getter]
    fn ui_size(&self) -> (f32, f32) {
        self.shared.borrow().ui_size
    }

    /// Which panels the last frame drew: `(top, bottom, left, right)`.
    ///
    /// All four in the ordinary layout. With `config.focus` on, only the ones
    /// the pointer has summoned to an edge.
    #[getter]
    fn panels_shown(&self) -> (bool, bool, bool, bool) {
        let p = self.shared.borrow().panels_shown;
        (p[0], p[1], p[2], p[3])
    }

    /// The iteration the frame on screen was drawn for.
    ///
    /// Not `simulation.state.iteration`, which counts iterations *finished*:
    /// it moves at the end of a frame, so once iteration 0 has been drawn it
    /// already reads 1. This is the number the editor's toolbar shows, being
    /// the one you are actually looking at.
    #[getter]
    fn drawn_iteration(&self) -> usize {
        self.shared.borrow().drawn_iteration
    }

    /// Whether a run has been asked for and not yet taken.
    ///
    /// A peek, unlike `take_script_request`, so a script that is driving its
    /// own loop can notice the request without consuming it and unwind back
    /// to whoever owns the loop.
    #[getter]
    fn script_requested(&self) -> bool {
        self.shared.borrow().script_pending.is_some()
    }

    /// Whether the script in the editor's buffer is the one that is running.
    ///
    /// Drives the Play button: `False` and it runs the script, `True` and it
    /// is a pause toggle. A launcher that has already executed the script
    /// sets this, so Play does not offer to run it a second time.
    #[getter]
    fn script_ran(&self) -> bool {
        self.shared.borrow().script_ran
    }

    #[setter]
    fn set_script_ran(&self, v: bool) {
        self.shared.borrow_mut().script_ran = v;
    }

    /// Put stdout and stderr back and flush anything still buffered.
    ///
    /// `kalast.editor.capture_output` registers this with `atexit`, so a
    /// script's last words reach the terminal. Safe to call more than once.
    fn flush_output(&self) {
        self.inner.borrow_mut().flush_output();
    }

    /// Append a line to the editor's log panel, or to stdout without one.
    fn log(&self, line: &str) {
        self.shared.borrow_mut().log.push(line);
    }

    /// Draw one frame. Returns `False` once the window has closed.
    ///
    /// The alternative to `start()`: the loop stays in the script, so there
    /// is no callback boundary to hand state across.
    ///
    /// **`step()` goes in the middle of the loop body, not in the `while`
    /// line.**
    ///
    /// ```python
    /// sim = app.simulation
    /// while app.running:
    ///     it = sim.state.iteration
    ///     sim.bodies[0].mat = pose(et0 + it * dt)   # the before_render half
    ///     sim.request_facet_shadow(0)
    ///
    ///     if not app.step():
    ///         break
    ///
    ///     lit = sim.facet_shadow(0)                 # the after_render half
    /// ```
    ///
    /// **Where the code goes, against the callbacks.** Work written *before*
    /// `step()` is what `before_render` did -- it lands in the frame about to
    /// be drawn. Work written *after* it is what `after_render` did: the
    /// frame has rendered, so `facet_shadow()`, `facet_id_map()` and
    /// `hemicube()` answer for the scene just drawn.
    ///
    /// `while app.step():` is wrong, and wrong silently: it puts every line
    /// after the draw, so the pose you set applies to the next frame while
    /// the result you read describes the previous one. `while True:` is wrong
    /// too -- once the window closes `step()` returns at once without
    /// drawing, `state.iteration` stops advancing, and a loop keyed on it
    /// spins forever.
    ///
    /// Callbacks still run if set, inside the frame, so mixing the two works
    /// and every existing script is unaffected.
    ///
    /// **`step()` does not skip a paused frame.** It draws and returns `True`
    /// as usual, since the window must stay responsive to the key that
    /// unpauses it; what it does not do is advance `state.iteration` or run
    /// the callbacks. A driven loop that should also idle when paused has to
    /// check `sim.state.is_paused` itself.
    ///
    /// One call is one frame. The first is slower than the rest -- it creates
    /// the window and configures the surface.
    ///
    /// Not usable after `start()`: a platform event loop cannot be created
    /// twice in one process, and `start()` consumes it. Pick one.
    fn step(&self) -> bool {
        self.inner.borrow_mut().step()
    }

    /// Ask the window to close, ending a `while app.step():` loop.
    ///
    /// Takes effect on the next `step()`, not immediately: closing runs the
    /// same shutdown the window button does, which includes flushing every
    /// queued frame export to disk.
    fn close(&self) {
        self.shared.borrow_mut().exit_requested = true;
    }

    /// Whether the window is still open.
    #[getter]
    fn running(&self) -> bool {
        self.shared.borrow().running
    }

    /// Runs before each frame is drawn. Set body transforms, camera and
    /// sun here.
    ///
    /// Called as `f(app, dt)`. `dt` is the **wall-clock time since the last
    /// frame**, in seconds -- not a simulation step, so integrating physics
    /// with it ties the result to the frame rate.
    ///
    /// **Annotate the parameter** -- `def before_render(app: App, dt: float)`
    /// -- or an editor has no way to know what `app` is and completes nothing
    /// inside the body.
    ///
    /// ```python
    /// def before_render(app: App, dt: float) -> None:
    ///     sim = app.simulation
    ///     sim.huds[0].text = f"it={sim.state.iteration}  {dt * 1e3:.1f} ms"
    ///     sim.bodies[0].mat = pos_mat("MARS", "IAU_MARS", et0 + sim.state.iteration * step)
    /// ```
    ///
    /// :pytype: Callable[[App, float], None]
    #[setter]
    fn set_before_render(&self, callback: Py<PyAny>) {
        let simulation = self.get_simulation();
        self.shared.borrow_mut().before_render =
            Some(crate::app::Tick::Python { callback, simulation });
    }

    /// Alias for `before_render`, kept because it is what every example and
    /// existing script uses.
    ///
    /// :pytype: Callable[[App, float], None]
    #[setter]
    fn set_tick(&self, callback: Py<PyAny>) {
        self.set_before_render(callback);
    }

    /// Runs after each frame is drawn, when GPU results for that frame
    /// exist -- `sim.facet_shadow()` is only filled in once the shadow map
    /// holds this frame's geometry, so this is where to consume it without
    /// a one-frame lag.
    ///
    /// Scene changes made here apply to the *next* frame, and heavy CPU work
    /// here blocks the render loop (fine for a simulation run, but frame
    /// rate stops meaning much).
    ///
    /// Called as `f(app, dt)`, same shape as `before_render`.
    ///
    /// :pytype: Callable[[App, float], None]
    #[setter]
    fn set_after_render(&self, callback: Py<PyAny>) {
        let simulation = self.get_simulation();
        self.shared.borrow_mut().after_render =
            Some(crate::app::Tick::Python { callback, simulation });
    }
}
