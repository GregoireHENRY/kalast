// Diffuse solar radiation
//
// Args:
//     view factor between the surface of the two bodies
//     radiation of the Sun on the surface of the other body
//     albedo of the surface of the other body
//
// Out:
//     heat flux (W/m2)
//
// The diffuse solar radiation contribution from all $N$ facets $i$ the other body onto the
// facet $j$ of the body is defined as,
//
// .. math::
// W_{i}=\sum_{\substack{j \\ j\neq i}}^{N}V_{ij}\frac{S_\odot A\cos\varsigma_j\left(t\right)}{r_H^2\left(t\right)}
//
// where $V_{ij}$ is the view factor describing the fraction of energy emitted from one facet
// $i$ towards the facet $j$, $S_\odot$ is [Solar Constant][SOLAR_CONSTANT], $A$ the albedo,
// $\varsigma_j$ the illumination angle of the facet $j$, and $r_H$ the heliocentric distance
// in [AU][ASTRONOMICAL_UNIT].
//
//
// Direct thermal heating
//
// Args:
//     view factor between the surface of the two bodies
//     temperature and emissivity of the surface of the other body
//
// Out:
//     heat flux (W/m2)
//
// Expression:
//     The direct thermal heating contribution from all $N$ facets $i$ of the other body onto the
//     facet $j$ of the body is defined as,
//
//     $$u_{j}=\sum_{i\cancel{=}j}^{N}V_{ij}\epsilon\sigma T_{i}^4$$
//
//     where $V_{ij}$ is the view factor describing the fraction of energy emitted from one facet
//     $i$ towards the facet $j$, $\epsilon$ the emissivity, $\sigma$ the
//     [Stefan-Boltzmann constant][STEFAN_BOLTZMANN], and $T_i$ the temperature of the facet $i$.
//
// units:
// - radiance: W/m2/sr
// - spectral radiance: W/m3/sr
// - irradiance (=flux density): W/m2
// - spectral irradiance: W/m3
//   W/m2/um = W/m3 * 1e-6
//
// Jansky: 1 W/m2/Hz = 1e26 Jy
// 1) convert spectral irradiance from W/m3 to W/m2/Hz
//    with: W/m3 * lamda^2 / speed_light = W/m2/Hz
// 2) Then can apply: W/m2/Hz * JANSKY
//
//
// kirchhoff_law:
//     Emissivity and albedo (directional-hemispherical reflectivity) are simply related.
//     Required to obtain thermal equilibrium and essential to derive Planck spectrum.
//     a = 1 - e

use anyhow::{Result, anyhow};
use numpy::ndarray::{Array1, ArrayView1, s};
use pyo3::prelude::*;

use crate::Float;

#[pyfunction]
pub fn stability(d: Float, dt: Float, dx2: Float) -> Float {
    // Stability coefficient for conduction_1d, lower than 0.5 is converging.
    // Also called Fourier mesh number.
    //
    // d: diffusivity (...)
    // dt: time step (s)
    // dx2: depth step squared (m2)
    d * dt / dx2
}

#[pyfunction]
#[pyo3(signature = (d, dx2, s=0.5))]
pub fn stability_maxdt(d: Float, dx2: Float, s: Float) -> Float {
    // Find largest dt for conduction_1d to be stable considering depth step and diffusivity.
    // s is usually 0.5
    //
    // d: diffusivity (...)
    // dx2: depth step squared (m2)
    // s: stability coef
    s * dx2 / d
}

#[pyfunction]
pub fn conduction(t: Float, f: Float, k: Float, dx: Float) -> Float {
    // Update temperature from a flux over a distance.
    // Adiabatic is f=0.
    //
    // t: temperature (K)
    // f: heat flux (W/m2)
    // k: conductivity (...)
    // dx: distance (m)
    t + dx * f / k
}

#[pyfunction]
pub fn effective_temperature(dau: Float, r: Float, a: Float, e: Float) -> Float {
    // dau: distance of Sun is AU
    // r: ratio between areas receiving and emitting
    // a: albedo
    // e: emissivity
    (crate::util::SOLAR_CONSTANT * r * (1.0 - a)
        / (e * crate::util::STEFAN_BOLTZMANN * dau.powi(2)))
    .powf(0.25)
}

#[pyfunction]
pub fn radiation_sun(dau: Float, cosi: Float, a: Float) -> Float {
    // dau: distance of Sun is AU
    // cosi: cosine of incidence angle of local surface
    // a: albedo
    crate::util::SOLAR_CONSTANT * (1.0 - a) * cosi / dau.powi(2)
}

#[pyfunction]
pub fn radiation_sun_reflected(viewf: Float, a: Float, cosi: Float, dau: Float) -> Float {
    // viewf: view-factor of local surface
    // a: albedo
    // cosi: cosine of incidence angle of local surface
    // dau: distance of Sun is AU
    viewf * crate::util::SOLAR_CONSTANT * a * cosi / dau.powi(2)
}

/// care with albedos
#[pyfunction]
pub fn radiation_sun_reflected_reuse(viewf: Float, f: Float, a: Float) -> Float {
    // viewf: view-factor of local surface
    // f: radiation from sun from another surface
    // a: albedo
    viewf * f * a / (1.0 - a)
}

#[pyfunction]
pub fn radiation_emitted(viewf: Float, t: Float, e: Float) -> Float {
    // viewf: view-factor of local surface
    // t: temperature (K)
    // e: emissivity
    viewf * crate::util::STEFAN_BOLTZMANN * e * t.powi(4)
}

#[pyfunction]
pub fn newton_method_fn(
    t: Float,
    f: Float,
    set3: Float,
    k: Float,
    subt1: Float,
    subt2: Float,
    twodx: Float,
) -> Float {
    f - set3 * t + k * (-3.0 * t + 4.0 * subt1 - subt2) / twodx
}

#[pyfunction]
pub fn newton_method_dfn(set3: Float, k: Float, twodx: Float) -> Float {
    -4.0 * set3 - 3.0 * k / twodx
}

pub fn newton_method(
    mut t: Float,
    f: Float,
    se: Float,
    k: Float,
    subt1: Float,
    subt2: Float,
    twodx: Float,
) -> Result<Float> {
    for _ in 0..crate::util::NEWTON_METHOD_MAX_ITERATION {
        let set3 = se * t.powi(3);
        let fn_ = newton_method_fn(t, f, set3, k, subt1, subt2, twodx);
        let dfn = newton_method_dfn(set3, k, twodx);
        let delta = -fn_ / dfn;
        t += delta;
        if delta.abs() < crate::util::NEWTON_METHOD_THRESHOLD {
            return Ok(t);
        }
    }
    Err(anyhow!("Newton method reached maximum iteration"))
}

pub fn conduction_1d(
    t: ArrayView1<'_, Float>,
    d: ArrayView1<'_, Float>,
    dtpdx2: ArrayView1<'_, Float>,
) -> Array1<Float> {
    let t_mid = t.slice(s![1..-1]);
    &t_mid + &d.slice(s![1..-1]) * &dtpdx2 * (&t.slice(s![..-2]) - 2.0 * &t_mid + &t.slice(s![2..]))
}

pub(crate) mod py {
    use numpy::{PyArray1, PyReadonlyArray1, ToPyArray};
    use pyo3::prelude::*;

    use super::Float;

    #[pyfunction]
    pub fn newton_method(
        t: Float,
        f: Float,
        se: Float,
        k: Float,
        subt1: Float,
        subt2: Float,
        twodx: Float,
    ) -> PyResult<Float> {
        Ok(super::newton_method(t, f, se, k, subt1, subt2, twodx).unwrap())
    }

    #[pyfunction]
    pub fn conduction_1d<'py>(
        py: Python<'py>,
        t: PyReadonlyArray1<'py, Float>,
        d: PyReadonlyArray1<'py, Float>,
        dtpdx2: PyReadonlyArray1<'_, Float>,
    ) -> Bound<'py, PyArray1<Float>> {
        super::conduction_1d(t.as_array(), d.as_array(), dtpdx2.as_array()).to_pyarray(py)
    }
}
