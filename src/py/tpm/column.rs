use std::{cell::RefCell, rc::Rc};

use numpy::{PyArrayMethods, PyReadonlyArray1};
use pyo3::prelude::*;

use crate::{Float, tpm::column::Column as RsColumn};

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Column {
    pub inner: Rc<RefCell<RsColumn>>,
}

#[pymethods]
impl Column {
    #[new]
    #[pyo3(signature = (z, prop, t_init))]
    fn new(z: PyReadonlyArray1<Float>, prop: super::properties::Properties, t_init: Float) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RsColumn::new(
                z.to_owned_array(),
                prop.inner.borrow().clone(),
                t_init,
            ))),
        }
    }

    #[getter]
    fn z<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        unsafe { numpy::PyArray1::borrow_from_array(&inner.borrow().z, slf.into_any()) }
    }

    #[getter]
    fn t<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        unsafe { numpy::PyArray1::borrow_from_array(&inner.borrow().t, slf.into_any()) }
    }

    #[getter]
    fn d<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        unsafe { numpy::PyArray1::borrow_from_array(&inner.borrow().d, slf.into_any()) }
    }
    
    fn clone(&self) -> Self {
        Self { inner: Rc::new(RefCell::new(self.inner.borrow().clone())) }
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}
