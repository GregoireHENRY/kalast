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
    /// Held alongside `inner`, not fetched through it. `start()` borrows the
    /// app mutably for the whole run loop, so a getter that went through
    /// `inner` would panic from inside a callback -- which is exactly where
    /// changing a setting is wanted.
    pub config: Rc<RefCell<crate::app::config::Config>>,
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl App {
    #[new]
    fn new() -> Self {
        let inner = Rc::new(RefCell::new(crate::app::App::new()));
        let (config, simulation) = {
            let app = inner.borrow();
            (app.config.clone(), app.simulation.clone())
        };
        Self {
            inner,
            config,
            simulation,
        }
    }

    #[getter]
    /// Renderer and window settings. See `CONFIG.md`.
    fn config(&self) -> config::Config {
        config::Config {
            config: self.config.clone(),
            simulation: self.simulation.clone(),
        }
    }

    #[getter]
    /// The scene: bodies, camera, Sun, iteration state and HUDs.
    fn get_simulation(&self) -> simulation::Simulation {
        simulation::Simulation {
            inner: self.simulation.clone(),
        }
    }

    /// Create the window and run the render loop.
    ///
    /// **Blocks until the window closes**, so set everything up before calling it
    /// and do per-frame work in `before_render`/`after_render`.
    ///
    /// Use `step()` instead to keep the loop in your own script.
    fn start(&mut self) {
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
    fn start_editor(&mut self) {
        self.inner.borrow_mut().start_editor();
    }

    /// Install what the editor's `Run` button calls.
    ///
    /// A callable taking `(source, path)`. `kalast.editor.make_runner(app)`
    /// builds the standard one, which executes a script against *this* app
    /// rather than letting it construct a second.
    #[setter]
    /// :pytype: Callable[[str, str], None]
    fn set_script_runner(&mut self, callback: Py<PyAny>) {
        self.inner.borrow_mut().script_runner = Some(callback);
    }

    /// Put a script in the editor's buffer, and name the file it came from.
    ///
    /// Settable before `start_editor()`, which is when a launcher does it.
    fn set_script(&mut self, path: &str, source: &str) {
        self.inner
            .borrow_mut()
            .set_script(path.to_string(), source.to_string());
    }

    /// Append a line to the editor's log panel, or to stdout without one.
    fn log(&mut self, line: &str) {
        self.inner.borrow_mut().log(line);
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
    fn step(&mut self) -> bool {
        self.inner.borrow_mut().step()
    }

    /// Ask the window to close, ending a `while app.step():` loop.
    ///
    /// Takes effect on the next `step()`, not immediately: closing runs the
    /// same shutdown the window button does, which includes flushing every
    /// queued frame export to disk.
    fn close(&mut self) {
        self.inner.borrow_mut().close();
    }

    /// Whether the window is still open.
    #[getter]
    fn running(&self) -> bool {
        self.inner.borrow().is_running()
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
    fn set_before_render(&mut self, callback: Py<PyAny>) {
        let app = self.clone();
        self.inner.borrow_mut().before_render = Some(crate::app::Tick::Python { callback, app });
    }

    /// Alias for `before_render`, kept because it is what every example and
    /// existing script uses.
    ///
    /// :pytype: Callable[[App, float], None]
    #[setter]
    fn set_tick(&mut self, callback: Py<PyAny>) {
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
    fn set_after_render(&mut self, callback: Py<PyAny>) {
        let app = self.clone();
        self.inner.borrow_mut().after_render = Some(crate::app::Tick::Python { callback, app });
    }
}
