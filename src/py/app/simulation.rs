use std::{cell::RefCell, rc::Rc};

use glam::Mat4;
use pyo3::prelude::*;

use crate::Float;

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Simulation {
    pub inner: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl Simulation {
    #[getter]
    fn state(&self) -> State {
        State {
            simulation: self.inner.clone(),
        }
    }

    #[getter]
    fn bodies(&mut self) -> Vec<super::body::Body> {
        self.inner
            .borrow()
            .bodies
            .iter()
            .enumerate()
            .map(|(index, _)| super::body::Body {
                simulation: self.inner.clone(),
                index,
            })
            .collect()
    }

    #[getter]
    fn camera(&self) -> super::frame::Eye {
        super::frame::Eye {
            simulation: self.inner.clone(),
            field: super::frame::EyeField::Camera,
        }
    }

    #[getter]
    fn sun(&self) -> super::frame::Eye {
        super::frame::Eye {
            simulation: self.inner.clone(),
            field: super::frame::EyeField::Sun,
        }
    }

    /// `shadow_path` optionally names a lower-resolution mesh to render into
    /// the shadow map in place of `path`. The shadow map only decides which
    /// fragments are lit, so a coarser occluder buys performance without
    /// touching per-facet science data -- unlike loading a coarser `path`,
    /// which would invalidate facet-indexed results. Omit it (the default)
    /// to shadow with the main mesh.
    #[pyo3(signature = (
        path,
        mat=None,
        flatten=None,
        shadow_path=None,
    ))]
    fn load_mesh(
        &mut self,
        path: &str,
        mat: Option<[[Float; 4]; 4]>,
        flatten: Option<bool>,
        shadow_path: Option<&str>,
    ) {
        self.inner.borrow_mut().load_mesh_with_shadow(
            path,
            mat.map(|m| Mat4::from_cols_array_2d(&m).transpose())
                .unwrap_or(Mat4::IDENTITY),
            flatten.unwrap_or(false),
            shadow_path,
        );
    }

    // This function in Python has to clone the mesh to transfer it to Simulation.
    // The rust equivalent transfer ownership without clone.
    // This is to avoid spreading Rc<RefCell<Mesh>>.
    // Can look for an upgrade later.
    #[pyo3(signature = (
        mesh,
        mat=None,
    ))]
    fn add_mesh(&mut self, mesh: crate::py::mesh::Mesh, mat: Option<[[Float; 4]; 4]>) {
        self.inner.borrow_mut().add_mesh(
            mesh.inner.borrow().clone(),
            mat.map(|m| Mat4::from_cols_array_2d(&m).transpose())
                .unwrap_or(Mat4::IDENTITY),
        );
    }

    #[getter]
    fn export(&self) -> bool {
        self.inner.borrow().export
    }

    #[setter]
    fn set_export(&mut self, v: bool) {
        self.inner.borrow_mut().export = v;
    }

    fn update(&mut self) {
        self.inner.borrow_mut().update();
    }

    fn toggle_export(&mut self) {
        self.inner.borrow_mut().toggle_export();
    }

    fn export_once(&mut self) {
        self.inner.borrow_mut().export_once();
    }

    /// Ask for `body`'s per-facet occluded fractions, read back from the GPU
    /// shadow map after this frame renders.
    ///
    /// The result is not available until the frame has been drawn, so read it
    /// with `facet_shadow()` on the *next* tick. Requesting every tick gives
    /// a steady stream one tick behind the geometry that produced it.
    fn request_facet_shadow(&mut self, body: usize) {
        self.inner.borrow_mut().request_facet_shadow(body);
    }

    /// Most recent per-facet occluded fractions, or `None` if nothing has
    /// been read back yet. One entry per facet, in `Mesh.facets` order:
    /// 0.0 fully lit, 1.0 fully shadowed, and quarter steps in between for
    /// facets straddling a shadow boundary (4 samples per facet).
    #[pyo3(signature = ())]
    fn facet_shadow<'py>(
        slf: pyo3::Bound<'py, Self>,
    ) -> Option<pyo3::Bound<'py, numpy::PyArray1<f32>>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let sim = self_.inner.borrow();
        sim.facet_shadow_body?;
        Some(numpy::PyArray1::from_slice(py, &sim.facet_shadow_result))
    }

    /// Body index the last `facet_shadow()` result belongs to.
    #[getter]
    fn facet_shadow_body(&self) -> Option<usize> {
        self.inner.borrow().facet_shadow_body
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

#[pyclass(unsendable)]
pub struct State {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl State {
    #[getter]
    fn iteration(&self) -> usize {
        self.simulation.borrow().state.iteration
    }

    #[setter]
    fn set_iteration(&mut self, iteration: usize) {
        self.simulation.borrow_mut().state.iteration = iteration;
    }

    #[getter]
    fn is_paused(&self) -> bool {
        self.simulation.borrow().state.is_paused
    }

    #[setter]
    fn set_is_paused(&mut self, is_paused: bool) {
        self.simulation.borrow_mut().state.is_paused = is_paused;
    }

    #[getter]
    fn pause_at(&self) -> Option<usize> {
        self.simulation.borrow().state.pause_at
    }

    #[setter]
    fn set_pause_at(&mut self, pause_at: Option<usize>) {
        self.simulation.borrow_mut().state.pause_at = pause_at;
    }

    pub fn toggle_pause(&mut self) -> bool {
        self.simulation.borrow_mut().state.toggle_pause()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.simulation.borrow().state)
    }
}
