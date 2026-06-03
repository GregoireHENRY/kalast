use std::{cell::RefCell, rc::Rc};

use numpy::PyArrayMethods;
use pyo3::prelude::*;

use crate::{Float, Mat4};

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Body {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub index: usize,
}

#[pymethods]
impl Body {
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

    #[getter]
    fn mesh(&self) -> Option<crate::py::mesh::Mesh> {
        self.simulation.borrow().bodies[self.index]
            .mesh
            .as_ref()
            .and_then(|m| Some(crate::py::mesh::Mesh { inner: m.clone() }))
    }
}
