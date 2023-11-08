use crate::prelude::*;
use crate::util;

pub use numpy::{PyArray1, PyArray2, ToPyArray};
pub use pyo3::exceptions::PyRuntimeError;
pub use pyo3::prelude::*;
pub use pyo3::types::{PyCFunction, PyList, PyType};

/*
#[derive(Debug, Clone, Copy)]
pub enum CallbackType<'a, F> {
    Rust(F),
    Python(&'a PyAny),
}

impl<'a, F> CallbackType<'a, F> {
    pub fn call<I>(&self, value: I) -> I
    where
        I: pyo3::IntoPy<PyObject> + pyo3::FromPyObject<'a> + Clone,
    {
        match self {
            Self::Rust(function) => function(value),
            Self::Python(function) => function.call1((value,)).unwrap().extract().unwrap(),
        }
    }
}
*/

#[pymodule]
fn kalast(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Material>()?;
    m.add_class::<Vertex>()?;
    m.add_class::<FaceData>()?;
    m.add_class::<IntegratedShapeModel>()?;
    m.add_class::<RawSurface>()?;
    m.add_class::<Surface>()?;
    m.add_class::<Colormap>()?;
    m.add_class::<SceneSettings>()?;
    m.add_class::<WindowSettings>()?;

    m.add_function(wrap_pyfunction!(util::python::ASTRONOMICAL_UNIT, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::AU, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::SECOND, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::MINUTE, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::HOUR, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::DAY, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::YEAR, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::SPICE_DATE_FORMAT, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::SPICE_DATE_FORMAT_FILE, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::DPR, m)?)?;
    m.add_function(wrap_pyfunction!(util::python::RPD, m)?)?;

    Ok(())
}
