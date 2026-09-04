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
    /// Eye position, world units.
    fn pos<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.pos);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_pos(&self, v: [Float; 3]) {
        self.with_mut(|e| e.pos = v.into());
    }

    #[getter]
    /// Unit vector the eye looks along.
    ///
    /// Ignored for the Sun, whose shadow layers aim themselves from `pos`.
    fn dir<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.dir);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_dir(&self, v: [Float; 3]) {
        self.with_mut(|e| e.dir = v.into());
    }

    #[getter]
    /// Unit vector defining which way is up in the image.
    fn up<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up = v.into());
    }

    #[getter]
    /// The point the arcball orbits, and what `look_anchor` aims at.
    ///
    /// Not consulted for the Sun.
    fn anchor<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.anchor);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_anchor(&self, v: [Float; 3]) {
        self.with_mut(|e| e.anchor = v.into());
    }

    /// Body index the anchor tracks, or `None` for a fixed anchor.
    ///
    /// `camera.anchor_body = 0` keeps the anchor on body 0 as it moves.
    /// Assigning `anchor` from a body matrix instead only captures where it
    /// was at that moment.
    #[getter]
    fn anchor_body(&self) -> Option<usize> {
        self.with(|e| e.anchor_body)
    }

    #[setter]
    fn set_anchor_body(&self, v: Option<usize>) {
        self.with_mut(|e| e.anchor_body = v);
    }

    #[getter]
    /// Reference 'up' the arcball keeps the camera aligned to.
    fn up_world<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        let v = self.with(|e| e.up_world);
        v.to_array().to_pyarray(py)
    }

    #[setter]
    fn set_up_world(&self, v: [Float; 3]) {
        self.with_mut(|e| e.up_world = v.into());
    }

    #[getter]
    /// Frustum: field of view and the near/far/side planes.
    fn projection(&self) -> Projection {
        Projection {
            simulation: self.simulation.clone(),
            field: self.field,
        }
    }

    /// Whether the camera is in WASD mode.
    fn is_control_wasd(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::WASD)
    }

    /// Whether the camera is in arcball mode.
    fn is_control_arcball(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::Arcball)
    }

    /// Whether the camera ignores input.
    fn is_control_none(&self) -> bool {
        self.with(|e| e.control == crate::app::frame::Control::None)
    }

    /// Fly the camera with WASD; grabs and hides the cursor.
    fn set_control_wasd(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::WASD);
    }

    /// Orbit the camera with the pointer. The default.
    fn set_control_arcball(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::Arcball);
    }

    /// Ignore all camera input.
    ///
    /// Use this for a scripted render whose camera is placed from SPICE, so a stray
    /// drag cannot move what represents an instrument pointing. Note `T` still
    /// switches out of it.
    fn set_control_none(&self) {
        self.with_mut(|e| e.control = crate::app::frame::Control::None);
    }

    /// Cycle the control mode, as pressing `T` does.
    fn control_toggle(&self) {
        self.with_mut(|e| e.control.toggle());
    }

    /// The point the eye is looking at: `pos + dir`.
    fn target<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.with(|e| e.target()).to_array().to_pyarray(py)
    }

    /// Unit vector pointing right in the image plane.
    fn right<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        self.with(|e| e.right()).to_array().to_pyarray(py)
    }

    /// Distance from the eye to its anchor.
    fn distance_anchor(&self) -> Float {
        self.with(|e| e.distance_anchor())
    }

    /// The view matrix for this eye.
    fn lookto<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|e| e.lookto())
            .unwrap()
            .as_ref()
            .to_pyarray(py)
            .reshape((4, 4))
            .unwrap()
    }

    /// View-projection matrix, for a given aspect ratio.
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

    /// This eye's transform as a 4x4 matrix.
    fn mat<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<Float>> {
        self.with(|e| e.mat())
            .as_ref()
            .to_pyarray(py)
            .reshape((3, 3))
            .unwrap()
    }

    /// Re-orthogonalise `up` against `dir`.
    ///
    /// An `up` parallel to `dir` would otherwise normalise a zero vector and produce
    /// NaN that freezes the camera permanently.
    fn fix_up(&self) {
        self.with_mut(|e| e.fix_up());
    }

    /// Point `dir` at `anchor`, leaving the position alone.
    ///
    /// Has no effect on the Sun: its shadow layers aim themselves from `pos`.
    fn look_anchor(&self) {
        self.with_mut(|e| e.look_anchor());
    }

    /// Set `anchor` to a point *and* look at it, in one call.
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
    /// Whether this is an orthographic projection.
    fn is_orthographic(&self) -> bool {
        self.with(|p| p.mode == crate::app::frame::ProjectionMode::Orthographic)
    }

    /// Whether this is a perspective projection.
    fn is_perspective(&self) -> bool {
        self.with(|p| p.mode == crate::app::frame::ProjectionMode::Perspective)
    }

    /// Switch to an orthographic projection.
    fn set_orthographic(&self) {
        self.with_mut(|p| p.mode = crate::app::frame::ProjectionMode::Orthographic);
    }

    /// Switch to a perspective projection.
    fn set_perspective(&self) {
        self.with_mut(|p| p.mode = crate::app::frame::ProjectionMode::Perspective);
    }

    #[getter]
    /// Vertical field of view, radians.
    ///
    /// Never fitted automatically: it is a real instrument property, not something
    /// derived from the scene.
    fn fovy(&self) -> Float {
        self.with(|p| p.fovy)
    }

    #[setter]
    fn set_fovy(&self, v: Float) {
        self.with_mut(|p| p.fovy = v);
    }

    // near/far/side are Option: None (the default) fits them to the scene
    // bounds every frame, and assigning None puts a pinned plane back on
    // automatic. Use `resolved_*` to read what the fit actually chose.
    #[getter]
    /// Near plane, or `None` for automatic.
    ///
    /// Assigning a value **pins** it and defeats the per-frame fit for that plane;
    /// assign `None` to restore automatic.
    fn near(&self) -> Option<Float> {
        self.with(|p| p.near)
    }

    #[setter]
    fn set_near(&self, v: Option<Float>) {
        self.with_mut(|p| p.near = v);
    }

    #[getter]
    /// Far plane, or `None` for automatic. See `near`.
    fn far(&self) -> Option<Float> {
        self.with(|p| p.far)
    }

    #[setter]
    fn set_far(&self, v: Option<Float>) {
        self.with_mut(|p| p.far = v);
    }

    #[getter]
    /// Half-extent of an orthographic frustum, or `None` for automatic.
    ///
    /// For the Sun with per-body shadow layers, pinning this applies one extent to
    /// every layer, which defeats the per-body sizing those layers exist to provide.
    fn side(&self) -> Option<Float> {
        self.with(|p| p.side)
    }

    #[setter]
    fn set_side(&self, v: Option<Float>) {
        self.with_mut(|p| p.side = v);
    }

    #[getter]
    /// What the fit actually chose for `near` this frame.
    fn resolved_near(&self) -> Float {
        self.with(|p| p.resolved().near)
    }

    #[getter]
    /// What the fit actually chose for `far` this frame.
    fn resolved_far(&self) -> Float {
        self.with(|p| p.resolved().far)
    }

    #[getter]
    /// What the fit actually chose for `side` this frame.
    fn resolved_side(&self) -> Float {
        self.with(|p| p.resolved().side)
    }

    /// The projection matrix for a given aspect ratio.
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
