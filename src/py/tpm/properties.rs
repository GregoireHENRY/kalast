use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

use crate::{Float, tpm::properties::Properties as RsProperties};

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Properties {
    pub inner: Rc<RefCell<RsProperties>>,
}

impl Properties {
    pub(crate) fn from_raw(p: RsProperties) -> Self {
        Self {
            inner: Rc::new(RefCell::new(p)),
        }
    }
}

#[pymethods]
impl Properties {
    #[new]
    #[pyo3(signature = (albedo=0.0, emissivity=1.0, density=0.0, heat_capacity=0.0, thermal_inertia=0.0, conductivity=0.0, diffusivity=0.0))]
    fn new(
        albedo: Float,
        emissivity: Float,
        density: Float,
        heat_capacity: Float,
        thermal_inertia: Float,
        conductivity: Float,
        diffusivity: Float,
    ) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RsProperties {
                albedo,
                emissivity,
                density,
                heat_capacity,
                thermal_inertia,
                conductivity,
                diffusivity,
            })),
        }
    }

    #[getter]
    fn albedo(&self) -> Float {
        self.inner.borrow().albedo
    }

    #[setter]
    fn set_albedo(&self, v: Float) {
        self.inner.borrow_mut().albedo = v;
    }

    #[getter]
    fn emissivity(&self) -> Float {
        self.inner.borrow().emissivity
    }

    #[setter]
    fn set_emissivity(&self, v: Float) {
        self.inner.borrow_mut().emissivity = v;
    }

    #[getter]
    fn density(&self) -> Float {
        self.inner.borrow().density
    }

    #[setter]
    fn set_density(&self, v: Float) {
        self.inner.borrow_mut().density = v;
    }

    #[getter]
    fn heat_capacity(&self) -> Float {
        self.inner.borrow().heat_capacity
    }

    #[setter]
    fn set_heat_capacity(&self, v: Float) {
        self.inner.borrow_mut().heat_capacity = v;
    }

    #[getter]
    fn thermal_inertia(&self) -> Float {
        self.inner.borrow().thermal_inertia
    }

    #[setter]
    fn set_thermal_inertia(&self, v: Float) {
        self.inner.borrow_mut().thermal_inertia = v;
    }

    #[getter]
    fn conductivity(&self) -> Float {
        self.inner.borrow().conductivity
    }

    #[setter]
    fn set_conductivity(&self, v: Float) {
        self.inner.borrow_mut().conductivity = v;
    }

    #[getter]
    fn diffusivity(&self) -> Float {
        self.inner.borrow().diffusivity
    }

    #[setter]
    fn set_diffusivity(&self, v: Float) {
        self.inner.borrow_mut().diffusivity = v;
    }

    pub fn compute_thermal_inertia(&mut self) {
        self.inner.borrow_mut().compute_thermal_inertia();
    }

    pub fn compute_conductivity(&mut self) {
        self.inner.borrow_mut().compute_conductivity();
    }

    pub fn compute_diffusivity(&mut self) {
        self.inner.borrow_mut().compute_diffusivity();
    }

    pub fn compute_conductivity_diffusivity(&mut self) {
        self.inner.borrow_mut().compute_conductivity_diffusivity();
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

impl std::fmt::Debug for Properties {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.inner.borrow())
    }
}
