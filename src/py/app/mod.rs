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
    fn start(&mut self) {
        self.inner.borrow_mut().start();
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
