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
    crate::util::TWO_HC2 / (w.powi(5) * ((crate::util::HC_PER_K / (t * w)).exp() - 1.0))
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn planck_photon_count(t: Float, w: Float) -> Float {
    // t: temperature (K)
    // w: wavelength (m)
    crate::util::TWO_C / (w.powi(4) * ((crate::util::HC_PER_K / (t * w)).exp() - 1.0))
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
    use numpy::PyReadonlyArray1;
    #[cfg(feature = "python")]
use pyo3::prelude::*;

    use super::Float;

    #[cfg_attr(feature = "python", pyfunction)]
    pub fn radiance(
        f: Float,
        r: PyReadonlyArray1<'_, Float>,
        w: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::radiance(f, r.as_array(), w.as_array()))
    }
}
