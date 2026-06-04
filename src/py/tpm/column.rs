use std::{cell::RefCell, rc::Rc};

use numpy::{PyArrayMethods, PyReadonlyArray1, ToPyArray};
use pyo3::prelude::*;

use crate::{
    Float,
    tpm::column::{Column as RsColumn, Interior as RsInterior},
};

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Column {
    pub inner: Rc<RefCell<RsColumn>>,
}

impl Column {
    pub(crate) fn from_raw(p: RsColumn) -> Self {
        Self {
            inner: Rc::new(RefCell::new(p)),
        }
    }
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

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Column {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}
