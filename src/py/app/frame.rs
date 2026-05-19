use std::{cell::RefCell, rc::Rc};

use numpy::{PyArrayMethods, ToPyArray};
use pyo3::prelude::*;

use crate::Float;

#[derive(Clone, Copy)]
pub enum EyeField {
    Camera,
    Sun,
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Eye {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub field: EyeField,
}

impl Eye {
    fn with<R>(&self, f: impl FnOnce(&crate::app::frame::Eye) -> R) -> R {
        let sim = self.simulation.borrow();
        f(match self.field {
            EyeField::Camera => &sim.camera,
            EyeField::Sun => &sim.sun,
        })
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut crate::app::frame::Eye) -> R) -> R {
        let mut sim = self.simulation.borrow_mut();
        f(match self.field {
            EyeField::Camera => &mut sim.camera,
            EyeField::Sun => &mut sim.sun,
        })
    }
}

#[pymethods]
impl Eye {
    #[getter]
    fn pos<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.pos);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_pos(&self, v: [Float; 3]) {
        self.with_mut(|e| e.pos = v.into());
    }

    #[getter]
    fn dir<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.dir);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_dir(&self, v: [Float; 3]) {
        self.with_mut(|e| e.dir = v.into());
    }

    #[getter]
    fn up<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up = v.into());
    }

    #[getter]
    fn anchor<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.anchor);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_anchor(&self, v: [Float; 3]) {
        self.with_mut(|e| e.anchor = v.into());
    }

    #[getter]
    fn up_world<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up_world);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up_world(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up_world = v.into());
    }

    #[getter]
    fn projection(&self) -> Projection {
        Projection {
            simulation: self.simulation.clone(),
            field: self.field,
        }
    }

    fn is_control_wasd(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::WASD)
    }

    fn is_control_arcball(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::Arcball)
    }

    fn is_control_none(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::None)
    }

    fn set_control_wasd(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::WASD);
    }

    fn set_control_arcball(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::Arcball);
    }

    fn set_control_none(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::None);
    }

    fn control_toggle(&self) {
        self.with_mut(|e| e.control.toggle());
    }

    fn target<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.with(|e| e.target()).to_array().to_pyarray(py)
    }

    fn right<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.with(|e| e.right()).to_array().to_pyarray(py)
    }

    fn distance_anchor(&self) -> Float {
        self.with(|e| e.distance_anchor())
    }

    fn lookto<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|e| e.lookto())
            .unwrap()
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    fn view_proj<'py>(
        &self,
        py: Python<'py>,
        aspect: Float,
    ) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|e| e.view_proj(aspect))
            .unwrap()
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    fn mat<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|e| e.mat())
            .as_ref()
            .to_pyarray(py)
            .reshape((3, 3))
            .unwrap()
    }

    fn fix_up(&self) {
        self.with_mut(|e| e.fix_up());
    }

    fn look_anchor(&self) {
        self.with_mut(|e| e.look_anchor());
    }

    fn set_target(&self, target: [Float; 3]) {
        self.with_mut(|e| e.set_target(target.into()));
    }

    fn __repr__(&self) -> String {
        self.with(|e| format!("{:?}", e))
    }
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Projection {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub field: EyeField,
}

impl Projection {
    fn with<R>(&self, f: impl FnOnce(&crate::app::frame::Projection) -> R) -> R {
        let sim = self.simulation.borrow();
        f(match self.field {
            EyeField::Camera => &sim.camera.projection,
            EyeField::Sun => &sim.sun.projection,
        })
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut crate::app::frame::Projection) -> R) -> R {
        let mut sim = self.simulation.borrow_mut();
        f(match self.field {
            EyeField::Camera => &mut sim.camera.projection,
            EyeField::Sun => &mut sim.sun.projection,
        })
    }
}

#[pymethods]
impl Projection {
    fn is_orthographic(&self) -> bool {
        self.with(|p| p.mode == crate::app::frame::ProjectionMode::Orthographic)
    }

    fn is_perspective(&self) -> bool {
        self.with(|p| p.mode == crate::app::frame::ProjectionMode::Perspective)
    }

    fn set_orthographic(&self) {
        self.with_mut(|p| p.mode = crate::app::frame::ProjectionMode::Orthographic);
    }

    fn set_perspective(&self) {
        self.with_mut(|p| p.mode = crate::app::frame::ProjectionMode::Perspective);
    }

    #[getter]
    fn fovy(&self) -> Float {
        self.with(|p| p.fovy)
    }

    #[setter]
    fn set_fovy(&self, v: Float) {
        self.with_mut(|p| p.fovy = v);
    }

    #[getter]
    fn near(&self) -> Float {
        self.with(|p| p.near)
    }

    #[setter]
    fn set_near(&self, v: Float) {
        self.with_mut(|p| p.near = v);
    }

    #[getter]
    fn far(&self) -> Float {
        self.with(|p| p.far)
    }

    #[setter]
    fn set_far(&self, v: Float) {
        self.with_mut(|p| p.far = v);
    }

    #[getter]
    fn side(&self) -> Float {
        self.with(|p| p.side)
    }

    #[setter]
    fn set_side(&self, v: Float) {
        self.with_mut(|p| p.side = v);
    }

    fn mat<'py>(&self, py: Python<'py>, aspect: Float) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|p| p.mat(aspect))
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    fn __repr__(&self) -> String {
        self.with(|p| format!("{:?}", p))
    }
}
