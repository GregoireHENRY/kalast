use pyo3::prelude::*;

/// The thermophysical model stepped on the GPU, one column per facet.
///
/// The state stays resident between steps, so a spin-up costs one upload, N
/// dispatches and one download rather than moving arrays every step.
///
/// Everything is f32: WGSL has no f64, while the numpy path is float64. The
/// difference is measured in `examples/analytical/tpm_gpu_vs_cpu.py` rather
/// than assumed away.
#[pyo3::pyclass(name = "GpuTpm", module = "kalast._rs.tpm.gpu")]
pub struct GpuTpm {
    inner: crate::tpm::gpu::GpuTpm,
}

#[pyo3::pymethods]
impl GpuTpm {
    /// `coef_lo`/`coef_hi` are what `routine.nonuniform_coefficients` returns,
    /// `diffusivity` is per node, and the rest is the radiative surface
    /// boundary: `se = emissivity * sigma`, the conductivity, and `2 dz`.
    #[new]
    #[pyo3(signature = (n_facets, coef_lo, coef_hi, diffusivity, se, conductivity,
                        twodz, newton_threshold=0.1, newton_max_iter=100))]
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
    ) -> PyResult<Self> {
        crate::tpm::gpu::GpuTpm::new(
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
        let flat = self_.inner.download();
        let rows: Vec<Vec<f32>> = flat.chunks(m).take(n).map(|c| c.to_vec()).collect();
        numpy::PyArray2::from_vec2(py, &rows)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Surface temperature per facet. Also a blocking readback.
    fn surface<'py>(slf: pyo3::Bound<'py, Self>) -> pyo3::Bound<'py, numpy::PyArray1<f32>> {
        let py = slf.py();
        let self_ = slf.borrow();
        numpy::PyArray1::from_slice(py, &self_.inner.surface())
    }
}
