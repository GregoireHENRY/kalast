use numpy::prelude::*;
use pyo3::prelude::*;

/// Kuehrt spherical-crater roughness correction for thermal infrared.
///
/// `correction(sun, det, psi)` returns the ratio of the flux a cratered facet
/// sends to the detector against a flat Lambertian one at the same insolation.
/// Multiply smooth radiance by it directly: the crater density is already
/// inside. See `kalast::tpm::roughness` for the lineage and the caveats.
#[pyo3::pyclass(name = "Crater", module = "kalast._rs.tpm.roughness")]
#[derive(Clone)]
pub struct Crater {
    inner: crate::tpm::roughness::Crater,
}

#[pyo3::pymethods]
impl Crater {
    #[new]
    #[pyo3(signature = (gamma_deg=180.0, density=0.25, emissivity=0.95, albedo=0.12,
                        rh=1.0, wavelength=8.0e-6, ntheta=32, nphi=32,
                        solar_constant=1369.0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        gamma_deg: f64,
        density: f64,
        emissivity: f64,
        albedo: f64,
        rh: f64,
        wavelength: f64,
        ntheta: usize,
        nphi: usize,
        solar_constant: f64,
    ) -> Self {
        Self {
            inner: crate::tpm::roughness::Crater {
                gamma: gamma_deg.to_radians(),
                density,
                emissivity,
                albedo,
                rh,
                wavelength,
                ntheta,
                nphi,
                solar_constant,
            },
        }
    }

    /// `(correction, crater_flux, flat_flux)` for one geometry, radians.
    fn correction(&self, sun: f64, det: f64, psi: f64) -> (f64, f64, f64) {
        self.inner.correction(sun, det, psi)
    }

    /// Correction over arrays of angles, elementwise. Radians.
    fn correction_many<'py>(
        slf: pyo3::Bound<'py, Self>,
        sun: numpy::PyReadonlyArray1<'_, f64>,
        det: numpy::PyReadonlyArray1<'_, f64>,
        psi: numpy::PyReadonlyArray1<'_, f64>,
    ) -> PyResult<pyo3::Bound<'py, numpy::PyArray1<f64>>> {
        let py = slf.py();
        let this = slf.borrow();
        let (s, d, p) = (sun.as_slice()?, det.as_slice()?, psi.as_slice()?);
        if s.len() != d.len() || s.len() != p.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "sun, det and psi must be the same length",
            ));
        }
        let out: Vec<f64> = (0..s.len())
            .map(|i| this.inner.correction(s[i], d[i], p[i]).0)
            .collect();
        Ok(numpy::PyArray1::from_vec(py, out))
    }

    /// A lookup table over a regular grid of the three angles, in degrees.
    ///
    /// Shape `(n_sun, n_det, n_psi)`. This is the thing that made the MATLAB
    /// worth running once: the correction depends only on geometry and
    /// material, so a table is built once per band and interpolated per facet
    /// per epoch thereafter.
    fn table<'py>(
        slf: pyo3::Bound<'py, Self>,
        sun_deg: numpy::PyReadonlyArray1<'_, f64>,
        det_deg: numpy::PyReadonlyArray1<'_, f64>,
        psi_deg: numpy::PyReadonlyArray1<'_, f64>,
    ) -> PyResult<pyo3::Bound<'py, numpy::PyArray3<f64>>> {
        let py = slf.py();
        let this = slf.borrow();
        let (s, d, p) = (sun_deg.as_slice()?, det_deg.as_slice()?, psi_deg.as_slice()?);
        let mut out = Vec::with_capacity(s.len() * d.len() * p.len());
        for &si in s {
            for &di in d {
                for &pi_ in p {
                    out.push(
                        this.inner
                            .correction(si.to_radians(), di.to_radians(), pi_.to_radians())
                            .0,
                    );
                }
            }
        }
        numpy::PyArray1::from_vec(py, out)
            .reshape([s.len(), d.len(), p.len()])
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}
