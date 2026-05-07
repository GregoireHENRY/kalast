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
    fn camera(&self) -> super::frame::EyeRef {
        super::frame::EyeRef {
            simulation: self.inner.clone(),
            field: super::frame::EyeType::Camera,
        }
    }

    #[getter]
    fn sun(&self) -> super::frame::EyeRef {
        super::frame::EyeRef {
            simulation: self.inner.clone(),
            field: super::frame::EyeType::Sun,
        }
    }

    #[pyo3(signature = (
        path,
        mat=None,
        flatten=None,
    ))]
    fn load_mesh(&mut self, path: &str, mat: Option<[[Float; 4]; 4]>, flatten: Option<bool>) {
        self.inner.borrow_mut().load_mesh(
            path,
            mat.map(|m| Mat4::from_cols_array_2d(&m).transpose())
                .unwrap_or(Mat4::IDENTITY),
            flatten.unwrap_or(false),
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

    fn __repr__(&self) -> String {
        format!("{:?}", self.simulation.borrow().state)
    }
}
