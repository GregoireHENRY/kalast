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

    /// How the surface reflects sunlight in the image: `None`, the default,
    /// for Lambert, or a law from `kalast.scattering` -- `Hapke(...)` or
    /// `LommelSeeligerLambert(...)`.
    ///
    /// Lambert makes a lit pixel `exposure * colour * cos(i)`. A law makes it
    /// `exposure * colour * pi * r(i, e, alpha) * cos(i)`: the I/F the law
    /// gives, scaled by the facet's colour. So with a law, leave the colours
    /// at 1 for the law's own albedo, or set them to a map relative to it.
    ///
    /// ```python
    /// from kalast.scattering import Hapke
    /// deimos.scattering = Hapke(w=0.068, b=0.275, c=1.0, b0=2.14, h=0.065, theta_bar=0.339)
    /// ```
    ///
    /// Reading gives a copy: assign a new law to change it.
    ///
    /// :pytype: Hapke | LommelSeeligerLambert | None
    #[getter]
    fn scattering(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(match self.simulation.borrow().bodies[self.index].scattering {
            None => None,
            Some(crate::lightcurve::Law::Hapke(h)) => Some(Py::new(py, h)?.into_any()),
            Some(crate::lightcurve::Law::Mix(m)) => Some(Py::new(py, m)?.into_any()),
        })
    }

    /// :pytype: Hapke | LommelSeeligerLambert | None
    #[setter]
    fn set_scattering(&mut self, law: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        use crate::lightcurve::Law;
        let law = match law {
            None => None,
            Some(l) if l.is_none() => None,
            Some(l) => {
                let law = if let Ok(h) = l.extract::<crate::scattering::Hapke>() {
                    h.check().map_err(pyo3::exceptions::PyValueError::new_err)?;
                    Law::Hapke(h)
                } else if let Ok(m) = l.extract::<crate::scattering::LommelSeeligerLambert>() {
                    Law::Mix(m)
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "scattering is None, kalast.scattering.Hapke or kalast.scattering.LommelSeeligerLambert",
                    ));
                };
                let w = match law {
                    Law::Hapke(h) => h.w,
                    Law::Mix(m) => m.w,
                };
                if !(0.0..=1.0).contains(&w) {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "the single-scattering albedo w is in [0, 1], got {w}"
                    )));
                }
                Some(law)
            }
        };
        self.simulation.borrow_mut().bodies[self.index].scattering = law;
        Ok(())
    }

    /// A dusty atmosphere over the surface, as the camera sees it: `None`,
    /// the default, for the bare surface, or `kalast.scattering.Atmosphere`.
    ///
    /// A pixel is then the dust's own light, scattered once and many times,
    /// plus the surface seen through the dust, lit by the beam that got
    /// through -- where the shadow map lets it -- and by the sky. Its colour
    /// is the surface's Lambert albedo for the sky's light, and with
    /// `scattering` set, the law still reflects the beam. The planet is
    /// taken as a sphere about the body's centre, `radius` in the scene's
    /// units: the defaults are Mars's, in km, at 655 nm in a clear season.
    ///
    /// ```python
    /// from kalast.scattering import Atmosphere
    /// mars.atmosphere = Atmosphere(tau=0.45)
    /// ```
    ///
    /// Reading gives a copy: assign a new one to change it.
    ///
    /// :pytype: Atmosphere | None
    #[getter]
    fn atmosphere(&self) -> Option<crate::atmosphere::Atmosphere> {
        self.simulation.borrow().bodies[self.index].atmosphere
    }

    /// :pytype: Atmosphere | None
    #[setter]
    fn set_atmosphere(&mut self, atmosphere: Option<crate::atmosphere::Atmosphere>) -> PyResult<()> {
        if let Some(a) = &atmosphere {
            a.check().map_err(pyo3::exceptions::PyValueError::new_err)?;
        }
        self.simulation.borrow_mut().bodies[self.index].atmosphere = atmosphere;
        Ok(())
    }

    /// The body's own shadows from a horizon map, worked out once on the GPU,
    /// rather than from drawing it into its own shadow layer every frame:
    /// `False` by default.
    ///
    /// For each facet, how high the terrain rises in 32 directions; a facet
    /// is lit where the Sun stands above its horizon, and with the Sun a disc
    /// (`light.sun_as_point = False`) by the part of the disc above it. The
    /// body's layer then holds only the other bodies that can shadow it.
    /// Faster for a large body seen whole -- Mars's 12.9M facets at AFC's
    /// closest approach -- at the price of a few seconds when it is first
    /// turned on and 64 bytes a facet on the GPU. Its shadows are a facet's:
    /// a facet is lit or not as its centre is, where the shadow map's edges
    /// cross facets. For a body each direction from whose centre crosses its
    /// surface once -- a planet, most asteroids -- in its own frame, z its
    /// spin axis.
    ///
    /// ```python
    /// mars.horizon_map = True
    /// ```
    #[getter]
    fn horizon_map(&self) -> bool {
        self.simulation.borrow().bodies[self.index].horizon_map
    }

    #[setter]
    fn set_horizon_map(&mut self, on: bool) {
        self.simulation.borrow_mut().bodies[self.index].horizon_map = on;
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
