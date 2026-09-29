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

use anyhow::{Result, anyhow, bail};
use ndarray::{Array1, Array2, ArrayView1, ArrayViewMut2, Zip, s};
#[cfg(feature = "python")]
use pyo3::prelude::*;

use super::properties::Properties;
use crate::Float;

#[cfg_attr(feature = "python", pyfunction)]
pub fn stability(d: Float, dt: Float, dx2: Float) -> Float {
    // Stability coefficient for conduction_1d, lower than 0.5 is converging.
    // Also called Fourier mesh number.
    //
    // d: diffusivity (...)
    // dt: time step (s)
    // dx2: depth step squared (m2)
    d * dt / dx2
}

#[cfg_attr(feature = "python", pyfunction)]
#[cfg_attr(feature = "python", pyo3(signature = (d, dx2, s=0.5)))]
pub fn stability_maxdt(d: Float, dx2: Float, s: Float) -> Float {
    // Find largest dt for conduction_1d to be stable considering depth step and diffusivity.
    // s is usually 0.5
    //
    // d: diffusivity (...)
    // dx2: depth step squared (m2)
    // s: stability coef
    s * dx2 / d
}

#[cfg_attr(feature = "python", pyfunction)]
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

#[cfg_attr(feature = "python", pyfunction)]
pub fn effective_temperature(dau: Float, r: Float, a: Float, e: Float) -> Float {
    // dau: distance of Sun is AU
    // r: ratio between areas receiving and emitting
    // a: albedo
    // e: emissivity
    (crate::util::SOLAR_CONSTANT * r * (1.0 - a)
        / (e * crate::util::STEFAN_BOLTZMANN * dau.powi(2)))
    .powf(0.25)
}

/// Absorbed solar flux on a surface element.
///
/// `cosi` is clamped at zero: a facet tilted away from the Sun receives
/// nothing, it does not radiate *into* the Sun. Without the clamp a negative
/// cosine yields negative insolation, which in a thermophysical model does
/// not merely lose a term but actively drives night-side facets below their
/// radiative balance.
///
/// `math::cosine_incidence` already clamps, so callers going through it were
/// safe; anything computing a dot product directly -- a vectorised inner loop,
/// for instance -- was not. The invariant belongs here rather than in each
/// caller.
#[cfg_attr(feature = "python", pyfunction)]
pub fn radiation_sun(dau: Float, cosi: Float, a: Float) -> Float {
    // dau: distance of Sun is AU
    // cosi: cosine of incidence angle of local surface
    // a: albedo
    crate::util::SOLAR_CONSTANT * (1.0 - a) * cosi.max(0.0) / dau.powi(2)
}

/// Sunlight reflected off one surface element toward another.
///
/// `cosi` is clamped for the same reason as `radiation_sun`: an element
/// facing away from the Sun reflects nothing, and an unclamped negative would
/// have a shadowed facet *removing* energy from whatever it illuminates.
#[cfg_attr(feature = "python", pyfunction)]
pub fn radiation_sun_reflected(viewf: Float, a: Float, cosi: Float, dau: Float) -> Float {
    // viewf: view-factor of local surface
    // a: albedo
    // cosi: cosine of incidence angle of local surface
    // dau: distance of Sun is AU
    viewf * crate::util::SOLAR_CONSTANT * a * cosi.max(0.0) / dau.powi(2)
}

/// care with albedos
#[cfg_attr(feature = "python", pyfunction)]
pub fn radiation_sun_reflected_reuse(viewf: Float, f: Float, a: Float) -> Float {
    // viewf: view-factor of local surface
    // f: radiation from sun from another surface
    // a: albedo
    viewf * f * a / (1.0 - a)
}

#[cfg_attr(feature = "python", pyfunction)]
pub fn radiation_emitted(viewf: Float, t: Float, e: Float) -> Float {
    // viewf: view-factor of local surface
    // t: temperature (K)
    // e: emissivity
    viewf * crate::util::STEFAN_BOLTZMANN * e * t.powi(4)
}

#[cfg_attr(feature = "python", pyfunction)]
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

#[cfg_attr(feature = "python", pyfunction)]
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

/// Explicit conduction step on a grid with variable spacing.
///
/// `conduction_1d` uses the equal-spacing second difference, which is only
/// second-order when every layer has the same thickness. A geometric grid --
/// the practical way to reach the seasonal skin depth without thousands of
/// nodes -- breaks that assumption badly: validated against the analytical
/// damped wave it errs by ~12 K where the uniform stencil on a uniform grid
/// errs by 0.3 K.
///
/// This applies the variable-spacing form
///
/// ```text
/// d2T/dz2 ~ 2/(h- + h+) * [ (T+ - T)/h+ - (T - T-)/h- ]
/// ```
///
/// with the two coefficients precomputed per interior node, mirroring how
/// `conduction_1d` takes `dt/dx^2`:
///
/// ```text
/// coef_lo = 2 dt / (h- (h- + h+))      coef_hi = 2 dt / (h+ (h- + h+))
/// ```
///
/// For equal spacing both collapse to `dt/h^2` and this reduces exactly to
/// `conduction_1d`.
pub fn conduction_1d_nonuniform(
    t: ArrayView1<'_, Float>,
    d: ArrayView1<'_, Float>,
    coef_lo: ArrayView1<'_, Float>,
    coef_hi: ArrayView1<'_, Float>,
) -> Array1<Float> {
    let t_lo = t.slice(s![..-2]);
    let t_mid = t.slice(s![1..-1]);
    let t_hi = t.slice(s![2..]);

    &t_mid
        + &d.slice(s![1..-1])
            * (&coef_lo * (&t_lo - &t_mid) + &coef_hi * (&t_hi - &t_mid))
}

/// Temperatures for a whole body: a column of `layers` under each of
/// `facets`, all at `t` (K), for `solar_bc`, `bottom_adiabatic` and
/// `heat_conduction` to step.
///
/// `(layers, facets)`: row 0 is the surface and the last row the bottom;
/// column `i` is the ground under facet `i`. A layer is then one contiguous
/// row across the body -- the surface colours a mesh as it is, and each step
/// below is a sweep over whole rows.
pub fn columns(layers: usize, facets: usize, t: Float) -> Array2<Float> {
    Array2::from_elem((layers, facets), t)
}

/// The solar boundary condition, every facet at once: each surface
/// temperature solved for the balance of the sunlight it absorbs, what it
/// radiates, and what it conducts into the layers under it,
///
/// ```text
/// S (1 - A) max(cos i, 0) / r^2  -  e sigma T0^4  +  k (-3 T0 + 4 T1 - T2) / (2 dz)  =  0
/// ```
///
/// -- `radiation_sun` and `newton_method` on each column of `t`, laid out as
/// `columns` makes it. `dau` is the distance to the Sun (AU), `cosi` a cosine
/// of incidence per facet (`Simulation::facet_incidence`), and `dz` the
/// thickness of a layer (m).
///
/// An error names the first facet whose balance did not converge. That is a
/// run already gone wrong under it -- temperatures run away or NaN from a
/// time step past `stability_maxdt` -- and nothing to continue from.
pub fn solar_bc(
    mut t: ArrayViewMut2<'_, Float>,
    dau: Float,
    cosi: ArrayView1<'_, Float>,
    prop: &Properties,
    dz: Float,
) -> Result<()> {
    let (layers, facets) = t.dim();
    if layers < 3 {
        bail!("the surface balance needs 3 layers or more, got {layers}");
    }
    if cosi.len() != facets {
        bail!("{} cosines of incidence for {facets} facets", cosi.len());
    }
    let se = crate::util::STEFAN_BOLTZMANN * prop.emissivity;
    let twodz = 2.0 * dz;
    let (mut surface, below) = t.multi_slice_mut((s![0, ..], s![1..3, ..]));
    let mut failed = None;
    Zip::indexed(&mut surface)
        .and(below.row(0))
        .and(below.row(1))
        .and(cosi)
        .for_each(|i, t0, &t1, &t2, &cosi| {
            let f = radiation_sun(dau, cosi, prop.albedo);
            match newton_method(*t0, f, se, prop.conductivity, t1, t2, twodz) {
                Ok(t) => *t0 = t,
                Err(_) => {
                    failed.get_or_insert((i, *t0, t1));
                }
            }
        });
    match failed {
        Some((i, t0, t1)) => bail!(
            "the surface balance of facet {i} did not converge (T0 = {t0} K, T1 = {t1} K): \
             the temperatures under it have run away, a time step past stability_maxdt?"
        ),
        None => Ok(()),
    }
}

/// The adiabatic bottom, every facet at once: no heat through the base of a
/// column, its last layer at the temperature of the one above.
///
/// Right for a column deep enough that the thermal wave has died out before
/// the base -- `skin_depth_2pi` for the wave of the rotation leaves 0.2 % of
/// the surface's swing -- so there is nothing left to cross it.
pub fn bottom_adiabatic(mut t: ArrayViewMut2<'_, Float>) {
    let layers = t.nrows();
    if layers < 2 {
        return;
    }
    let (mut bottom, above) = t.multi_slice_mut((s![layers - 1, ..], s![layers - 2, ..]));
    bottom.assign(&above);
}

/// Heat conduction, every facet at once: one explicit step of `dt` (s)
/// through the interior of each column, its layers `dz` (m) apart,
///
/// ```text
/// T_i  +=  D dt / dz^2  (T_{i-1} - 2 T_i + T_{i+1})
/// ```
///
/// -- `conduction_1d` on each column of `t`: the equal-spacing stencil, for
/// the uniform layers `columns` stands for. The surface and the bottom layer
/// are the boundary conditions', `solar_bc` and `bottom_adiabatic`.
///
/// Refused past the scheme's stability, `D dt / dz^2 > 1/2`, where it does
/// not lose accuracy so much as grow without bound; `stability_maxdt` gives
/// the largest `dt`. Refused too with no diffusivity -- a `Properties` whose
/// `compute_conductivity_diffusivity` was never called -- where nothing would
/// conduct and nothing would say so.
pub fn heat_conduction(
    mut t: ArrayViewMut2<'_, Float>,
    prop: &Properties,
    dt: Float,
    dz: Float,
) -> Result<()> {
    let d = prop.diffusivity;
    if !(d > 0.0) {
        bail!(
            "the diffusivity is {d}: compute it from the thermal inertia first, \
             with compute_conductivity_diffusivity()"
        );
    }
    let r = stability(d, dt, dz * dz);
    // A few ulps over 1/2 are the rounding of a `dt` taken at the limit.
    if r > 0.5 * (1.0 + 8.0 * Float::EPSILON) {
        bail!(
            "unstable: D dt / dz^2 = {r} > 0.5; dt can be {} s at most (stability_maxdt)",
            stability_maxdt(d, dz * dz, 0.5)
        );
    }
    let layers = t.nrows();
    if layers < 3 {
        return Ok(());
    }
    // Each layer is stepped from the old values only: the one above as it
    // was before it was stepped itself.
    let mut above = t.row(0).to_owned();
    for i in 1..layers - 1 {
        let (mut layer, below) = t.multi_slice_mut((s![i, ..], s![i + 1, ..]));
        Zip::from(&mut layer)
            .and(&below)
            .and(&mut above)
            .for_each(|t, &below, above| {
                let old = *t;
                *t = old + r * (*above - 2.0 * old + below);
                *above = old;
            });
    }
    Ok(())
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1, ToPyArray};
    #[cfg(feature = "python")]
use pyo3::prelude::*;

    use super::Float;

    #[cfg_attr(feature = "python", pyfunction)]
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

    #[cfg_attr(feature = "python", pyfunction)]
    pub fn conduction_1d_nonuniform<'py>(
        py: Python<'py>,
        t: PyReadonlyArray1<'py, Float>,
        d: PyReadonlyArray1<'py, Float>,
        coef_lo: PyReadonlyArray1<'_, Float>,
        coef_hi: PyReadonlyArray1<'_, Float>,
    ) -> Bound<'py, PyArray1<Float>> {
        super::conduction_1d_nonuniform(
            t.as_array(),
            d.as_array(),
            coef_lo.as_array(),
            coef_hi.as_array(),
        )
        .to_pyarray(py)
    }

    #[cfg_attr(feature = "python", pyfunction)]
    pub fn conduction_1d<'py>(
        py: Python<'py>,
        t: PyReadonlyArray1<'py, Float>,
        d: PyReadonlyArray1<'py, Float>,
        dtpdx2: PyReadonlyArray1<'_, Float>,
    ) -> Bound<'py, PyArray1<Float>> {
        super::conduction_1d(t.as_array(), d.as_array(), dtpdx2.as_array()).to_pyarray(py)
    }

    /// Temperatures for a whole body: a column of `layers` under each of
    /// `facets`, all at `t` (K), for `solar_bc`, `bottom_adiabatic` and
    /// `heat_conduction` to step in place.
    ///
    /// `(layers, facets)`: `t[0]` is the surface, one temperature per facet,
    /// ready for `mesh.values`; `t[-1]` the bottom; `t[:, i]` the ground under
    /// facet `i`. Made here rather than with `numpy.full` so that it has the
    /// float type kalast was built with.
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn columns<'py>(
        py: Python<'py>,
        layers: usize,
        facets: usize,
        t: Float,
    ) -> Bound<'py, PyArray2<Float>> {
        super::columns(layers, facets, t).into_pyarray(py)
    }

    /// The solar boundary condition, every facet at once: each surface
    /// temperature, `t[0]`, solved for the balance of the sunlight it
    /// absorbs, what it radiates, and what it conducts into the layers under
    /// it,
    ///
    /// ```text
    /// S (1 - A) max(cos i, 0) / r^2  -  e sigma T0^4  +  k (-3 T0 + 4 T1 - T2) / (2 dz)  =  0
    /// ```
    ///
    /// `t` is what `columns` made, changed in place; `dau` the distance to
    /// the Sun (AU); `cosi` a cosine of incidence per facet, as
    /// `sim.facet_incidence` gives them; `prop` the surface's `Properties`;
    /// `dz` the thickness of a layer (m).
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn solar_bc(
        t: &Bound<'_, PyAny>,
        dau: Float,
        cosi: &Bound<'_, PyAny>,
        prop: crate::py::tpm::properties::Properties,
        dz: Float,
    ) -> PyResult<()> {
        let mut t = temperatures(t)?;
        let cosi = per_facet(cosi, "cosi")?;
        super::solar_bc(t.as_array_mut(), dau, cosi.view(), &prop.inner.borrow(), dz)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// The adiabatic bottom, every facet at once: no heat through the base
    /// of a column, its last layer, `t[-1]`, at the temperature of the one
    /// above. `t` is what `columns` made, changed in place.
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn bottom_adiabatic(t: &Bound<'_, PyAny>) -> PyResult<()> {
        super::bottom_adiabatic(temperatures(t)?.as_array_mut());
        Ok(())
    }

    /// Heat conduction, every facet at once: one explicit step of `dt` (s)
    /// through the interior of each column, its layers `dz` (m) apart,
    ///
    /// ```text
    /// T_i  +=  D dt / dz^2  (T_{i-1} - 2 T_i + T_{i+1})
    /// ```
    ///
    /// `t` is what `columns` made, changed in place, and `prop` gives the
    /// diffusivity `D`. Refused past the stability of the scheme, `D dt /
    /// dz^2 > 1/2`: `stability_maxdt` gives the largest `dt`.
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn heat_conduction(
        t: &Bound<'_, PyAny>,
        prop: crate::py::tpm::properties::Properties,
        dt: Float,
        dz: Float,
    ) -> PyResult<()> {
        let mut t = temperatures(t)?;
        super::heat_conduction(t.as_array_mut(), &prop.inner.borrow(), dt, dz)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// The temperatures, to change in place, so of kalast's own float type
    /// already: there would be nothing to write back into otherwise. Checked
    /// here for a message that says so -- a float64 array's is otherwise
    /// "'ndarray' object is not an instance of 'ndarray'".
    fn temperatures<'py>(
        t: &Bound<'py, PyAny>,
    ) -> PyResult<numpy::PyReadwriteArray2<'py, Float>> {
        t.extract().map_err(|_| {
            let dtype = if std::mem::size_of::<Float>() == 8 { "float64" } else { "float32" };
            pyo3::exceptions::PyTypeError::new_err(format!(
                "t must be a writable {dtype} array of (layers, facets), \
                 as kalast.tpm.core.columns makes it"
            ))
        })
    }

    /// One value per facet, from an array of either float width or a list.
    fn per_facet(v: &Bound<'_, PyAny>, name: &str) -> PyResult<ndarray::Array1<Float>> {
        if let Ok(a) = v.extract::<PyReadonlyArray1<'_, f64>>() {
            return Ok(a.as_array().mapv(|x| x as Float));
        }
        if let Ok(a) = v.extract::<PyReadonlyArray1<'_, f32>>() {
            return Ok(a.as_array().mapv(|x| x as Float));
        }
        v.extract::<Vec<Float>>().map(ndarray::Array1::from).map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(format!(
                "{name} must be one number per facet: a 1-D array or a list"
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A facet tilted away from the Sun absorbs nothing. Negative insolation
    /// would drive night-side temperatures below the radiative balance rather
    /// than simply leaving them unforced.
    #[test]
    fn insolation_is_never_negative() {
        for cosi in [-1.0, -0.5, -1e-9] {
            assert_eq!(radiation_sun(1.0, cosi, 0.07), 0.0, "cosi={cosi}");
            assert_eq!(
                radiation_sun_reflected(0.5, 0.07, cosi, 1.0),
                0.0,
                "cosi={cosi}"
            );
        }
    }

    /// The clamp must not disturb the lit case.
    #[test]
    fn insolation_unchanged_when_lit() {
        let expected = crate::util::SOLAR_CONSTANT * (1.0 - 0.07) * 0.5 / 4.0;
        assert!((radiation_sun(2.0, 0.5, 0.07) - expected).abs() < 1e-6);
        assert!(radiation_sun(1.0, 1.0, 0.07) > 0.0);
    }

    fn surface(ti: Float) -> Properties {
        let mut p = Properties {
            albedo: 0.1,
            emissivity: 0.9,
            density: 2000.0,
            heat_capacity: 600.0,
            thermal_inertia: ti,
            ..Default::default()
        };
        p.compute_conductivity_diffusivity();
        p
    }

    /// Every facet at once is each column on its own: the same Newton
    /// balance at the surface, the same stencil below, to the bit.
    #[test]
    fn every_facet_at_once_is_each_column_alone() {
        let prop = surface(200.0);
        let (layers, facets, dz, dt) = (12, 5, 2e-3, 30.0);
        let mut t = columns(layers, facets, 0.0);
        for ((z, i), v) in t.indexed_iter_mut() {
            *v = 150.0 + 20.0 * i as Float + 3.0 * z as Float;
        }
        let cosi = ndarray::array![1.0, 0.5, 0.0, -0.3, 0.9];
        let before = t.clone();

        solar_bc(t.view_mut(), 1.2, cosi.view(), &prop, dz).unwrap();
        bottom_adiabatic(t.view_mut());
        heat_conduction(t.view_mut(), &prop, dt, dz).unwrap();

        let se = crate::util::STEFAN_BOLTZMANN * prop.emissivity;
        let r = stability(prop.diffusivity, dt, dz * dz);
        for i in 0..facets {
            let mut c = before.column(i).to_owned();
            let f = radiation_sun(1.2, cosi[i], prop.albedo);
            c[0] = newton_method(c[0], f, se, prop.conductivity, c[1], c[2], 2.0 * dz).unwrap();
            c[layers - 1] = c[layers - 2];
            let d = Array1::from_elem(layers, prop.diffusivity);
            let inner = conduction_1d(c.view(), d.view(), Array1::from_elem(layers - 2, r / prop.diffusivity).view());
            c.slice_mut(s![1..-1]).assign(&inner);
            for z in 0..layers {
                assert!((t[[z, i]] - c[z]).abs() <= 1e-4 * c[z], "facet {i} layer {z}: {} against {}", t[[z, i]], c[z]);
            }
        }
    }

    /// In sunlight that never changes, a column settles where the surface
    /// radiates what it absorbs, the same temperature all the way down: the
    /// adiabatic bottom lets nothing out.
    #[test]
    fn a_column_in_constant_sunlight_settles_at_radiative_balance() {
        let prop = surface(50.0);
        let (layers, dz) = (16, 1e-3);
        let dt = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let balance = (radiation_sun(1.0, 1.0, prop.albedo)
            / (prop.emissivity * crate::util::STEFAN_BOLTZMANN))
            .powf(0.25);
        let mut t = columns(layers, 1, 200.0);
        let cosi = ndarray::array![1.0];
        for _ in 0..20_000 {
            solar_bc(t.view_mut(), 1.0, cosi.view(), &prop, dz).unwrap();
            bottom_adiabatic(t.view_mut());
            heat_conduction(t.view_mut(), &prop, dt, dz).unwrap();
        }
        for z in 0..layers {
            assert!((t[[z, 0]] - balance).abs() < 0.05, "layer {z}: {} K, balance {balance} K", t[[z, 0]]);
        }
    }

    /// A column on a spinning body's equator, spun up: over a rotation it
    /// radiates what it absorbed -- 0.0002 % apart -- the day's heat given
    /// back at night, and its bottom stirs only as much as the wave that
    /// reaches it.
    #[test]
    fn a_spinning_column_gives_back_what_it_absorbs() {
        let prop = surface(200.0);
        let period = 6.0 * 3600.0;
        let ls = super::super::properties::skin_depth_1(prop.diffusivity, period);
        let dz = ls / 8.0;
        let layers = (super::super::properties::skin_depth_2pi(prop.diffusivity, period) / dz).round() as usize + 1;
        let steps = (period / stability_maxdt(prop.diffusivity, dz * dz, 0.5)).ceil() as usize;
        let dt = period / steps as Float;

        let mut t = columns(layers, 1, 280.0);
        let mut cosi = ndarray::array![0.0];
        let (mut absorbed, mut emitted) = (0.0, 0.0);
        let (mut bottom_min, mut bottom_max) = (Float::MAX, Float::MIN);
        let (mut surf_min, mut surf_max) = (Float::MAX, Float::MIN);
        let spins = 40;
        for it in 0..spins * steps {
            cosi[0] = (2.0 * crate::util::PI * (it % steps) as Float / steps as Float).cos();
            solar_bc(t.view_mut(), 1.0, cosi.view(), &prop, dz).unwrap();
            bottom_adiabatic(t.view_mut());
            heat_conduction(t.view_mut(), &prop, dt, dz).unwrap();
            if it >= (spins - 1) * steps {
                surf_min = surf_min.min(t[[0, 0]]);
                surf_max = surf_max.max(t[[0, 0]]);
                absorbed += radiation_sun(1.0, cosi[0], prop.albedo);
                emitted += prop.emissivity * crate::util::STEFAN_BOLTZMANN * t[[0, 0]].powi(4);
                bottom_min = bottom_min.min(t[[layers - 1, 0]]);
                bottom_max = bottom_max.max(t[[layers - 1, 0]]);
            }
        }
        let gap = (emitted - absorbed) / absorbed;
        assert!(gap.abs() < 0.01, "emitted {emitted}, absorbed {absorbed}: {:.3} %", 100.0 * gap);
        // The wave arrives at the bottom e^-2pi of the surface's, and the wall
        // doubles it: 0.58 K of a 156 K day, where 0.64 K is measured.
        let (surface, bottom) = (surf_max - surf_min, bottom_max - bottom_min);
        let wave = 2.0 * (-2.0 * crate::util::PI).exp() * surface;
        assert!(bottom < 1.25 * wave, "the bottom swings {bottom} K, {wave} K expected of a {surface} K day");
    }

    #[test]
    fn conduction_refuses_what_would_blow_up_or_do_nothing() {
        let prop = surface(200.0);
        let dz = 1e-3;
        let limit = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let mut t = columns(8, 3, 250.0);
        assert!(heat_conduction(t.view_mut(), &prop, limit, dz).is_ok(), "at the limit");
        let err = heat_conduction(t.view_mut(), &prop, 1.01 * limit, dz).unwrap_err();
        assert!(err.to_string().contains("stability_maxdt"), "{err}");

        let raw = Properties { thermal_inertia: 200.0, ..surface(200.0) };
        let raw = Properties { diffusivity: 0.0, ..raw };
        let err = heat_conduction(t.view_mut(), &raw, 1.0, dz).unwrap_err();
        assert!(err.to_string().contains("compute_conductivity_diffusivity"), "{err}");

        let err = solar_bc(t.view_mut(), 1.0, ndarray::array![1.0, 0.0].view(), &prop, dz).unwrap_err();
        assert!(err.to_string().contains("2 cosines of incidence for 3 facets"), "{err}");
    }
}
