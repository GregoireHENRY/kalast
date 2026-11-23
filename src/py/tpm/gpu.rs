use numpy::prelude::*;
use pyo3::prelude::*;
use std::sync::Arc;

/// A GPU device shared by the compute stages.
///
/// Build one and hand it to both `GpuTpm` and `GpuRadiance` and the
/// temperatures never leave the GPU. Build them separately and each gets its
/// own device, which still works but pays a round trip through host memory.
#[pyo3::pyclass(from_py_object, name = "GpuContext", module = "kalast._rs.tpm.gpu")]
#[derive(Clone)]
pub struct GpuContext {
    pub(crate) inner: Arc<crate::gpu::Context>,
}

#[pyo3::pymethods]
impl GpuContext {
    #[new]
    fn new() -> PyResult<Self> {
        crate::gpu::Context::new()
            .map(|inner| Self { inner })
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }
}

/// The thermophysical model stepped on the GPU, one column per facet.
///
/// The state stays resident between steps, so a spin-up costs one upload, N
/// dispatches and one download rather than moving arrays every step.
///
/// Everything is f32: WGSL has no f64, while the numpy path is float64. The
/// difference is measured in `examples/analytical/tpm_gpu_vs_cpu.py` rather
/// than assumed away -- at most 0.00012 K on the surface, against a 0.1 K
/// Newton threshold.
#[pyo3::pyclass(name = "GpuTpm", module = "kalast._rs.tpm.gpu")]
pub struct GpuTpm {
    inner: crate::tpm::gpu::GpuTpm,
}

#[pyo3::pymethods]
impl GpuTpm {
    /// `coef_lo`/`coef_hi` are what `routine.nonuniform_coefficients` returns,
    /// `diffusivity` is per node, and the rest is the radiative surface
    /// boundary: `se = emissivity * sigma`, the conductivity, and `2 dz`.
    ///
    /// Pass `context` to share a device with `GpuRadiance`; omit it and this
    /// creates its own.
    #[new]
    #[pyo3(signature = (n_facets, coef_lo, coef_hi, diffusivity, se, conductivity,
                        twodz, newton_threshold=0.1, newton_max_iter=100,
                        context=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        n_facets: u32,
        coef_lo: numpy::PyReadonlyArray1<'_, f32>,
        coef_hi: numpy::PyReadonlyArray1<'_, f32>,
        diffusivity: numpy::PyReadonlyArray1<'_, f32>,
        se: f32,
        conductivity: f32,
        twodz: f32,
        newton_threshold: f32,
        newton_max_iter: u32,
        context: Option<GpuContext>,
    ) -> PyResult<Self> {
        let ctx = match context {
            Some(c) => c.inner,
            None => crate::gpu::Context::new()
                .map_err(pyo3::exceptions::PyRuntimeError::new_err)?,
        };
        crate::tpm::gpu::GpuTpm::new(
            ctx,
            n_facets,
            coef_lo.as_slice()?,
            coef_hi.as_slice()?,
            diffusivity.as_slice()?,
            se,
            conductivity,
            twodz,
            newton_threshold,
            newton_max_iter,
        )
        .map(|inner| Self { inner })
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    #[getter]
    fn n_facets(&self) -> u32 {
        self.inner.n_facets()
    }

    #[getter]
    fn n_nodes(&self) -> u32 {
        self.inner.n_nodes()
    }

    /// Which ping-pong buffer is live. `GpuRadiance.compute` needs it.
    #[getter]
    fn front(&self) -> usize {
        self.inner.front()
    }

    /// Upload the whole state, `(n_facets, n_nodes)`.
    fn upload(&mut self, state: numpy::PyReadonlyArray2<'_, f32>) -> PyResult<()> {
        let a = state.as_array();
        let owned;
        let flat = match a.as_slice() {
            Some(s) => s,
            None => {
                owned = a.iter().copied().collect::<Vec<f32>>();
                &owned
            }
        };
        self.inner
            .upload(flat)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Facet centres and normals in the body frame, `(n_facets, 3)` each.
    ///
    /// Static, so call it once. Needed by `step_sun`, which is what keeps the
    /// boundary flux off the CPU -- the numpy path re-streamed 151 MB of these
    /// every step at 3.1M facets.
    fn set_geometry(
        &mut self,
        positions: numpy::PyReadonlyArray2<'_, f32>,
        normals: numpy::PyReadonlyArray2<'_, f32>,
    ) -> PyResult<()> {
        let flat = |a: &numpy::PyReadonlyArray2<'_, f32>| -> Vec<f32> {
            a.as_array().iter().copied().collect()
        };
        self.inner
            .set_geometry(&flat(&positions), &flat(&normals))
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// A heat flow `q` (W/m2) through the base of every column, from below:
    /// the base node sits `q spacing / conductivity` above the node over it,
    /// `spacing` the distance between the two (m). Zero, the default, is the
    /// adiabatic bottom. The CPU's `kalast.tpm.core.bottom_flux`.
    fn set_bottom_flux(&mut self, q: f32, spacing: f32) -> PyResult<()> {
        self.inner
            .set_bottom_flux(q, spacing)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Lit fraction per facet, 1 where fully lit. Only for runs with
    /// shadowing; it stays at 1 otherwise.
    fn set_lit(&mut self, lit: numpy::PyReadonlyArray1<'_, f32>) -> PyResult<()> {
        self.inner
            .set_lit(lit.as_slice()?)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// One timestep with the boundary flux computed on the GPU.
    ///
    /// `sun_pos` is the Sun in this body's frame in metres, and
    /// `absorbed_at_1au` the solar constant times `1 - albedo`. Nothing
    /// per-facet crosses the bus: a step uploads three floats.
    #[pyo3(signature = (sun_pos, absorbed_at_1au, au))]
    fn step_sun(
        &mut self,
        sun_pos: [f32; 3],
        absorbed_at_1au: f32,
        au: f32,
    ) -> PyResult<()> {
        self.inner
            .step_sun(sun_pos, absorbed_at_1au, au)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// One timestep. `flux` is absorbed flux per facet in W/m2 -- insolation
    /// and radiative heating together, whatever the boundary sees.
    ///
    /// Does not block: the dispatch is queued and the state stays on the GPU.
    fn step(&mut self, flux: numpy::PyReadonlyArray1<'_, f32>) -> PyResult<()> {
        self.inner
            .step(flux.as_slice()?)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// The whole state, `(n_facets, n_nodes)`. Blocks on a readback, so take
    /// it at snapshots rather than every step.
    fn download<'py>(
        slf: pyo3::Bound<'py, Self>,
    ) -> PyResult<pyo3::Bound<'py, numpy::PyArray2<f32>>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let (n, m) = (
            self_.inner.n_facets() as usize,
            self_.inner.n_nodes() as usize,
        );
        // Reshaped from one flat array rather than built as rows: at 3.1M
        // facets `Vec<Vec<f32>>` means 3.1M allocations, and it dominated the
        // readback until it was measured.
        let flat = self_.inner.download();
        numpy::PyArray1::from_slice(py, &flat)
            .reshape([n, m])
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Surface temperature per facet. Also a blocking readback.
    fn surface<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<f32>> {
        let py = slf.py();
        let self_ = slf.borrow();
        numpy::PyArray1::from_slice(py, &self_.inner.surface())
    }
}

/// The thermal columns stepped on the GPU by the scheme of one's choice:
/// the GPU twin of `kalast.tpm.core.Stepper`, with what it takes -- one
/// material or a `Ground`, layered, graded, changing with temperature; a
/// heat flow at the base; the same five schemes.
///
/// ```python
/// gpu = GpuStepper(ground, layers, facets, dz, scheme="bdf2")
/// gpu.upload(t)                       # (layers, facets), as core.columns makes them
/// for _ in range(steps):
///     gpu.step(flux, dt)              # flux: W/m2 absorbed, one a facet, float32
/// t = gpu.download()                  # a readback: for a result, not every step
/// ```
///
/// The temperatures stay on the GPU between steps. The two differ in how
/// fast they run and in rounding: on a graded ground with the Moon's laws
/// the GPU is within 0.003 K of the CPU by every scheme.
///
/// A step past what the explicit scheme or RKC reaches is refused from the
/// ground's properties as given; with `radiative_conductivity` or
/// `heat_capacity_polynomial` set, the limit is the script's to keep
/// (`ground.stability_maxdt(dz, temperatures=...)`): the GPU cannot refuse
/// a step.
///
/// Pass `context` to share a device with `GpuRadiance`.
#[pyo3::pyclass(name = "GpuStepper", module = "kalast._rs.tpm.gpu")]
pub struct GpuStepper {
    inner: crate::tpm::gpu_stepper::GpuStepper,
}

/// `prop` as the core reads it, for the length of `with`: a `Ground`'s
/// arrays borrowed, or the body's `Properties`.
fn with_thermal<R>(
    prop: &Bound<'_, PyAny>,
    with: impl FnOnce(crate::tpm::core::Thermal<'_>) -> anyhow::Result<R>,
) -> PyResult<R> {
    use crate::tpm::core::py::{Ground, body, value_error};
    if let Ok(g) = prop.extract::<PyRef<'_, Ground>>() {
        let arrays = g.readonly(prop.py())?;
        return with(arrays.view().into()).map_err(value_error);
    }
    let prop = body(prop)?;
    let inner = prop.inner.borrow();
    with((&*inner).into()).map_err(value_error)
}

#[pyo3::pymethods]
impl GpuStepper {
    /// The columns of a body of `facets` facets, `layers` deep and `dz` (m)
    /// apart, with the properties of `prop`, stepped by `scheme` (`stages`
    /// for `"rkc"`). `upload` gives them their temperatures.
    #[new]
    #[pyo3(signature = (prop: "Properties | Ground", layers, facets, dz, scheme="explicit", stages=1,
                        newton_threshold=None, newton_iterations=None, context=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        prop: &Bound<'_, PyAny>,
        layers: usize,
        facets: usize,
        dz: crate::Float,
        scheme: &str,
        stages: usize,
        newton_threshold: Option<crate::Float>,
        newton_iterations: Option<usize>,
        context: Option<GpuContext>,
    ) -> PyResult<Self> {
        use crate::tpm::core::Newton;
        let value_error = crate::tpm::core::py::value_error;
        let scheme = crate::tpm::stepper::Scheme::named(scheme, stages).map_err(value_error)?;
        let default = Newton::default();
        // A thread of the GPU iterates alone: a hundred is past any balance
        // that converges.
        let newton = Newton {
            threshold: newton_threshold.unwrap_or(default.threshold),
            iterations: newton_iterations.unwrap_or(100),
        };
        let ctx = match context {
            Some(c) => c.inner,
            None => crate::gpu::Context::new().map_err(pyo3::exceptions::PyRuntimeError::new_err)?,
        };
        with_thermal(prop, |thermal| crate::tpm::gpu_stepper::GpuStepper::new(ctx, layers, facets, dz, thermal, scheme, newton))
            .map(|inner| Self { inner })
    }

    /// The scheme's name.
    #[getter]
    fn scheme(&self) -> &'static str {
        self.inner.scheme().name()
    }

    /// The stages of a step: those of `"rkc"`, 1 for any other scheme.
    #[getter]
    fn stages(&self) -> usize {
        match self.inner.scheme() {
            crate::tpm::stepper::Scheme::Rkc { stages } => stages,
            _ => 1,
        }
    }

    #[getter]
    fn layers(&self) -> usize {
        self.inner.dim().0
    }

    #[getter]
    fn facets(&self) -> usize {
        self.inner.dim().1
    }

    /// The ground's properties sent again, after they were changed: a
    /// `Properties` or a `Ground` of the same columns.
    #[pyo3(signature = (prop: "Properties | Ground"))]
    fn set_properties(&mut self, prop: &Bound<'_, PyAny>) -> PyResult<()> {
        with_thermal(prop, |thermal| self.inner.set_properties(thermal))
    }

    /// The heat flow that enters the base of every column from below
    /// (W/m2): 0, the default, for none. The Moon's is 0.018.
    fn set_bottom_flux(&mut self, q: crate::Float) -> PyResult<()> {
        self.inner.set_bottom_flux(q).map_err(crate::tpm::core::py::value_error)
    }

    /// The temperatures, `(layers, facets)` as `core.columns` makes them.
    /// What the steps before left over is forgotten.
    fn upload(&mut self, t: numpy::PyReadonlyArray2<'_, crate::Float>) -> PyResult<()> {
        self.inner.upload(t.as_array()).map_err(crate::tpm::core::py::value_error)
    }

    /// The temperatures, `(layers, facets)`. Blocks on a readback: for a
    /// result, not for every step.
    fn download<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray2<crate::Float>> {
        self.inner.download().into_pyarray(py)
    }

    /// The surface temperatures, one a facet. Also a blocking readback.
    fn surface<'py>(&self, py: Python<'py>) -> pyo3::Bound<'py, numpy::PyArray1<crate::Float>> {
        numpy::PyArray1::from_vec(py, self.inner.surface())
    }

    /// Facet centres and normals in the body's own frame, `(facets, 3)`
    /// each, for `step_sun`. Once: they do not change.
    fn set_geometry(&mut self, positions: numpy::PyReadonlyArray2<'_, f32>, normals: numpy::PyReadonlyArray2<'_, f32>) -> PyResult<()> {
        let flat = |a: &numpy::PyReadonlyArray2<'_, f32>| -> Vec<f32> { a.as_array().iter().copied().collect() };
        self.inner.set_geometry(&flat(&positions), &flat(&normals)).map_err(crate::tpm::core::py::value_error)
    }

    /// The lit fraction of each facet, 1 where fully lit, for `step_sun`.
    fn set_lit(&mut self, lit: numpy::PyReadonlyArray1<'_, f32>) -> PyResult<()> {
        self.inner.set_lit(lit.as_slice()?).map_err(crate::tpm::core::py::value_error)
    }

    /// One step of `dt` (s): each surface from the `flux` it absorbs
    /// (W/m2, one a facet), the layers, the base. Does not block.
    fn step(&mut self, flux: numpy::PyReadonlyArray1<'_, f32>, dt: crate::Float) -> PyResult<()> {
        self.inner.step(flux.as_slice()?, dt).map_err(crate::tpm::core::py::value_error)
    }

    /// `step` with each surface's `temperature` (K) given in place of a
    /// flux: for a test against a closed form.
    fn step_surface(&mut self, temperature: numpy::PyReadonlyArray1<'_, f32>, dt: crate::Float) -> PyResult<()> {
        self.inner.step_surface(temperature.as_slice()?, dt).map_err(crate::tpm::core::py::value_error)
    }

    /// One step of `dt` (s) with the sunlight each facet absorbs computed
    /// on the GPU, `S (1 - A) cos(i) lit / r**2`: `sun_pos` is the Sun in
    /// the body's own frame (m) and `au` the astronomical unit (m). Each
    /// facet's albedo is the ground's. Needs `set_geometry`; nothing a
    /// facet crosses to the GPU at a step.
    fn step_sun(&mut self, sun_pos: [f32; 3], au: f32, dt: crate::Float) -> PyResult<()> {
        self.inner.step_sun(sun_pos, au, dt).map_err(crate::tpm::core::py::value_error)
    }

    /// Forget the steps before: BDF2 starts again with a backward-Euler
    /// step. `upload` does it.
    fn reset(&mut self) {
        self.inner.reset();
    }

    fn __repr__(&self) -> String {
        let (layers, facets) = self.inner.dim();
        format!("GpuStepper('{}', {layers} layers, {facets} facets)", self.inner.scheme().name())
    }
}

/// Band-integrated radiance from facet temperatures, on the GPU.
///
/// Independent of where the temperatures came from, which is the point: the
/// model's two stages each run on the CPU or the GPU as the caller likes.
///
/// ```text
/// CPU TPM -> GPU radiance :  set_temperatures(t)
/// GPU TPM -> GPU radiance :  bind_tpm(tpm)      # nothing moves
/// GPU TPM -> CPU radiance :  bands[f](tpm.surface())
/// CPU TPM -> CPU radiance :  bands[f](t)
/// ```
#[pyo3::pyclass(name = "GpuRadiance", module = "kalast._rs.tpm.gpu")]
pub struct GpuRadiance {
    inner: crate::tpm::radiance::GpuRadiance,
}

#[pyo3::pymethods]
impl GpuRadiance {
    /// `tables` is `(n_bands, n_table)` of band-integrated radiance over
    /// temperatures `t_min .. t_max` -- `BandRadiance.l_table` per filter.
    #[new]
    #[pyo3(signature = (n_facets, tables, t_min, t_max, context=None))]
    fn new(
        n_facets: u32,
        tables: numpy::PyReadonlyArray2<'_, f32>,
        t_min: f32,
        t_max: f32,
        context: Option<GpuContext>,
    ) -> PyResult<Self> {
        let ctx = match context {
            Some(c) => c.inner,
            None => crate::gpu::Context::new()
                .map_err(pyo3::exceptions::PyRuntimeError::new_err)?,
        };
        let a = tables.as_array();
        let n_bands = a.shape()[0] as u32;
        let flat: Vec<f32> = a.iter().copied().collect();
        crate::tpm::radiance::GpuRadiance::new(ctx, n_facets, n_bands, &flat, t_min, t_max)
            .map(|inner| Self { inner })
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    #[getter]
    fn n_facets(&self) -> u32 {
        self.inner.n_facets()
    }

    #[getter]
    fn n_bands(&self) -> u32 {
        self.inner.n_bands()
    }

    /// Read temperatures straight out of a `GpuTpm` built on the same
    /// context. Zero copy: nothing crosses to host memory.
    fn bind_tpm(&mut self, tpm: PyRef<'_, GpuTpm>) -> PyResult<()> {
        self.inner
            .bind_tpm(&tpm.inner)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Upload surface temperatures, one per facet. For a CPU-stepped model.
    fn set_temperatures(&mut self, t: numpy::PyReadonlyArray1<'_, f32>) -> PyResult<()> {
        self.inner
            .set_temperatures(t.as_slice()?)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    /// Radiance per facet and band, `(n_facets, n_bands)`.
    ///
    /// `front` is `tpm.front` when bound to a TPM, and ignored otherwise.
    #[pyo3(signature = (front=0))]
    fn compute<'py>(
        slf: pyo3::Bound<'py, Self>,
        front: usize,
    ) -> PyResult<pyo3::Bound<'py, numpy::PyArray2<f32>>> {
        let py = slf.py();
        let self_ = slf.borrow();
        let nb = self_.inner.n_bands() as usize;
        let nf = self_.inner.n_facets() as usize;
        let flat = self_.inner.compute(front);
        numpy::PyArray1::from_slice(py, &flat)
            .reshape([nf, nb])
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// What facets send to one another, on the GPU: the twin of
/// `kalast.tpm.exchange.absorbed`, for one set of view factors and surfaces.
///
/// ```python
/// gpu = GpuExchange(vf, albedo, emissivity)
/// scattered, thermal = gpu.absorbed(incident=sunlight, temperature=surface)
/// ```
///
/// `vf` is a `kalast.tpm.exchange.ViewFactors` with a row and a column for
/// each facet of every body; `albedo` and `emissivity` are a number or one a
/// facet. `absorbed` takes what the CPU's takes and gives the same two arrays
/// (W/m2), to the rounding of single precision.
///
/// A thread cannot tell that the reflections have settled, so with
/// `bounces=0` the passes are counted beforehand from the view factors and
/// the surfaces (`passes`), enough to bring what is left under `tolerance`.
///
/// Pass `context` to share a device with `GpuStepper`.
#[pyo3::pyclass(unsendable, name = "GpuExchange", module = "kalast._rs.tpm.gpu")]
pub struct GpuExchange {
    inner: crate::tpm::gpu_exchange::GpuExchange,
}

#[pyo3::pymethods]
impl GpuExchange {
    #[new]
    #[pyo3(signature = (factors, albedo, emissivity, context=None))]
    fn new(
        factors: PyRef<'_, crate::tpm::exchange::py::ViewFactors>,
        albedo: &Bound<'_, PyAny>,
        emissivity: &Bound<'_, PyAny>,
        context: Option<GpuContext>,
    ) -> PyResult<Self> {
        let value_error = crate::tpm::core::py::value_error;
        let n = factors.inner.rows();
        let albedo = crate::tpm::exchange::py::spread(albedo, n, "albedo")?;
        let emissivity = crate::tpm::exchange::py::spread(emissivity, n, "emissivity")?;
        let ctx = match context {
            Some(c) => c.inner,
            None => crate::gpu::Context::new().map_err(pyo3::exceptions::PyRuntimeError::new_err)?,
        };
        crate::tpm::gpu_exchange::GpuExchange::new(ctx, &factors.inner, &albedo, &emissivity).map(|inner| Self { inner }).map_err(value_error)
    }

    /// `(scattered, thermal)`: what every facet absorbs of the sunlight the
    /// others scatter and of their thermal radiation (W/m2). Blocking.
    #[pyo3(signature = (incident=None, temperature=None, bounces=0, tolerance=1.0e-6))]
    #[allow(clippy::type_complexity)]
    fn absorbed<'py>(
        &self,
        py: Python<'py>,
        incident: Option<&Bound<'py, PyAny>>,
        temperature: Option<&Bound<'py, PyAny>>,
        bounces: usize,
        tolerance: crate::Float,
    ) -> PyResult<(pyo3::Bound<'py, numpy::PyArray1<crate::Float>>, pyo3::Bound<'py, numpy::PyArray1<crate::Float>>)> {
        use crate::tpm::core::py::{per_facet, value_error};
        let incident = incident.map(|v| per_facet(v, "incident")).transpose()?;
        let temperature = temperature.map(|v| per_facet(v, "temperature")).transpose()?;
        let got = self
            .inner
            .absorbed(
                incident.as_ref().map(|v| v.as_slice().unwrap()),
                temperature.as_ref().map(|v| v.as_slice().unwrap()),
                crate::tpm::exchange::Bounces { most: bounces, tolerance },
            )
            .map_err(value_error)?;
        Ok((numpy::PyArray1::from_vec(py, got.scattered), numpy::PyArray1::from_vec(py, got.thermal)))
    }

    /// The passes `absorbed` takes for these `bounces` and `tolerance`.
    #[pyo3(signature = (bounces=0, tolerance=1.0e-6))]
    fn passes(&self, bounces: usize, tolerance: crate::Float) -> PyResult<usize> {
        self.inner.passes(crate::tpm::exchange::Bounces { most: bounces, tolerance }).map_err(crate::tpm::core::py::value_error)
    }

    /// The facets.
    #[getter]
    fn facets(&self) -> usize {
        self.inner.facets()
    }
}
