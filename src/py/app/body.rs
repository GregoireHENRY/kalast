use std::{cell::RefCell, rc::Rc};

use numpy::PyArrayMethods;
use pyo3::prelude::*;

use crate::{Float, Mat4};

/// One body of the scene: where it is, `mat`, and its shape model, `mesh`.
///
/// A handle onto the live scene rather than a copy, so what is written
/// through it moves the body from the next frame. The mesh has the rest --
/// facets, positions, colours, per-facet data: `help(body.mesh)`.
#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Body {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub index: usize,
}

#[pymethods]
impl Body {
    /// The model matrix, 4x4, from the body's own frame to the world's.
    ///
    /// A view onto the live matrix, so `body.mat[:3, 3] = p` moves the body
    /// and `body.mat[:3, :3] = r` turns it; a whole 4x4 can be assigned too.
    #[getter]
    fn mat<'py>(slf: pyo3::Bound<'py, Self>) -> PyResult<Bound<'py, numpy::PyArray2<Float>>> {
        let self_ = slf.borrow();
        let m = &self_.simulation.borrow().bodies[self_.index].mat;
        let arr = ndarray::ArrayView1::from(m.as_ref())
            .into_shape_with_order((4, 4))
            .unwrap();
        unsafe { numpy::PyArray2::borrow_from_array(&arr, slf.into_any()).transpose() }
    }

    #[setter]
    fn set_mat(&mut self, m: [[Float; 4]; 4]) {
        self.simulation.borrow_mut().bodies[self.index].mat =
            Mat4::from_cols_array_2d(&m).transpose();
    }

    /// The shape model the renderer draws: facets, positions, colours and
    /// per-facet data -- `help(body.mesh)` for all of it.
    ///
    /// Every body a script loads or adds has one. A body built without one,
    /// which only Rust can do, raises `AttributeError` -- so that
    /// `getattr(body, "mesh", None)` still asks -- rather than giving `None`,
    /// which typed every `body.mesh.values = ...` as an error in the editor.
    #[getter]
    fn mesh(&self) -> PyResult<crate::py::mesh::Mesh> {
        self.simulation.borrow().bodies[self.index]
            .mesh
            .as_ref()
            .map(|m| crate::py::mesh::Mesh { inner: m.clone() })
            .ok_or_else(|| {
                pyo3::exceptions::PyAttributeError::new_err(format!("body {} has no mesh", self.index))
            })
    }
}
