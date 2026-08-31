use std::{cell::RefCell, rc::Rc};

use glam::Mat4;
use pyo3::prelude::*;

use crate::Float;

#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Simulation {
    pub inner: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl Simulation {
    #[getter]
    fn state(&self) -> State {
        State {
            simulation: self.inner.clone(),
        }
    }

    #[getter]
    fn bodies(&mut self) -> Vec<super::body::Body> {
        self.inner
            .borrow()
            .bodies
            .iter()
            .enumerate()
            .map(|(index, _)| super::body::Body {
                simulation: self.inner.clone(),
                index,
            })
            .collect()
    }

    #[getter]
    fn camera(&self) -> super::frame::Eye {
        super::frame::Eye {
            simulation: self.inner.clone(),
            field: super::frame::EyeField::Camera,
        }
    }

    #[getter]
    fn sun(&self) -> super::frame::Eye {
        super::frame::Eye {
            simulation: self.inner.clone(),
            field: super::frame::EyeField::Sun,
        }
    }

    /// `shadow_path` optionally names a lower-resolution mesh to render into
    /// the shadow map in place of `path`. The shadow map only decides which
    /// fragments are lit, so a coarser occluder buys performance without
    /// touching per-facet science data -- unlike loading a coarser `path`,
    /// which would invalidate facet-indexed results. Omit it (the default)
    /// to shadow with the main mesh.
    #[pyo3(signature = (
        path,
        mat=None,
        flatten=None,
        shadow_path=None,
    ))]
    fn load_mesh(
        &mut self,
        path: &str,
        mat: Option<[[Float; 4]; 4]>,
        flatten: Option<bool>,
        shadow_path: Option<&str>,
    ) {
        self.inner.borrow_mut().load_mesh_with_shadow(
            path,
            mat.map(|m| Mat4::from_cols_array_2d(&m).transpose())
                .unwrap_or(Mat4::IDENTITY),
            flatten.unwrap_or(false),
            shadow_path,
        );
    }

    // This function in Python has to clone the mesh to transfer it to Simulation.
    // The rust equivalent transfer ownership without clone.
    // This is to avoid spreading Rc<RefCell<Mesh>>.
    // Can look for an upgrade later.
    #[pyo3(signature = (
        mesh,
        mat=None,
    ))]
    fn add_mesh(&mut self, mesh: crate::py::mesh::Mesh, mat: Option<[[Float; 4]; 4]>) {
        self.inner.borrow_mut().add_mesh(
            mesh.inner.borrow().clone(),
            mat.map(|m| Mat4::from_cols_array_2d(&m).transpose())
                .unwrap_or(Mat4::IDENTITY),
        );
    }

    #[getter]
    fn export(&self) -> bool {
        self.inner.borrow().export
    }

    #[setter]
    fn set_export(&mut self, v: bool) {
        self.inner.borrow_mut().export = v;
    }

    fn update(&mut self) {
        self.inner.borrow_mut().update();
    }

    fn toggle_export(&mut self) {
        self.inner.borrow_mut().toggle_export();
    }

    fn export_once(&mut self) {
        self.inner.borrow_mut().export_once();
    }

    /// Ask for `body`'s per-facet occluded fractions, read back from the GPU
    /// shadow map after this frame renders.
    ///
    /// Request from `before_render`, read from `after_render`: the shadow map
    /// only holds this frame's geometry once it has been drawn. Reading from
    /// `before_render` instead still works but returns the *previous* frame's
    /// result.
    fn request_facet_shadow(&mut self, body: usize) {
        self.inner.borrow_mut().request_facet_shadow(body);
    }

    /// Per-facet occluded fractions for `body`, or `None` if they were not
    /// computed this frame.
    ///
    /// One entry per facet, in `Mesh.facets` order: 0.0 fully lit, 1.0 fully
    /// shadowed, quarter steps between for facets straddling a shadow
    /// boundary (4 samples per facet). `1.0 - frac` is the lit fraction.
    ///
    /// Set `app.config.access_shadow_map = True` to have every body computed
    /// each frame, then read this from `after_render`.
    #[pyo3(signature = (body=0))]
    fn facet_shadow<'py>(
        slf: pyo3::Bound<'py, Self>,
        body: usize,
    ) -> Option<pyo3::Bound<'py, numpy::PyArray1<f32>>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let sim = self_.inner.borrow();
        let v = sim.facet_shadow(body)?;
        Some(numpy::PyArray1::from_slice(py, v))
    }

    /// Ask for hemicube view factors for `facets` of `body`, this frame.
    ///
    /// Request from `before_render`, read with `hemicube` from
    /// `after_render`. This is a precompute, not a per-frame query: a full
    /// 10,000-facet matrix is minutes of GPU work and 400 MB dense. For a
    /// rigid body the self view factors are fixed in the body frame, so it is
    /// computed once per shape model and reused.
    ///
    /// `resolution` is one hemicube face; the delta form factors close to
    /// unity as `1/resolution^2`, reaching 3e-5 at 128. `batch` hemicubes are
    /// accumulated on the GPU before a readback.
    #[pyo3(signature = (body=0, facets=None, resolution=128, batch=64))]
    fn request_hemicube(
        &mut self,
        body: usize,
        facets: Option<numpy::PyReadonlyArray1<'_, u32>>,
        resolution: u32,
        batch: u32,
    ) {
        let list = match facets {
            Some(f) => f.as_slice().unwrap().to_vec(),
            None => {
                let sim = self.inner.borrow();
                let n = sim
                    .bodies
                    .get(body)
                    .and_then(|b| b.mesh.as_ref())
                    .map(|m| m.borrow().facets.len())
                    .unwrap_or(0);
                (0..n as u32).collect()
            }
        };
        self.inner
            .borrow_mut()
            .request_hemicube(body, list, resolution, batch);
    }

    /// `(view_factors, offsets)` from the last `request_hemicube`, or `None`.
    ///
    /// `view_factors` has shape `(len(facets), n_total)` over a facet index
    /// space shared by every loaded body, so one array carries self *and*
    /// mutual view factors. `offsets[b]` is where body `b` starts, so
    /// `vf[:, offsets[b]:offsets[b] + n_b]` is the block for body `b`.
    ///
    /// Rows sum to at most 1; the shortfall is the fraction radiated to
    /// space. Occlusion is included, by the other body as well as by the
    /// body's own terrain.
    fn hemicube<'py>(
        slf: pyo3::Bound<'py, Self>,
    ) -> Option<(
        pyo3::Bound<'py, numpy::PyArray2<f32>>,
        pyo3::Bound<'py, numpy::PyArray1<u32>>,
    )> {
        let py = slf.py();
        let self_ = slf.borrow();
        let sim = self_.inner.borrow();
        let (rows, n_rows, n_cols, offsets) = sim.hemicube_result()?;
        let vf = numpy::PyArray2::from_vec2(
            py,
            &rows.chunks(*n_cols).take(*n_rows).map(|r| r.to_vec()).collect::<Vec<_>>(),
        )
        .ok()?;
        Some((vf, numpy::PyArray1::from_slice(py, offsets)))
    }

    /// Ask for a facet index map from the camera's point of view this frame.
    ///
    /// Request from `before_render`, read with `facet_id_map` from
    /// `after_render`. Unlike the shadow query there is no config flag to
    /// enable it permanently: it renders the scene a second time and blocks
    /// on a readback, so it is meant for the frames a data product comes
    /// from.
    fn request_facet_id(&mut self) {
        self.inner.borrow_mut().request_facet_id();
    }

    /// `(ids, offsets)` for the last requested frame, or `None`.
    ///
    /// `ids` is `(height, width)` of `uint32`: 0 where nothing was drawn,
    /// otherwise `1 + offsets[body] + facet`. So for body `b`,
    ///
    /// ```text
    /// mask  = (ids > offsets[b]) & (ids <= offsets[b] + n_facets_b)
    /// facet = ids[mask] - offsets[b] - 1
    /// ```
    ///
    /// picks its pixels and their facet indices, in `Mesh.facets` order.
    /// Depth is resolved by the rasteriser, so a facet missing from the map
    /// is one the camera genuinely cannot see -- occluded by another body,
    /// over the limb, or outside the field of view.
    ///
    /// Only flattened meshes are drawn: the facet index comes from the vertex
    /// index, which is only meaningful when each facet owns its vertices.
    fn facet_id_map<'py>(
        slf: pyo3::Bound<'py, Self>,
    ) -> Option<(
        pyo3::Bound<'py, numpy::PyArray2<u32>>,
        pyo3::Bound<'py, numpy::PyArray1<u32>>,
    )> {
        let py = slf.py();
        let self_ = slf.borrow();
        let sim = self_.inner.borrow();
        let (pixels, offsets, w, h) = sim.facet_id_map()?;
        let arr = numpy::PyArray2::from_vec2(
            py,
            &pixels
                .chunks(*w as usize)
                .take(*h as usize)
                .map(|r| r.to_vec())
                .collect::<Vec<_>>(),
        )
        .ok()?;
        Some((arr, numpy::PyArray1::from_slice(py, offsets)))
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner.borrow())
    }
}

#[pyclass(unsendable)]
pub struct State {
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl State {
    #[getter]
    fn iteration(&self) -> usize {
        self.simulation.borrow().state.iteration
    }

    #[setter]
    fn set_iteration(&mut self, iteration: usize) {
        self.simulation.borrow_mut().state.iteration = iteration;
    }

    #[getter]
    fn is_paused(&self) -> bool {
        self.simulation.borrow().state.is_paused
    }

    #[setter]
    fn set_is_paused(&mut self, is_paused: bool) {
        self.simulation.borrow_mut().state.is_paused = is_paused;
    }

    #[getter]
    fn pause_at(&self) -> Option<usize> {
        self.simulation.borrow().state.pause_at
    }

    #[setter]
    fn set_pause_at(&mut self, pause_at: Option<usize>) {
        self.simulation.borrow_mut().state.pause_at = pause_at;
    }

    pub fn toggle_pause(&mut self) -> bool {
        self.simulation.borrow_mut().state.toggle_pause()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.simulation.borrow().state)
    }
}
