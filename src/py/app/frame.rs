use std::{cell::RefCell, rc::Rc};

use numpy::{PyArrayMethods, ToPyArray};
use pyo3::prelude::*;

use crate::Float;

#[derive(Clone, Copy)]
pub enum EyeType {
    Camera,
    Sun,
}

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct EyeRef {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub field: EyeType,
}

impl EyeRef {
    fn with<R>(&self, f: impl FnOnce(&crate::app::frame::Eye) -> R) -> R {
        let sim = self.simulation.borrow();
        f(match self.field {
            EyeType::Camera => &sim.camera,
            EyeType::Sun => &sim.sun,
        })
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut crate::app::frame::Eye) -> R) -> R {
        let mut sim = self.simulation.borrow_mut();
        f(match self.field {
            EyeType::Camera => &mut sim.camera,
            EyeType::Sun => &mut sim.sun,
        })
    }
}

#[pymethods]
impl EyeRef {
    // --- pos ---------------------------------------------------------------
    #[getter]
    fn pos<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.pos);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_pos(&self, v: [Float; 3]) {
        self.with_mut(|e| e.pos = v.into());
    }

    // --- dir ---------------------------------------------------------------
    #[getter]
    fn dir<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.dir);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_dir(&self, v: [Float; 3]) {
        self.with_mut(|e| e.dir = v.into());
    }

    // --- up ----------------------------------------------------------------
    #[getter]
    fn up<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up = v.into());
    }

    // --- anchor ------------------------------------------------------------
    #[getter]
    fn anchor<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.anchor);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_anchor(&self, v: [Float; 3]) {
        self.with_mut(|e| e.anchor = v.into());
    }

    // --- up_world ----------------------------------------------------------
    #[getter]
    fn up_world<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up_world);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up_world(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up_world = v.into());
    }

    // --- projection --------------------------------------------------------
    #[getter]
    fn projection(&self) -> ProjectionRef {
        ProjectionRef {
            simulation: self.simulation.clone(),
            field: self.field,
        }
    }

    // --- control -----------------------------------------------------------
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

    // --- computed ----------------------------------------------------------
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

// ---------------------------------------------------------------------------
// ProjectionRef – a reference into the projection inside camera or sun
// ---------------------------------------------------------------------------

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct ProjectionRef {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
    pub field: EyeType,
}

impl ProjectionRef {
    fn with<R>(&self, f: impl FnOnce(&crate::app::frame::Projection) -> R) -> R {
        let sim = self.simulation.borrow();
        f(match self.field {
            EyeType::Camera => &sim.camera.projection,
            EyeType::Sun => &sim.sun.projection,
        })
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut crate::app::frame::Projection) -> R) -> R {
        let mut sim = self.simulation.borrow_mut();
        f(match self.field {
            EyeType::Camera => &mut sim.camera.projection,
            EyeType::Sun => &mut sim.sun.projection,
        })
    }
}

#[pymethods]
impl ProjectionRef {
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

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Eye {
    pub inner: crate::app::frame::Eye,
}

#[pymethods]
impl Eye {
    #[getter]
    fn pos<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = &slf.borrow().inner.pos;
        let arr = ndarray::ArrayView1::from(v.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_pos(&mut self, v: [Float; 3]) {
        self.inner.pos = v.into();
    }

    #[getter]
    fn dir<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = &slf.borrow().inner.dir;
        let arr = ndarray::ArrayView1::from(v.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_dir(&mut self, v: [Float; 3]) {
        self.inner.dir = v.into();
    }

    #[getter]
    fn up<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = &slf.borrow().inner.up;
        let arr = ndarray::ArrayView1::from(v.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_up(&mut self, v: [Float; 3]) {
        self.inner.up = v.into();
    }

    #[getter]
    fn anchor<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = &slf.borrow().inner.anchor;
        let arr = ndarray::ArrayView1::from(v.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_anchor(&mut self, v: [Float; 3]) {
        self.inner.anchor = v.into();
    }

    #[getter]
    fn up_world<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = &slf.borrow().inner.up_world;
        let arr = ndarray::ArrayView1::from(v.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    #[setter]
    fn set_up_world(&mut self, v: [Float; 3]) {
        self.inner.up_world = v.into();
    }

    #[getter]
    fn projection(&self) -> Projection {
        Projection {
            inner: self.inner.projection.clone(),
        }
    }

    #[setter]
    fn set_projection(&mut self, projection: Projection) {
        self.inner.projection = projection.inner;
    }

    fn is_control_wasd(&self) -> bool {
        self.inner.control == crate::app::frame::Control::WASD
    }

    fn is_control_arcball(&self) -> bool {
        self.inner.control == crate::app::frame::Control::Arcball
    }

    fn is_control_none(&self) -> bool {
        self.inner.control == crate::app::frame::Control::None
    }

    fn set_control_wasd(&mut self) {
        self.inner.control = crate::app::frame::Control::WASD;
    }

    fn set_control_arcball(&mut self) {
        self.inner.control = crate::app::frame::Control::Arcball;
    }

    fn set_control_none(&mut self) {
        self.inner.control = crate::app::frame::Control::None;
    }

    fn control_toggle(&mut self) {
        self.inner.control.toggle()
    }

    fn target<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.inner.target().as_ref().to_pyarray(py)
    }

    fn right<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.inner.right().as_ref().to_pyarray(py)
    }

    fn distance_anchor(&self) -> Float {
        self.inner.distance_anchor()
    }

    fn lookto<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.inner
            .lookto()
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
        self.inner
            .view_proj(aspect)
            .unwrap()
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    fn mat<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.inner
            .mat()
            .as_ref()
            .to_pyarray(py)
            .reshape((3, 3))
            .unwrap()
    }

    fn fix_up(&mut self) {
        self.inner.fix_up()
    }

    fn look_anchor(&mut self) {
        self.inner.look_anchor()
    }

    fn set_target(&mut self, target: [Float; 3]) {
        self.inner.set_target(target.into());
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

// ---------------------------------------------------------------------------
// Projection – standalone owned Projection
// ---------------------------------------------------------------------------

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Projection {
    pub inner: crate::app::frame::Projection,
}

#[pymethods]
impl Projection {
    fn is_orthographic(&self) -> bool {
        self.inner.mode == crate::app::frame::ProjectionMode::Orthographic
    }

    fn is_perspective(&self) -> bool {
        self.inner.mode == crate::app::frame::ProjectionMode::Perspective
    }

    fn set_orthographic(&mut self) {
        self.inner.mode = crate::app::frame::ProjectionMode::Orthographic;
    }

    fn set_perspective(&mut self) {
        self.inner.mode = crate::app::frame::ProjectionMode::Perspective;
    }

    #[getter]
    fn fovy(&self) -> Float {
        self.inner.fovy
    }

    #[setter]
    fn set_fovy(&mut self, v: Float) {
        self.inner.fovy = v;
    }

    #[getter]
    fn near(&self) -> Float {
        self.inner.near
    }

    #[setter]
    fn set_near(&mut self, v: Float) {
        self.inner.near = v;
    }

    #[getter]
    fn far(&self) -> Float {
        self.inner.far
    }

    #[setter]
    fn set_far(&mut self, v: Float) {
        self.inner.far = v;
    }

    #[getter]
    fn side(&self) -> Float {
        self.inner.side
    }

    #[setter]
    fn set_side(&mut self, v: Float) {
        self.inner.side = v;
    }

    fn mat<'py>(&self, py: Python<'py>, aspect: Float) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.inner
            .mat(aspect)
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}
