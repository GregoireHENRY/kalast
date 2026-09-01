use numpy::prelude::*;
use pyo3::prelude::*;
use std::sync::Arc;

/// A GPU device shared by the compute stages.
///
/// Build one and hand it to both `GpuTpm` and `GpuRadiance` and the
/// temperatures never leave the GPU. Build them separately and each gets its
/// own device, which still works but pays a round trip through host memory.
#[pyo3::pyclass(name = "GpuContext", module = "kalast._rs.tpm.gpu")]
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

/// Band-integrated radiance from facet temperatures, on the GPU.
///
/// Independent of where the temperatures came from, which is the point: the
/// model's two stages each run on the CPU or the GPU as the caller likes.
///
///     CPU TPM -> GPU radiance :  set_temperatures(t)
///     GPU TPM -> GPU radiance :  bind_tpm(tpm)      # nothing moves
///     GPU TPM -> CPU radiance :  bands[f](tpm.surface())
///     CPU TPM -> CPU radiance :  bands[f](t)
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
