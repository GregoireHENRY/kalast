pub mod body;
pub mod frame;
pub mod config;
pub mod gpu;
pub mod simulation;

use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

#[pyclass(unsendable)]
pub struct App {
    pub inner: Rc<RefCell<crate::app::App>>,
}

#[pymethods]
impl App {
    #[new]
    fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(crate::app::App::new())),
        }
    }

    #[getter]
    fn config(&self) -> config::Config {
        config::Config {
            app: self.inner.clone(),
        }
    }

    #[getter]
    fn get_simulation(&mut self) -> simulation::Simulation {
        simulation::Simulation {
            inner: self.inner.borrow_mut().simulation.clone(),
        }
    }

    fn start(&mut self) {
        self.inner.borrow_mut().start();
    }

    /// Runs before each frame is drawn. Set body transforms, camera and
    /// sun here.
    #[setter]
    fn set_before_render(&mut self, callback: Py<PyAny>) {
        self.inner.borrow_mut().before_render = Some(crate::app::Tick::Python {
            callback,
            simulation: self.get_simulation(),
        });
    }

    /// Alias for `before_render`, kept because it is what every example and
    /// existing script uses.
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
    #[setter]
    fn set_after_render(&mut self, callback: Py<PyAny>) {
        self.inner.borrow_mut().after_render = Some(crate::app::Tick::Python {
            callback,
            simulation: self.get_simulation(),
        });
    }
}
