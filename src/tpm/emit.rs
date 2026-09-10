use ndarray::ArrayView1;
#[cfg(feature = "python")]
use pyo3::prelude::*;

use crate::Float;

#[cfg_attr(feature = "python", pyfunction)]
pub fn planck(t: Float, w: Float) -> Float {
    // t: temperature (K)
    // w: wavelength (m)
    //
    // output spectral radiance (W/m3/sr)
    //
    // `exp_m1`, not `exp() - 1.0`. The two are the same mathematically and
    // not numerically: at long wavelength the exponent goes to zero, `exp`
    // returns something just above 1, and subtracting 1 from it in f32 throws
    // away almost every significant digit. At 50 mm and 300 K the exponent is
    // 9.6e-4 and the naive form loses ~6e-5 relative -- enough to miss the
    // Rayleigh-Jeans limit by 2 %, which is how `tests/test_planck.py` found
    // it. `exp_m1` computes the small difference directly.
    //
    // The numpy Planck this replaced already used `expm1`; the merge would
    // have quietly lost that if the limit were not tested.
    crate::util::TWO_HC2 / (w.powi(5) * (crate::util::HC_PER_K / (t * w)).exp_m1())
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn planck_photon_count(t: Float, w: Float) -> Float {
    // t: temperature (K)
    // w: wavelength (m)
    // `exp_m1` for the reason given in `planck` above.
    crate::util::TWO_C / (w.powi(4) * (crate::util::HC_PER_K / (t * w)).exp_m1())
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn spectral_radiance(f: Float, e: Float, cose: Float, r: Float) -> Float {
    // f: planck radiation (W/m3/sr)
    // e: spectral emissivity
    // cose: cosine of emission angle
    // r: roughness correction
    f * e * cose * r
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn steradian(a: Float, d: Float) -> Float {
    // a: area (m2)
    // d: distance (m)
    a / d.powi(2)
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn irradiance(f: Float, sr: Float) -> Float {
    // f: radiance (W/m2/sr) or spectral radiance (W/m3/sr)
    // sr: steradian
    f * sr
}

pub fn radiance(f: Float, r: ArrayView1<Float>, w: ArrayView1<Float>) -> Float {
    // f: spectral radiance (W/m3/sr)
    // r: response function (filters, transparency, ...)
    // w: wavelength (m)
    let y = f * &r;
    crate::math::simpson_1_3(y.view(), w)
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn reflectance(w: Float, a: Float, area: Float, cose: Float, d: Float, r: Float) -> Float {
    // w: wavelength (m)
    // a: albedo
    // area reflecting
    // cose: cosine of emission angle
    // d: distance from reflecting to target
    // r: distance from sun to reflecting target
    planck(crate::util::TEMP_SUN, w) * a * area * cose / (d * d * r * r)
}

// fn_f_sun = lambda x: planck(TEMP_SUN, x) * pi * RADIUS_SUN * RADIUS_SUN / (AU * AU)
// S, _err = scipy.integrate.quad(fn_f_sun, 1e-10, 1e-2)
// print(S)
// = solar constant

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{PyArray1, PyReadonlyArray1};
    #[cfg(feature = "python")]
use pyo3::prelude::*;

    use super::Float;

    /// `planck` over paired arrays, so numpy code has one formula to call
    /// rather than a second copy of it.
    ///
    /// `kalast/tpm/radiance.py` used to carry its own numpy Planck, with its
    /// own `h`, `c` and `k_B` redefined locally -- two implementations of a
    /// closed form, agreeing only by luck, while `kalast.util` was already
    /// re-exporting these very constants from Rust. It calls this instead now.
    ///
    /// Elementwise on two equal-length arrays rather than broadcasting here:
    /// numpy broadcasts far better than this could, so the Python side
    /// broadcasts and flattens, and this stays a loop over one formula.
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn planck_array<'py>(
        py: Python<'py>,
        t: PyReadonlyArray1<'py, Float>,
        w: PyReadonlyArray1<'py, Float>,
    ) -> PyResult<Bound<'py, PyArray1<Float>>> {
        let t = t.as_array();
        let w = w.as_array();
        if t.len() != w.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "planck_array: {} temperatures against {} wavelengths;                  broadcast them to a common shape first",
                t.len(),
                w.len()
            )));
        }
        let out: Vec<Float> = t
            .iter()
            .zip(w.iter())
            .map(|(&t, &w)| super::planck(t, w))
            .collect();
        Ok(PyArray1::from_vec(py, out))
    }

    #[cfg_attr(feature = "python", pyfunction)]
    pub fn radiance(
        f: Float,
        r: PyReadonlyArray1<'_, Float>,
        w: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::radiance(f, r.as_array(), w.as_array()))
    }
}
