use std::{cell::RefCell, rc::Rc};

use itertools::Itertools;
use pyo3::{prelude::*, types::PyList};

use crate::{
    Float, entity::Body as RsBody, entity::Camera as RsCamera, entity::Entity as RsEntity,
    entity::Spacecraft as RsSpacecraft,
};

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Entity {
    pub inner: Rc<RefCell<RsEntity>>,
}

impl Entity {
    pub fn from_raw(p: RsEntity) -> Self {
        Self {
            inner: Rc::new(RefCell::new(p)),
        }
    }
}

#[pymethods]
impl Entity {
    #[new]
    #[pyo3(signature = (id=0, name="", frame="", label=""))]
    fn new(id: isize, name: &str, frame: &str, label: &str) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RsEntity {
                id,
                name: name.into(),
                frame: frame.into(),
                label: label.into(),
            })),
        }
    }

    #[getter]
    fn id(&self) -> isize {
        self.inner.borrow().id
    }

    #[setter]
    fn set_id(&self, id: isize) {
        self.inner.borrow_mut().id = id;
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.borrow().name.clone()
    }

    #[setter]
    fn set_name(&self, name: &str) {
        self.inner.borrow_mut().name = name.into();
    }

    #[getter]
    fn frame(&self) -> String {
        self.inner.borrow().frame.clone()
    }

    #[setter]
    fn set_frame(&self, frame: &str) {
        self.inner.borrow_mut().frame = frame.into();
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Body {
    pub inner: Rc<RefCell<RsBody>>,
}

impl Body {
    pub(crate) fn from_raw(p: RsBody) -> Self {
        Self {
            inner: Rc::new(RefCell::new(p)),
        }
    }
}

#[pymethods]
impl Body {
    #[new]
    #[pyo3(signature = (id=0, name="", frame="", label="", radii=[0.0, 0.0, 0.0], orbit_period=0.0, spin_period=0.0))]
    fn new(
        id: isize,
        name: &str,
        frame: &str,
        label: &str,
        radii: [Float; 3],
        orbit_period: Float,
        spin_period: Float,
    ) -> Self {
        let entity = RsEntity {
            id,
            name: name.into(),
            frame: frame.into(),
            label: label.into(),
        };
        Self {
            inner: Rc::new(RefCell::new(RsBody {
                entity,
                radii: radii.into(),
                orbit_period,
                spin_period,
            })),
        }
    }

    #[getter]
    fn id(&self) -> isize {
        self.inner.borrow().entity.id
    }

    #[setter]
    fn set_id(&self, id: isize) {
        self.inner.borrow_mut().entity.id = id;
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.borrow().entity.name.clone()
    }

    #[setter]
    fn set_name(&self, name: &str) {
        self.inner.borrow_mut().entity.name = name.into();
    }

    #[getter]
    fn frame(&self) -> String {
        self.inner.borrow().entity.frame.clone()
    }

    #[setter]
    fn set_frame(&self, frame: &str) {
        self.inner.borrow_mut().entity.frame = frame.into();
    }

    // Getter that allows numpy.ndarray read and write.
    #[getter]
    fn radii<'py>(slf: Bound<'py, Self>) -> Bound<'py, numpy::PyArray1<Float>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().radii;
        let arr = numpy::ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    // Setter that allows shorthand operators.
    #[setter]
    fn set_radii(&self, radii: [Float; 3]) {
        self.inner.borrow_mut().radii = radii.into();
    }

    #[getter]
    fn orbit_period(&self) -> Float {
        self.inner.borrow().orbit_period
    }

    #[setter]
    fn set_orbit_period(&self, period: Float) {
        self.inner.borrow_mut().orbit_period = period;
    }

    #[getter]
    fn spin_period(&self) -> Float {
        self.inner.borrow().spin_period
    }

    #[setter]
    fn set_spin_period(&self, period: Float) {
        self.inner.borrow_mut().spin_period = period;
    }

    pub fn radius(&self) -> Float {
        self.inner.borrow().radius()
    }

    pub fn diameter(&self) -> Float {
        self.inner.borrow().diameter()
    }

    pub fn flattening_radius(&self) -> Float {
        self.inner.borrow().flattening_radius()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

#[pyclass(from_py_object, unsendable, dict)]
#[derive(Clone)]
pub struct Camera {
    pub inner: Rc<RefCell<RsCamera>>,
}

impl Camera {
    pub(crate) fn from_raw(p: RsCamera) -> Self {
        Self {
            inner: Rc::new(RefCell::new(p)),
        }
    }
}

#[pymethods]
impl Camera {
    #[new]
    #[pyo3(signature = (id=0, name="", frame="", label="", px=[0, 0], fovy=0.0, filters=[].to_vec()))]
    fn new(
        id: isize,
        name: &str,
        frame: &str,
        label: &str,
        px: [usize; 2],
        fovy: Float,
        filters: Vec<String>,
    ) -> Self {
        let entity = RsEntity {
            id,
            name: name.into(),
            frame: frame.into(),
            label: label.into(),
        };
        Self {
            inner: Rc::new(RefCell::new(RsCamera {
                entity,
                px: px.into(),
                fovy,
                filters,
            })),
        }
    }

    #[getter]
    fn id(&self) -> isize {
        self.inner.borrow().entity.id
    }

    #[setter]
    fn set_id(&self, id: isize) {
        self.inner.borrow_mut().entity.id = id;
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.borrow().entity.name.clone()
    }

    #[setter]
    fn set_name(&self, name: &str) {
        self.inner.borrow_mut().entity.name = name.into();
    }

    #[getter]
    fn frame(&self) -> String {
        self.inner.borrow().entity.frame.clone()
    }

    #[setter]
    fn set_frame(&self, frame: &str) {
        self.inner.borrow_mut().entity.frame = frame.into();
    }

    #[getter]
    fn label(&self) -> String {
        self.inner.borrow().entity.label.clone()
    }

    #[setter]
    fn set_label(&self, label: &str) {
        self.inner.borrow_mut().entity.label = label.into();
    }

    // Getter that allows numpy.ndarray read and write.
    #[getter]
    fn px<'py>(slf: Bound<'py, Self>) -> Bound<'py, numpy::PyArray1<usize>> {
        let inner = &slf.borrow().inner;
        let slice = &inner.borrow().px;
        let arr = numpy::ndarray::ArrayView1::from(slice.as_ref());
        unsafe { numpy::PyArray1::borrow_from_array(&arr, slf.into_any()) }
    }

    // Setter that allows shorthand operators.
    #[setter]
    fn set_px(&self, px: [usize; 2]) {
        self.inner.borrow_mut().px = px.into();
    }

    #[getter]
    fn fovy(&self) -> Float {
        self.inner.borrow().fovy
    }

    #[setter]
    fn set_fovy(&self, fovy: Float) {
        self.inner.borrow_mut().fovy = fovy;
    }

    pub fn npx(&self) -> usize {
        self.inner.borrow().npx()
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

#[pyclass(unsendable, dict)]
pub struct Spacecraft {
    pub entity: Entity,

    // Vec<String>
    #[pyo3(get, set)]
    pub id_cameras: Py<PyList>,
}

impl Spacecraft {
    pub fn from_raw(py: Python, p: RsSpacecraft) -> Self {
        Self {
            entity: Entity::from_raw(p.entity),
            id_cameras: PyList::new(py, p.id_cameras).unwrap().into(),
        }
    }
}

#[pymethods]
impl Spacecraft {
    #[new]
    #[pyo3(signature = (id=0, name="", frame="", label="", id_cameras=vec![]))]
    fn new<'py>(
        py: Python<'py>,
        id: isize,
        name: &str,
        frame: &str,
        label: &str,
        id_cameras: Vec<String>,
    ) -> Self {
        let entity = Entity::new(id, name.into(), frame.into(), label.into());
        Self {
            entity,
            id_cameras: PyList::new(py, id_cameras).unwrap().into(),
        }
    }

    #[getter]
    fn id(&self) -> isize {
        self.entity.inner.borrow().id
    }

    #[setter]
    fn set_id(&self, id: isize) {
        self.entity.inner.borrow_mut().id = id;
    }

    #[getter]
    fn name(&self) -> String {
        self.entity.inner.borrow().name.clone()
    }

    #[setter]
    fn set_name(&self, name: &str) {
        self.entity.inner.borrow_mut().name = name.into();
    }

    #[getter]
    fn frame(&self) -> String {
        self.entity.inner.borrow().frame.clone()
    }

    #[setter]
    fn set_frame(&self, frame: &str) {
        self.entity.inner.borrow_mut().frame = frame.into();
    }

    pub fn __repr__(&self, py: Python) -> String {
        let e = self.entity.inner.borrow();
        format!(
            "Spacecraft(id={}, name={}, frame={}, label={}, id_cameras=[{}])",
            e.id,
            e.name,
            e.frame,
            e.label,
            self.id_cameras.bind(py).iter().join(", ")
        )
    }
}
