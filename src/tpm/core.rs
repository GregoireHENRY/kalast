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
use ndarray::{Array1, Array2, ArrayView1, ArrayView2, ArrayViewMut2, Zip, s};
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

/// The temperature (K) at which a surface radiates what it absorbs of the
/// Sun on average, `dau` AU away, its albedo `a` and emissivity `e`:
///
/// ```text
/// e sigma T^4 = S (1 - A) r / dau^2
/// ```
///
/// `r` the ratio of the area receiving sunlight to the area emitting, which
/// is the mean cosine of incidence: 1/4 over a whole sphere, the Sun
/// shared out over its every latitude; one latitude's over a spin,
/// `mean_incidence(lat, dec)`. A start for a thermophysical model's
/// temperatures, near where they settle.
pub fn effective_temperature(dau: Float, r: Float, a: Float, e: Float) -> Float {
    (crate::util::SOLAR_CONSTANT * r * (1.0 - a)
        / (e * crate::util::STEFAN_BOLTZMANN * dau.powi(2)))
    .powf(0.25)
}

/// The cosine of incidence averaged over a spin, counted while the Sun is
/// up: the mean sunlight on ground facing latitude `lat`, the Sun at
/// latitude `dec` -- both radians from the spin's equator, `dec` the
/// subsolar latitude, which is the obliquity at a solstice and 0 at an
/// equinox. Times the solar flux, the ground's mean insolation over a day:
///
/// ```text
/// <max(cos i, 0)> = (h0 sin(lat) sin(dec) + sin(h0) cos(lat) cos(dec)) / pi
/// cos(h0) = -tan(lat) tan(dec)
/// ```
///
/// `h0` the hour angle of sunset: `pi` where the Sun never sets, the polar
/// day, and 0 where it never rises, the polar night, which gets nothing.
/// `1/pi` on the equator with the Sun over it. Over a whole sphere, each
/// latitude weighed by its area, `1/4` whatever `dec`: the same sunlight,
/// shared out otherwise.
///
/// `lat` is where the ground faces, the latitude of its normal -- on a
/// sphere, where it lies. The rest of the body's shadow is not counted.
pub fn mean_incidence(lat: Float, dec: Float) -> Float {
    use crate::util::PI;
    let (s, c) = (lat.sin() * dec.sin(), lat.cos() * dec.cos());
    let h0 = if s >= c {
        PI // the polar day
    } else if -s >= c {
        0.0 // the polar night
    } else {
        (-s / c).acos()
    };
    ((h0 * s + h0.sin() * c) / PI).max(0.0)
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
/// below is a sweep over whole rows. The layers are equal, `dz` apart: row
/// `j` at depth `j dz`, as the three steps take them.
pub fn columns(layers: usize, facets: usize, t: Float) -> Array2<Float> {
    Array2::from_elem((layers, facets), t)
}

/// What the thermal steps read a body's properties from: one `Properties`
/// for every facet and every layer, or a `Ground` giving them facet by facet
/// and layer by layer. Either converts: `&prop`, `&ground`, `ground.view()`.
#[derive(Clone, Copy)]
pub enum Thermal<'a> {
    Body(&'a Properties),
    Ground(GroundView<'a>),
}

impl<'a> From<&'a Properties> for Thermal<'a> {
    fn from(p: &'a Properties) -> Self {
        Self::Body(p)
    }
}

impl<'a> From<&'a Ground> for Thermal<'a> {
    fn from(g: &'a Ground) -> Self {
        Self::Ground(g.view())
    }
}

impl<'a> From<GroundView<'a>> for Thermal<'a> {
    fn from(g: GroundView<'a>) -> Self {
        Self::Ground(g)
    }
}

/// A body's thermal properties facet by facet and layer by layer, where one
/// `Properties` for the whole body is not enough: a darker patch, bare rock
/// under a crater, a fluffy layer over a denser one.
///
/// At the surface, per facet: `albedo` and `emissivity`, `(facets,)`. Below
/// it, per layer and facet, as the temperatures are laid out:
/// `conductivity`, `density` and `heat_capacity`, `(layers, facets)`. Any of
/// them may be of a shape that broadcasts to its own instead, as numpy
/// broadcasts: `(1,)` or `(1, 1)` one value for the body, `(1, facets)` the
/// same all the way down each column, `(layers, 1)` the same across the body
/// at each depth -- a property that does not vary costs a value, a row or a
/// column rather than the whole array.
#[derive(Clone, Debug)]
pub struct Ground {
    pub albedo: Array1<Float>,
    pub emissivity: Array1<Float>,
    pub conductivity: Array2<Float>,
    pub density: Array2<Float>,
    pub heat_capacity: Array2<Float>,
    layers: usize,
    facets: usize,
}

impl Ground {
    /// Every facet and layer as `prop` has them, in full arrays to be changed
    /// where they differ. Its conductivity has to have been computed from
    /// the thermal inertia (`compute_conductivity_diffusivity`).
    pub fn new(prop: &Properties, layers: usize, facets: usize) -> Result<Self> {
        if !(prop.conductivity > 0.0) {
            bail!(
                "the conductivity is {}: compute it from the thermal inertia first, \
                 with compute_conductivity_diffusivity()",
                prop.conductivity
            );
        }
        Ok(Self {
            albedo: Array1::from_elem(facets, prop.albedo),
            emissivity: Array1::from_elem(facets, prop.emissivity),
            conductivity: Array2::from_elem((layers, facets), prop.conductivity),
            density: Array2::from_elem((layers, facets), prop.density),
            heat_capacity: Array2::from_elem((layers, facets), prop.heat_capacity),
            layers,
            facets,
        })
    }

    /// A column graded with depth: the first three layers `dz` thick -- the
    /// surface's gradient reads them -- then each `ratio` times the one
    /// above, down past `depth` (m). Thin where a day's wave is, thick where
    /// only a year's reaches: 36 layers from 8 mm to 20 m at `ratio` 1.2,
    /// where layers all 8 mm would take 2,500.
    ///
    /// Still a `Ground` of layers `dz` apart, as `solar_bc` and
    /// `heat_conduction` take one: each layer's width `w` is carried by its
    /// properties -- the conductivity times `dz / w`, the density times
    /// `w / dz` -- which is the conservative scheme on the graded cells
    /// exactly,
    ///
    /// ```text
    /// rho c w_i dT_i/dt = k (T_{i+1} - T_i) / d_{i+1/2} - k (T_i - T_{i-1}) / d_{i-1/2}
    /// ```
    ///
    /// `d` the distance between two layers' middles, the mean of their
    /// widths: the harmonic mean of `k dz / w` over two layers is `k dz / d`.
    /// Its stable step is the thinnest layers', `stability_maxdt(dz)`.
    /// Returns it with the widths (m), top down.
    pub fn graded(prop: &Properties, facets: usize, dz: Float, depth: Float, ratio: Float) -> Result<(Self, Vec<Float>)> {
        if !(dz > 0.0 && ratio >= 1.0 && depth >= 3.0 * dz && depth.is_finite()) {
            bail!(
                "a graded column wants dz > 0, ratio >= 1 and depth >= 3 dz: not dz = {dz} m, \
                 ratio = {ratio}, depth = {depth} m"
            );
        }
        let mut widths = vec![dz; 3];
        while widths.iter().sum::<Float>() < depth {
            widths.push(widths[widths.len() - 1] * ratio);
        }
        let layers = widths.len();
        let mut ground = Self::new(prop, layers, facets)?;
        ground.conductivity = Array2::from_shape_fn((layers, 1), |(i, _)| prop.conductivity * dz / widths[i]);
        ground.density = Array2::from_shape_fn((layers, 1), |(i, _)| prop.density * widths[i] / dz);
        ground.heat_capacity = Array2::from_elem((1, 1), prop.heat_capacity);
        Ok((ground, widths))
    }

    /// `(layers, facets)`: the temperatures' shape, which every array
    /// broadcasts to.
    pub fn dim(&self) -> (usize, usize) {
        (self.layers, self.facets)
    }

    pub fn view(&self) -> GroundView<'_> {
        GroundView {
            albedo: self.albedo.view(),
            emissivity: self.emissivity.view(),
            conductivity: self.conductivity.view(),
            density: self.density.view(),
            heat_capacity: self.heat_capacity.view(),
        }
    }

    /// The largest stable time step (s) for layers `dz` (m) apart:
    /// `GroundView::stability_maxdt`.
    pub fn stability_maxdt(&self, dz: Float, s: Float) -> Result<Float> {
        self.view().stability_maxdt(self.layers, self.facets, dz, s)
    }
}

/// A `Ground` borrowed: what the steps read, whether it lives in Rust or in
/// the numpy arrays a script edits.
#[derive(Clone, Copy)]
pub struct GroundView<'a> {
    pub albedo: ArrayView1<'a, Float>,
    pub emissivity: ArrayView1<'a, Float>,
    pub conductivity: ArrayView2<'a, Float>,
    pub density: ArrayView2<'a, Float>,
    pub heat_capacity: ArrayView2<'a, Float>,
}

impl<'a> GroundView<'a> {
    /// Each array against `(layers, facets)`: its own shape, or one that
    /// broadcasts to it.
    pub fn check(&self, layers: usize, facets: usize) -> Result<()> {
        for (name, a) in [("albedo", &self.albedo), ("emissivity", &self.emissivity)] {
            if a.len() != facets && a.len() != 1 {
                bail!("{name} has {} values for {facets} facets: one a facet, or one for all", a.len());
            }
        }
        for (name, a) in [
            ("conductivity", &self.conductivity),
            ("density", &self.density),
            ("heat_capacity", &self.heat_capacity),
        ] {
            let (r, c) = a.dim();
            if !((r == layers || r == 1) && (c == facets || c == 1)) {
                bail!(
                    "{name} is ({r}, {c}), which does not broadcast to ({layers}, {facets}), \
                     layers by facets: (layers, facets), (1, facets), (layers, 1) or (1, 1)"
                );
            }
        }
        Ok(())
    }

    /// Layer `i` of a per-node array: its row, one value or one a facet --
    /// broadcast where it is read (`Zip::and_broadcast`).
    fn layer<'b>(a: &'b ArrayView2<'a, Float>, i: usize) -> ArrayView1<'b, Float> {
        a.row(if a.nrows() == 1 { 0 } else { i })
    }

    /// The largest stable time step (s) for layers `dz` (m) apart. The
    /// explicit step is stable while, at every interior node,
    ///
    /// ```text
    /// dt (k_{i-1/2} + k_{i+1/2}) / (rho c_i dz^2)  <=  2 s
    /// ```
    ///
    /// -- `s = 1/2` its limit -- with `k` at a boundary between layers the
    /// harmonic mean of the two. For one material this is
    /// `stability_maxdt(D, dz^2, s)`.
    pub fn stability_maxdt(&self, layers: usize, facets: usize, dz: Float, s: Float) -> Result<Float> {
        self.check(layers, facets)?;
        let row = |a: &ArrayView2<'_, Float>, i: usize| -> Vec<Float> {
            let r = GroundView::layer(a, i);
            if r.len() == 1 { vec![r[0]; facets] } else { r.to_vec() }
        };
        let mut best = Float::INFINITY;
        let (mut k_up, mut k_here) = (row(&self.conductivity, 0), row(&self.conductivity, 1));
        for i in 1..layers.saturating_sub(1) {
            let k_next = row(&self.conductivity, i + 1);
            let (rho, c) = (row(&self.density, i), row(&self.heat_capacity, i));
            {
                let (k_up, k_here, k_next, rho, c) =
                    (&k_up[..facets], &k_here[..facets], &k_next[..facets], &rho[..facets], &c[..facets]);
                for f in 0..facets {
                    let flow = harmonic(k_up[f], k_here[f]) + harmonic(k_here[f], k_next[f]);
                    // No conduction, no limit: an infinite step, which `min`
                    // passes over.
                    best = best.min(2.0 * s * rho[f] * c[f] * dz * dz / flow);
                }
            }
            k_up = std::mem::replace(&mut k_here, k_next);
        }
        Ok(best)
    }
}

/// The conductivity at the boundary between two layers: the harmonic mean,
/// which is what the two in series conduct, half a layer of each -- `0` where
/// either does not conduct.
fn harmonic(a: Float, b: Float) -> Float {
    2.0 * a * b / (a + b).max(Float::MIN_POSITIVE)
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
/// The method: the balance is solved for `T0` by Newton's method, from the
/// facet's surface temperature the step before, to `NEWTON_METHOD_THRESHOLD`
/// (0.1 K) -- implicit in the surface, with the layers under it as they
/// stand. The gradient into the column is the second-order one-sided
/// difference above.
///
/// With a `Ground`, each facet's own albedo and emissivity, and the
/// conductivity of its top layer.
///
/// An error names the first facet whose balance did not converge. That is a
/// run already gone wrong under it -- temperatures run away or NaN from a
/// time step past `stability_maxdt` -- and nothing to continue from.
pub fn solar_bc<'a>(
    mut t: ArrayViewMut2<'_, Float>,
    dau: Float,
    cosi: ArrayView1<'_, Float>,
    thermal: impl Into<Thermal<'a>>,
    dz: Float,
) -> Result<()> {
    let (layers, facets) = t.dim();
    if layers < 3 {
        bail!("the surface balance needs 3 layers or more, got {layers}");
    }
    if cosi.len() != facets {
        bail!("{} cosines of incidence for {facets} facets", cosi.len());
    }
    let twodz = 2.0 * dz;
    let (mut surface, below) = t.multi_slice_mut((s![0, ..], s![1..3, ..]));
    let mut failed = None;
    match thermal.into() {
        Thermal::Body(prop) => {
            let se = crate::util::STEFAN_BOLTZMANN * prop.emissivity;
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
        }
        Thermal::Ground(g) => {
            g.check(layers, facets)?;
            let albedo = g.albedo.broadcast(facets).expect("checked");
            let emissivity = g.emissivity.broadcast(facets).expect("checked");
            let mut k = Array1::zeros(facets);
            k.assign(&GroundView::layer(&g.conductivity, 0));
            for i in 0..facets {
                let (t1, t2) = (below[[0, i]], below[[1, i]]);
                let f = radiation_sun(dau, cosi[i], albedo[i]);
                let se = crate::util::STEFAN_BOLTZMANN * emissivity[i];
                match newton_method(surface[i], f, se, k[i], t1, t2, twodz) {
                    Ok(t) => surface[i] = t,
                    Err(_) => {
                        failed = Some((i, surface[i], t1));
                        break;
                    }
                }
            }
        }
    }
    match failed {
        Some((i, t0, t1)) => bail!(
            "the surface balance of facet {i} did not converge (T0 = {t0} K, T1 = {t1} K): \
             the temperatures under it have run away, a time step past stability_maxdt?"
        ),
        None => Ok(()),
    }
}

/// The adiabatic bottom, every facet at once: no heat through the base of a
/// column, its last layer at the temperature of the one above --
/// `T[-1] = T[-2]`, the first-order form of `dT/dz = 0` there.
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

/// Heat conduction, every facet at once: one step of `dt` (s) of the heat
/// equation `dT/dt = D d2T/dz2` through the interior of each column, its
/// layers `dz` (m) apart.
///
/// The method: explicit finite differences, forward in time and centred in
/// depth (FTCS) -- a forward Euler step of the second difference,
///
/// ```text
/// T_i  +=  D dt / dz^2  (T_{i-1} - 2 T_i + T_{i+1})
/// ```
///
/// on equal layers and at a fixed step: first-order accurate in time,
/// second-order in depth, every layer stepped from the temperatures of the
/// step before. `conduction_1d` on each column of `t`. The surface and the
/// bottom layer are the boundary conditions', `solar_bc` and
/// `bottom_adiabatic`.
///
/// With a `Ground`, conductivity `k`, density `rho` and heat capacity `c` a
/// layer and a facet each, the same scheme in the form that keeps the heat
/// flow continuous where the material changes -- the balance of what crosses
/// the boundary above a layer and the one below it,
///
/// ```text
/// rho c_i (T_i' - T_i) / dt  =  [ k_{i+1/2} (T_{i+1} - T_i) - k_{i-1/2} (T_i - T_{i-1}) ] / dz^2
/// ```
///
/// with `k` at a boundary the harmonic mean of the two layers, what two
/// half-layers conduct in series. A diffusivity per layer in the stencil
/// above would not do: it drops the change of `k` with depth, and with it
/// the flow across a boundary between two materials.
///
/// Refused past the scheme's stability, `D dt / dz^2 > 1/2`, where it does
/// not lose accuracy so much as grow without bound; `stability_maxdt` gives
/// the largest `dt` -- `Ground::stability_maxdt` for a `Ground`, whose
/// check runs as the layers are stepped: the step stops at the first layer
/// past it, the ones above it already stepped. Refused too with no
/// diffusivity -- a `Properties` whose `compute_conductivity_diffusivity`
/// was never called -- where nothing would conduct and nothing would say so.
pub fn heat_conduction<'a>(
    t: ArrayViewMut2<'_, Float>,
    thermal: impl Into<Thermal<'a>>,
    dt: Float,
    dz: Float,
) -> Result<()> {
    match thermal.into() {
        Thermal::Body(prop) => conduction_body(t, prop, dt, dz),
        Thermal::Ground(g) => conduction_ground(t, g, dt, dz),
    }
}

/// `heat_conduction` with one material: `D dt / dz^2` for every node.
fn conduction_body(mut t: ArrayViewMut2<'_, Float>, prop: &Properties, dt: Float, dz: Float) -> Result<()> {
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

/// `heat_conduction` with a `Ground`: the conservative stencil, a layer at a
/// time across the facets, from the old temperatures only.
///
/// One pass a layer, fused: the boundary's conductivity, the layer's rate and
/// the step, each node read once, from the arrays themselves where a row is
/// contiguous and from a row filled out where it is broadcast -- straight
/// loops over slices cut to `facets`, which the compiler vectorises. Through
/// `Zip`, with the facet index and a pass per coefficient, this took twelve
/// times the uniform step at 5120 facets by 51 layers. The stability is kept
/// as a max and a min, reductions that vectorise too, and checked after each
/// layer: a step past it stops there, with the layers down to it stepped.
fn conduction_ground(mut t: ArrayViewMut2<'_, Float>, g: GroundView<'_>, dt: Float, dz: Float) -> Result<()> {
    let (layers, facets) = t.dim();
    g.check(layers, facets)?;
    if layers < 3 {
        return Ok(());
    }
    let dtdz2 = dt / (dz * dz);
    // A few ulps over the limit are the rounding of a `dt` taken at it.
    let limit = 1.0 + 16.0 * Float::EPSILON;
    let zeros = || vec![0.0 as Float; facets];
    // Buffers for broadcast rows only; a full row is read in place.
    let (mut b_here, mut b_next, mut b_rho, mut b_c) = (RowBuf::new(facets), RowBuf::new(facets), RowBuf::new(facets), RowBuf::new(facets));
    // The layer above as it was, and the conductivity of the boundary above
    // the layer being stepped, `k_{i-1/2}`.
    let mut above: Vec<Float> = t.row(0).iter().copied().collect();
    let mut k_up = zeros();
    {
        let (k0, k1) = (row_of(&g.conductivity, 0, facets, &mut b_here), row_of(&g.conductivity, 1, facets, &mut b_next));
        for ((o, &a), &b) in k_up.iter_mut().zip(k0).zip(k1) {
            *o = harmonic(a, b);
        }
    }
    for i in 1..layers - 1 {
        let k_here = row_of(&g.conductivity, i, facets, &mut b_here);
        let k_next = row_of(&g.conductivity, i + 1, facets, &mut b_next);
        let rho = row_of(&g.density, i, facets, &mut b_rho);
        let c = row_of(&g.heat_capacity, i, facets, &mut b_c);
        let (mut worst, mut lowest) = (0.0 as Float, 0.0 as Float);
        let (mut layer, below) = t.multi_slice_mut((s![i, ..], s![i + 1, ..]));
        match (layer.as_slice_mut(), below.as_slice()) {
            (Some(layer), Some(below)) => {
                let (layer, below, above, k_up) =
                    (&mut layer[..facets], &below[..facets], &mut above[..facets], &mut k_up[..facets]);
                let (k_here, k_next, rho, c) = (&k_here[..facets], &k_next[..facets], &rho[..facets], &c[..facets]);
                for f in 0..facets {
                    let kd = harmonic(k_here[f], k_next[f]);
                    let r = dtdz2 / (rho[f] * c[f]);
                    worst = worst.max(r * (k_up[f] + kd));
                    lowest = lowest.min(r.min(kd));
                    let v = layer[f];
                    layer[f] = v + r * (kd * (below[f] - v) - k_up[f] * (v - above[f]));
                    above[f] = v;
                    k_up[f] = kd;
                }
            }
            // A view with gaps between its values: the same, value by value.
            _ => {
                for f in 0..facets {
                    let kd = harmonic(k_here[f], k_next[f]);
                    let r = dtdz2 / (rho[f] * c[f]);
                    worst = worst.max(r * (k_up[f] + kd));
                    lowest = lowest.min(r.min(kd));
                    let v = layer[f];
                    layer[f] = v + r * (kd * (below[f] - v) - k_up[f] * (v - above[f]));
                    above[f] = v;
                    k_up[f] = kd;
                }
            }
        }
        if worst > limit || lowest < 0.0 {
            bail!(
                "unstable at layer {i}: D dt / dz^2 reaches {} of the 1/2 its layers allow \
                 (ground.stability_maxdt gives the largest dt); the layers down to it are stepped",
                worst / 2.0
            );
        }
    }
    Ok(())
}

/// A row filled out from a broadcast one, remembering the value it holds,
/// so that one value for the whole body is written once a step, not once a
/// layer.
struct RowBuf {
    values: Vec<Float>,
    holds: Option<Float>,
}

impl RowBuf {
    fn new(facets: usize) -> Self {
        Self { values: vec![0.0; facets], holds: None }
    }
}

/// Layer `i` of a per-node array, as a slice of `facets` values: the array's
/// own row where it is contiguous and full, else `buf` filled from it -- one
/// value broadcast, or a row with gaps.
fn row_of<'s>(a: &'s ArrayView2<'_, Float>, i: usize, facets: usize, buf: &'s mut RowBuf) -> &'s [Float] {
    let r = GroundView::layer(a, i);
    if r.len() == facets {
        if let Some(values) = r.to_slice() {
            return values;
        }
        buf.values.iter_mut().zip(r.iter()).for_each(|(o, &v)| *o = v);
        buf.holds = None;
    } else if buf.holds != Some(r[0]) {
        buf.values.fill(r[0]);
        buf.holds = Some(r[0]);
    }
    &buf.values
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{IntoPyArray, PyArray1, PyArray2, PyArrayMethods, PyReadonlyArray1, ToPyArray};
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
    /// `facets`, starting at `t` (K), for `solar_bc`, `bottom_adiabatic` and
    /// `heat_conduction` to step in place.
    ///
    /// `(layers, facets)`: `t[0]` is the surface, one temperature per facet,
    /// ready for `mesh.values`; `t[-1]` the bottom; `t[:, i]` the ground under
    /// facet `i`. The layers are equal, `dz` apart: `t[j]` at depth `j dz`.
    /// Made here rather than with `numpy.full` so that it has the float type
    /// kalast was built with.
    ///
    /// `t` one temperature for the whole body, or one a facet, each column
    /// starting at its own -- any shape that broadcasts to `(layers,
    /// facets)`, as numpy broadcasts.
    #[cfg_attr(feature = "python", pyfunction)]
    #[cfg_attr(feature = "python", pyo3(signature = (layers, facets, t: "numpy.ndarray | Sequence[float] | float")))]
    pub fn columns<'py>(
        py: Python<'py>,
        layers: usize,
        facets: usize,
        t: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyArray2<Float>>> {
        let t = floats(t)?;
        let shape = node_shape(t.shape(), layers, facets, "t")?;
        let t = t.into_shape_with_order(shape).expect("the same number of values");
        Ok(t.broadcast((layers, facets)).expect("a shape that broadcasts").to_owned().into_pyarray(py))
    }

    /// The temperature (K) at which a surface radiates what it absorbs of
    /// the Sun on average, `dau` AU away, its albedo `a` and emissivity `e`:
    ///
    /// ```text
    /// e sigma T^4 = S (1 - A) r / dau^2
    /// ```
    ///
    /// `r` the ratio of the area receiving sunlight to the area emitting,
    /// which is the mean cosine of incidence: 1/4 over a whole sphere; one
    /// latitude's over a spin, `mean_incidence(lat, dec)`. A start for a
    /// thermophysical model's temperatures, near where they settle.
    ///
    /// `r`, `a` and `e` each a number or an array -- one a facet, say -- as
    /// numpy broadcasts them, and the temperatures come back in their
    /// shape: a start for `columns`, each column at its own.
    /// :pytype: float | numpy.ndarray
    #[cfg_attr(feature = "python", pyfunction)]
    #[cfg_attr(feature = "python", pyo3(signature = (
        dau,
        r: "numpy.ndarray | Sequence[float] | float",
        a: "numpy.ndarray | Sequence[float] | float",
        e: "numpy.ndarray | Sequence[float] | float",
    )))]
    pub fn effective_temperature(
        py: Python<'_>,
        dau: Float,
        r: &Bound<'_, PyAny>,
        a: &Bound<'_, PyAny>,
        e: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        let given = [("r", floats(r)?), ("a", floats(a)?), ("e", floats(e)?)];
        let [r, a, e] = broadcast(&given)?;
        let t = ndarray::Zip::from(&r)
            .and(&a)
            .and(&e)
            .map_collect(|&r, &a, &e| super::effective_temperature(dau, r, a, e));
        number_or_array(py, t)
    }

    /// The cosine of incidence averaged over a spin, counted while the Sun
    /// is up: the mean sunlight on ground facing latitude `lat`, the Sun at
    /// latitude `dec` -- both radians from the spin's equator, `dec` the
    /// subsolar latitude, which is the obliquity at a solstice and 0 at an
    /// equinox. Times the solar flux, the ground's mean insolation over a
    /// day:
    ///
    /// ```text
    /// <max(cos i, 0)> = (h0 sin(lat) sin(dec) + sin(h0) cos(lat) cos(dec)) / pi
    /// cos(h0) = -tan(lat) tan(dec)
    /// ```
    ///
    /// `h0` the hour angle of sunset: `pi` where the Sun never sets, the
    /// polar day, and 0 where it never rises, the polar night, which gets
    /// nothing. `1/pi` on the equator with the Sun over it. Over a whole
    /// sphere, each latitude weighed by its area, `1/4` whatever `dec`.
    ///
    /// `lat` is where the ground faces, the latitude of its normal -- on a
    /// sphere, where it lies: each facet's, `numpy.arcsin(normals @
    /// spin_axis)`. The rest of the body's shadow is not counted. `lat` and
    /// `dec` each a number or an array, as numpy broadcasts them, and the
    /// means come back in their shape -- for `effective_temperature`'s `r`.
    /// :pytype: float | numpy.ndarray
    #[cfg_attr(feature = "python", pyfunction)]
    #[cfg_attr(feature = "python", pyo3(signature = (
        lat: "numpy.ndarray | Sequence[float] | float",
        dec: "numpy.ndarray | Sequence[float] | float",
    )))]
    pub fn mean_incidence(py: Python<'_>, lat: &Bound<'_, PyAny>, dec: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let given = [("lat", floats(lat)?), ("dec", floats(dec)?)];
        let [lat, dec] = broadcast(&given)?;
        let r = ndarray::Zip::from(&lat).and(&dec).map_collect(|&lat, &dec| super::mean_incidence(lat, dec));
        number_or_array(py, r)
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
    /// The method: `T0` solved by Newton's method, from the facet's surface
    /// temperature the step before, to 0.1 K -- implicit in the surface, the
    /// layers under it as they stand -- with the gradient into the column
    /// the second-order one-sided difference above.
    ///
    /// `t` is what `columns` made, changed in place; `dau` the distance to
    /// the Sun (AU); `cosi` a cosine of incidence per facet, as
    /// `sim.facet_incidence` gives them; `prop` the body's `Properties`, or a
    /// `Ground` for each facet's own albedo, emissivity and top-layer
    /// conductivity; `dz` the thickness of a layer (m).
    #[cfg_attr(feature = "python", pyfunction)]
    #[cfg_attr(feature = "python", pyo3(signature = (t: "numpy.ndarray", dau, cosi: "numpy.ndarray", prop: "Properties | Ground", dz)))]
    pub fn solar_bc(
        t: &Bound<'_, PyAny>,
        dau: Float,
        cosi: &Bound<'_, PyAny>,
        prop: &Bound<'_, PyAny>,
        dz: Float,
    ) -> PyResult<()> {
        let mut t = temperatures(t)?;
        let cosi = per_facet(cosi, "cosi")?;
        if let Ok(g) = prop.extract::<PyRef<'_, Ground>>() {
            let arrays = g.readonly(prop.py())?;
            return super::solar_bc(t.as_array_mut(), dau, cosi.view(), arrays.view(), dz).map_err(value_error);
        }
        let prop = body(prop)?;
        super::solar_bc(t.as_array_mut(), dau, cosi.view(), &*prop.inner.borrow(), dz).map_err(value_error)
    }

    /// The adiabatic bottom, every facet at once: no heat through the base
    /// of a column, its last layer, `t[-1]`, at the temperature of the one
    /// above -- `t[-1] = t[-2]`, the first-order form of `dT/dz = 0` there.
    /// `t` is what `columns` made, changed in place.
    #[cfg_attr(feature = "python", pyfunction)]
    pub fn bottom_adiabatic(t: &Bound<'_, PyAny>) -> PyResult<()> {
        super::bottom_adiabatic(temperatures(t)?.as_array_mut());
        Ok(())
    }

    /// Heat conduction, every facet at once: one step of `dt` (s) of the heat
    /// equation `dT/dt = D d2T/dz2` through the interior of each column, its
    /// layers `dz` (m) apart.
    ///
    /// The method: explicit finite differences, forward in time and centred
    /// in depth (FTCS) -- a forward Euler step of the second difference,
    ///
    /// ```text
    /// T_i  +=  D dt / dz^2  (T_{i-1} - 2 T_i + T_{i+1})
    /// ```
    ///
    /// on equal layers and at a fixed step: first-order accurate in time,
    /// second-order in depth, every layer stepped from the temperatures of
    /// the step before. Stable while `D dt / dz^2 <= 1/2`, and refused past
    /// it: `stability_maxdt(D, dz**2, 0.5)` gives the largest `dt`.
    ///
    /// With a `Ground` -- conductivity `k`, density `rho` and heat capacity
    /// `c` a layer and a facet each -- the same scheme in the form that keeps
    /// the heat flow continuous where the material changes,
    ///
    /// ```text
    /// rho c_i (T_i' - T_i) / dt  =  [ k_{i+1/2} (T_{i+1} - T_i) - k_{i-1/2} (T_i - T_{i-1}) ] / dz^2
    /// ```
    ///
    /// `k` at a boundary the harmonic mean of the two layers. Its limit is
    /// `ground.stability_maxdt(dz)`.
    ///
    /// `t` is what `columns` made, changed in place, and `prop` the body's
    /// `Properties` -- `D` their diffusivity -- or a `Ground`. The surface
    /// and the bottom layer are the boundary conditions', `solar_bc` and
    /// `bottom_adiabatic`.
    #[cfg_attr(feature = "python", pyfunction)]
    #[cfg_attr(feature = "python", pyo3(signature = (t: "numpy.ndarray", prop: "Properties | Ground", dt, dz)))]
    pub fn heat_conduction(
        t: &Bound<'_, PyAny>,
        prop: &Bound<'_, PyAny>,
        dt: Float,
        dz: Float,
    ) -> PyResult<()> {
        let mut t = temperatures(t)?;
        if let Ok(g) = prop.extract::<PyRef<'_, Ground>>() {
            let arrays = g.readonly(prop.py())?;
            return super::heat_conduction(t.as_array_mut(), arrays.view(), dt, dz).map_err(value_error);
        }
        let prop = body(prop)?;
        super::heat_conduction(t.as_array_mut(), &*prop.inner.borrow(), dt, dz).map_err(value_error)
    }

    fn value_error(e: anyhow::Error) -> PyErr {
        pyo3::exceptions::PyValueError::new_err(e.to_string())
    }

    /// A body's `Properties`, or a message saying what else would do.
    fn body(prop: &Bound<'_, PyAny>) -> PyResult<crate::py::tpm::properties::Properties> {
        prop.extract().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err(
                "prop must be the body's Properties, or a Ground for them facet by facet and layer by layer",
            )
        })
    }

    /// A body's thermal properties facet by facet and layer by layer, where
    /// one `Properties` for the body is not enough: a darker patch, bare rock
    /// under a crater, a fluffy layer over a denser one.
    ///
    /// ```python
    /// ground = core.Ground(prop, layers, facets)   # all as prop has them
    /// ground.albedo[dark] = 0.05                   # per facet
    /// ground.conductivity[:6] = 0.002              # the top 6 layers, every facet
    /// ground.conductivity[:, rock] = 0.5           # every layer under some facets
    /// dt = ground.stability_maxdt(dz)             # once: a pass over every node
    /// core.solar_bc(t, dau, cosi, ground, dz)
    /// core.heat_conduction(t, ground, dt, dz)
    /// ```
    ///
    /// Numpy arrays, changed in place: `albedo` and `emissivity` one a facet,
    /// `conductivity`, `density` and `heat_capacity` one a layer and a facet,
    /// `(layers, facets)` as the temperatures are. Assigned, any shape that
    /// broadcasts to that is kept as it is -- a number for the whole body,
    /// `(facets,)` the same all the way down, `(layers, 1)` the same across
    /// the body at each depth -- which on a large mesh saves the memory a
    /// full array takes; to change one in place again, give it its full
    /// shape back.
    #[pyclass(unsendable)]
    pub struct Ground {
        layers: usize,
        facets: usize,
        albedo: Py<PyArray1<Float>>,
        emissivity: Py<PyArray1<Float>>,
        conductivity: Py<PyArray2<Float>>,
        density: Py<PyArray2<Float>>,
        heat_capacity: Py<PyArray2<Float>>,
    }

    /// The arrays borrowed for one call, read as the core reads a `Ground`.
    struct GroundArrays<'py> {
        albedo: numpy::PyReadonlyArray1<'py, Float>,
        emissivity: numpy::PyReadonlyArray1<'py, Float>,
        conductivity: numpy::PyReadonlyArray2<'py, Float>,
        density: numpy::PyReadonlyArray2<'py, Float>,
        heat_capacity: numpy::PyReadonlyArray2<'py, Float>,
    }

    impl GroundArrays<'_> {
        fn view(&self) -> super::GroundView<'_> {
            super::GroundView {
                albedo: self.albedo.as_array(),
                emissivity: self.emissivity.as_array(),
                conductivity: self.conductivity.as_array(),
                density: self.density.as_array(),
                heat_capacity: self.heat_capacity.as_array(),
            }
        }
    }

    impl Ground {
        /// A core `Ground`'s arrays, handed to numpy.
        fn from_core(py: Python<'_>, g: super::Ground) -> Self {
            let (layers, facets) = g.dim();
            Self {
                layers,
                facets,
                albedo: g.albedo.into_pyarray(py).unbind(),
                emissivity: g.emissivity.into_pyarray(py).unbind(),
                conductivity: g.conductivity.into_pyarray(py).unbind(),
                density: g.density.into_pyarray(py).unbind(),
                heat_capacity: g.heat_capacity.into_pyarray(py).unbind(),
            }
        }

        fn readonly<'py>(&self, py: Python<'py>) -> PyResult<GroundArrays<'py>> {
            let busy = |_| pyo3::exceptions::PyValueError::new_err("a Ground's array is being written elsewhere");
            Ok(GroundArrays {
                albedo: self.albedo.bind(py).try_readonly().map_err(busy)?,
                emissivity: self.emissivity.bind(py).try_readonly().map_err(busy)?,
                conductivity: self.conductivity.bind(py).try_readonly().map_err(busy)?,
                density: self.density.bind(py).try_readonly().map_err(busy)?,
                heat_capacity: self.heat_capacity.bind(py).try_readonly().map_err(busy)?,
            })
        }
    }

    #[pymethods]
    impl Ground {
        /// Every facet and layer as `prop` has them, in full arrays to change
        /// where they differ. `prop`'s conductivity has to have been computed
        /// (`compute_conductivity_diffusivity`).
        #[new]
        fn new(py: Python<'_>, prop: crate::py::tpm::properties::Properties, layers: usize, facets: usize) -> PyResult<Self> {
            let g = super::Ground::new(&prop.inner.borrow(), layers, facets).map_err(value_error)?;
            Ok(Self::from_core(py, g))
        }

        /// A column graded with depth, for a day's wave and a year's in one:
        /// the first three layers `dz` thick, then each `ratio` times the one
        /// above, down past `depth` (m) -- 36 layers from 8 mm to 20 m at 1.2,
        /// where layers all 8 mm would take 2,500.
        ///
        /// ```python
        /// dz = properties.skin_depth_1(prop.diffusivity, day) / 4           # the top layers
        /// ground = core.Ground.graded(prop, facets, dz, properties.skin_depth_2pi(prop.diffusivity, year))
        /// t = core.columns(ground.layers, facets, 0.0)
        /// dt = ground.stability_maxdt(dz)                                # the thinnest layers'
        /// core.solar_bc(t, dau, cosi, ground, dz)
        /// core.heat_conduction(t, ground, dt, dz)
        /// ```
        ///
        /// A `Ground` of layers `dz` apart as the steps take one, each
        /// layer's width `w` carried by its properties -- the conductivity
        /// times `dz / w`, the density times `w / dz` -- which is the
        /// conservative scheme on the graded layers exactly. Change them as
        /// for any `Ground`, but as widths: a value set whole is that of a
        /// `dz` layer.
        #[staticmethod]
        #[pyo3(signature = (prop, facets, dz, depth, ratio=1.2))]
        fn graded(
            py: Python<'_>,
            prop: crate::py::tpm::properties::Properties,
            facets: usize,
            dz: Float,
            depth: Float,
            ratio: Float,
        ) -> PyResult<Self> {
            let (g, _) = super::Ground::graded(&prop.inner.borrow(), facets, dz, depth, ratio).map_err(value_error)?;
            Ok(Self::from_core(py, g))
        }

        /// The temperatures' layers, the first axis of every per-layer array.
        #[getter]
        fn layers(&self) -> usize {
            self.layers
        }

        #[getter]
        fn facets(&self) -> usize {
            self.facets
        }

        /// One a facet, `(facets,)`: its share of sunlight reflected.
        #[getter]
        fn albedo(&self, py: Python<'_>) -> Py<PyArray1<Float>> {
            self.albedo.clone_ref(py)
        }

        /// :pytype: numpy.ndarray | Sequence[float] | float
        #[setter]
        fn set_albedo(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
            self.albedo = per_facet_property(v, self.facets, "albedo")?;
            Ok(())
        }

        /// One a facet, `(facets,)`: bolometric, for what the surface radiates.
        #[getter]
        fn emissivity(&self, py: Python<'_>) -> Py<PyArray1<Float>> {
            self.emissivity.clone_ref(py)
        }

        /// :pytype: numpy.ndarray | Sequence[float] | float
        #[setter]
        fn set_emissivity(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
            self.emissivity = per_facet_property(v, self.facets, "emissivity")?;
            Ok(())
        }

        /// One a layer and a facet, `(layers, facets)`, W/m/K.
        #[getter]
        fn conductivity(&self, py: Python<'_>) -> Py<PyArray2<Float>> {
            self.conductivity.clone_ref(py)
        }

        /// :pytype: numpy.ndarray | Sequence[float] | float
        #[setter]
        fn set_conductivity(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
            self.conductivity = per_node_property(v, self.layers, self.facets, "conductivity")?;
            Ok(())
        }

        /// One a layer and a facet, `(layers, facets)`, kg/m3.
        #[getter]
        fn density(&self, py: Python<'_>) -> Py<PyArray2<Float>> {
            self.density.clone_ref(py)
        }

        /// :pytype: numpy.ndarray | Sequence[float] | float
        #[setter]
        fn set_density(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
            self.density = per_node_property(v, self.layers, self.facets, "density")?;
            Ok(())
        }

        /// One a layer and a facet, `(layers, facets)`, J/kg/K.
        #[getter]
        fn heat_capacity(&self, py: Python<'_>) -> Py<PyArray2<Float>> {
            self.heat_capacity.clone_ref(py)
        }

        /// :pytype: numpy.ndarray | Sequence[float] | float
        #[setter]
        fn set_heat_capacity(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
            self.heat_capacity = per_node_property(v, self.layers, self.facets, "heat_capacity")?;
            Ok(())
        }

        /// The largest stable time step (s) for layers `dz` (m) apart, over
        /// every interior node: `dt (k_{i-1/2} + k_{i+1/2}) / (rho c_i dz^2)
        /// <= 2 s`, `s = 1/2` the limit itself. For one material,
        /// `stability_maxdt(D, dz**2, s)`.
        #[pyo3(signature = (dz, s=0.5))]
        fn stability_maxdt(&self, py: Python<'_>, dz: Float, s: Float) -> PyResult<Float> {
            let arrays = self.readonly(py)?;
            arrays.view().stability_maxdt(self.layers, self.facets, dz, s).map_err(value_error)
        }

        fn __repr__(&self) -> String {
            format!("Ground({} layers, {} facets)", self.layers, self.facets)
        }
    }

    /// Numbers from Python: a number, a list, or an array of either float
    /// width, any shape.
    fn floats(v: &Bound<'_, PyAny>) -> PyResult<ndarray::ArrayD<Float>> {
        if let Ok(x) = v.extract::<f64>() {
            return Ok(ndarray::ArrayD::from_elem(ndarray::IxDyn(&[]), x as Float));
        }
        if let Ok(a) = v.extract::<numpy::PyReadonlyArrayDyn<'_, f64>>() {
            return Ok(a.as_array().mapv(|x| x as Float));
        }
        if let Ok(a) = v.extract::<numpy::PyReadonlyArrayDyn<'_, f32>>() {
            return Ok(a.as_array().mapv(|x| x as Float));
        }
        let a = v.py().import("numpy")?.call_method1("asarray", (v, "float64"))?;
        let a = a.extract::<numpy::PyReadonlyArrayDyn<'_, f64>>()?;
        Ok(a.as_array().mapv(|x| x as Float))
    }

    /// A property one a facet: `(facets,)`, or one value for all.
    fn per_facet_property(v: &Bound<'_, PyAny>, facets: usize, name: &str) -> PyResult<Py<PyArray1<Float>>> {
        let a = floats(v)?;
        let a = match (a.ndim(), a.len()) {
            (0, _) | (1, 1) => ndarray::Array1::from_elem(1, a.iter().copied().next().unwrap_or_default()),
            (1, n) if n == facets => a.into_dimensionality::<ndarray::Ix1>().expect("one axis"),
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "{name} is one value a facet, {facets} of them, or one for all: not {:?}",
                    a.shape()
                )));
            }
        };
        Ok(a.into_pyarray(v.py()).unbind())
    }

    /// A property one a layer and a facet: any shape that broadcasts to
    /// `(layers, facets)` as numpy broadcasts, kept at that shape.
    fn per_node_property(v: &Bound<'_, PyAny>, layers: usize, facets: usize, name: &str) -> PyResult<Py<PyArray2<Float>>> {
        let a = floats(v)?;
        let shape = node_shape(a.shape(), layers, facets, name)?;
        let a = a.into_shape_with_order(shape).expect("the same number of values");
        Ok(a.into_pyarray(v.py()).unbind())
    }

    /// Values of `shape` as two axes that broadcast to `(layers, facets)`
    /// as numpy broadcasts -- a number `(1, 1)`, one a facet `(1, facets)`
    /// -- or a message saying why they do not.
    fn node_shape(shape: &[usize], layers: usize, facets: usize, name: &str) -> PyResult<(usize, usize)> {
        let (r, c) = match shape {
            [] => (1, 1),
            [n] => (1, *n),
            [r, c] => (*r, *c),
            _ => (0, 0),
        };
        if (r == layers || r == 1) && (c == facets || c == 1) {
            return Ok((r, c));
        }
        let hint = if shape.len() == 1 && shape[0] == layers {
            " -- one value a layer is (layers, 1): values[:, None]"
        } else {
            ""
        };
        Err(pyo3::exceptions::PyValueError::new_err(format!(
            "{name} of shape {shape:?} does not broadcast to ({layers}, {facets}), layers by facets{hint}"
        )))
    }

    /// Numbers given together, broadcast as numpy broadcasts them, or a
    /// message naming the one that does not fit.
    fn broadcast<'a, const N: usize>(
        given: &'a [(&str, ndarray::ArrayD<Float>); N],
    ) -> PyResult<[ndarray::ArrayViewD<'a, Float>; N]> {
        let ndim = given.iter().map(|(_, a)| a.ndim()).max().unwrap_or(0);
        let mut shape = vec![1; ndim];
        for (name, a) in given {
            for (axis, &n) in shape.iter_mut().rev().zip(a.shape().iter().rev()) {
                if *axis == 1 {
                    *axis = n;
                } else if n != 1 && n != *axis {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "{name} of shape {:?} does not broadcast with the rest, {shape:?}",
                        a.shape()
                    )));
                }
            }
        }
        Ok(given.each_ref().map(|(_, a)| a.broadcast(shape.as_slice()).expect("a shape that broadcasts")))
    }

    /// Back to Python as it came: a number for numbers, an array for arrays.
    fn number_or_array(py: Python<'_>, a: ndarray::ArrayD<Float>) -> PyResult<Py<PyAny>> {
        if a.ndim() == 0 {
            let x = a.into_iter().next().unwrap_or_default();
            return Ok(x.into_pyobject(py)?.into_any().unbind());
        }
        Ok(a.into_pyarray(py).into_any().unbind())
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

    /// A diurnal column of `surface(ti)`: its layer thickness, layer count
    /// and the steps of one rotation at the stable limit.
    fn diurnal(prop: &Properties, period: Float) -> (Float, usize) {
        let ls = super::super::properties::skin_depth_1(prop.diffusivity, period);
        let dz = ls / 8.0;
        let layers = (super::super::properties::skin_depth_2pi(prop.diffusivity, period) / dz).round() as usize + 1;
        (dz, layers)
    }

    /// The closed form against the day it averages: the incidence on ground
    /// at latitude `lat`, the Sun at `dec`, summed hour by hour over a spin
    /// while the Sun is up -- through the polar day and night, near their
    /// edges, and at the equator.
    #[test]
    fn a_latitude_gets_the_mean_of_its_day() {
        let hours = 20_000;
        for lat in [-90.0, -75.0, -65.5, -40.0, -10.0, 0.0, 10.0, 40.0, 64.0, 66.0, 80.0, 90.0_f64] {
            for dec in [-25.0, 0.0, 10.0, 25.0, 60.0_f64] {
                let (phi, delta) = (lat.to_radians(), dec.to_radians());
                let day = (0..hours)
                    .map(|k| {
                        let h = std::f64::consts::PI * (2.0 * (k as f64 + 0.5) / hours as f64 - 1.0);
                        (phi.sin() * delta.sin() + phi.cos() * delta.cos() * h.cos()).max(0.0)
                    })
                    .sum::<f64>()
                    / hours as f64;
                let mean = mean_incidence(phi as Float, delta as Float) as f64;
                assert!((mean - day).abs() < 1e-5, "lat {lat} dec {dec}: {mean} against {day}");
            }
        }
        // The equator at an equinox, and a pole in its day and in its night.
        let (pi, dec) = (crate::util::PI, (25.0 as Float).to_radians());
        assert!((mean_incidence(0.0, 0.0) - 1.0 / pi).abs() < 1e-6);
        assert!((mean_incidence(pi / 2.0, dec) - dec.sin()).abs() < 1e-6);
        assert_eq!(mean_incidence(-pi / 2.0, dec), 0.0);
    }

    /// Over a whole sphere, each latitude weighed by its area, a quarter of
    /// the sunlight whatever the tilt: the obliquity shares it out otherwise
    /// and loses none. 1/4 is the ratio a whole sphere's effective
    /// temperature takes.
    #[test]
    fn a_sphere_gets_a_quarter_whatever_the_tilt() {
        let bands = 20_000;
        let dlat = crate::util::PI / bands as Float;
        for dec in [0.0, 10.0, 25.0, 45.0, 80.0 as Float] {
            let dec = dec.to_radians();
            let sphere = (0..bands)
                .map(|k| {
                    let lat = -crate::util::PI / 2.0 + (k as Float + 0.5) * dlat;
                    (mean_incidence(lat, dec) * lat.cos() * dlat) as f64
                })
                .sum::<f64>()
                / 2.0;
            assert!((sphere - 0.25).abs() < 1e-5, "dec {}: {sphere}", dec.to_degrees());
        }
    }

    /// A column at latitude 60, the Sun 25 degrees north of the equator,
    /// spun until its days repeat, radiates on average what its latitude's
    /// mean sunlight brings: as a surface at the effective temperature
    /// `mean_incidence` gives it, 309.00 K against 308.99, a thousandth of
    /// a kelvin apart -- the start a script gives each column.
    #[test]
    fn a_latitude_radiates_its_mean_sunlight() {
        let prop = surface(200.0);
        let period = 6.0 * 3600.0;
        let (dz, layers) = diurnal(&prop, period);
        let steps = (period / stability_maxdt(prop.diffusivity, dz * dz, 0.5)).ceil() as usize;
        let dt = period / steps as Float;
        let (lat, dec) = ((60.0 as Float).to_radians(), (25.0 as Float).to_radians());
        let start = effective_temperature(1.0, mean_incidence(lat, dec), prop.albedo, prop.emissivity);

        let mut t = columns(layers, 1, start);
        let mut cosi = ndarray::array![0.0];
        let mut emitted = 0.0;
        let spins = 40;
        for it in 0..spins * steps {
            let h = 2.0 * crate::util::PI * (it % steps) as Float / steps as Float;
            cosi[0] = lat.sin() * dec.sin() + lat.cos() * dec.cos() * h.cos();
            solar_bc(t.view_mut(), 1.0, cosi.view(), &prop, dz).unwrap();
            bottom_adiabatic(t.view_mut());
            heat_conduction(t.view_mut(), &prop, dt, dz).unwrap();
            if it >= (spins - 1) * steps {
                emitted += prop.emissivity * crate::util::STEFAN_BOLTZMANN * t[[0, 0]].powi(4) / steps as Float;
            }
        }
        let radiates = (emitted / (prop.emissivity * crate::util::STEFAN_BOLTZMANN)).powf(0.25);
        assert!(
            (radiates - start).abs() < 1e-3 * start,
            "radiates as {radiates} K, its latitude's effective temperature {start} K"
        );
    }

    /// One material in a `Ground` steps as its `Properties` do: the
    /// conservative stencil and the surface per facet reduce to the uniform
    /// ones, to the rounding of a float.
    #[test]
    fn a_ground_of_one_material_steps_as_its_properties() {
        let prop = surface(200.0);
        let (dz, layers) = diurnal(&prop, 6.0 * 3600.0);
        let facets = 4;
        let ground = Ground::new(&prop, layers, facets).unwrap();
        let dt = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let limit = ground.stability_maxdt(dz, 0.5).unwrap();
        assert!((limit - dt).abs() <= 1e-4 * dt, "{limit} against {dt}");

        let mut a = columns(layers, facets, 260.0);
        let mut b = a.clone();
        for step in 0..500 {
            let phase = step as Float * 0.05;
            let cosi = ndarray::Array1::from_iter((0..facets).map(|f| (phase + f as Float).cos()));
            solar_bc(a.view_mut(), 1.0, cosi.view(), &prop, dz).unwrap();
            bottom_adiabatic(a.view_mut());
            heat_conduction(a.view_mut(), &prop, dt, dz).unwrap();
            solar_bc(b.view_mut(), 1.0, cosi.view(), &ground, dz).unwrap();
            bottom_adiabatic(b.view_mut());
            heat_conduction(b.view_mut(), &ground, dt, dz).unwrap();
        }
        let worst = (&a - &b).mapv(Float::abs).fold(0.0 as Float, |m, &x| m.max(x));
        assert!(worst < 1e-2, "{worst} K apart after 500 steps");
    }

    /// Each facet takes its own albedo and emissivity: a column in a
    /// `Ground` is the one its facet's `Properties` would give.
    #[test]
    fn each_facet_takes_its_own_surface() {
        let prop = surface(200.0);
        let (dz, layers) = diurnal(&prop, 6.0 * 3600.0);
        let dt = stability_maxdt(prop.diffusivity, dz * dz, 0.5);
        let mut ground = Ground::new(&prop, layers, 2).unwrap();
        ground.albedo = ndarray::array![0.05, 0.3];
        ground.emissivity = ndarray::array![0.95, 0.95];
        let mut t = columns(layers, 2, 270.0);
        let cosi = ndarray::array![0.9, 0.9];
        let mut alone: Vec<_> = (0..2).map(|_| columns(layers, 1, 270.0)).collect();
        let own: Vec<_> = (0..2)
            .map(|f| Properties { albedo: ground.albedo[f], emissivity: ground.emissivity[f], ..prop.clone() })
            .collect();
        for _ in 0..300 {
            solar_bc(t.view_mut(), 1.0, cosi.view(), &ground, dz).unwrap();
            heat_conduction(t.view_mut(), &ground, dt, dz).unwrap();
            for f in 0..2 {
                solar_bc(alone[f].view_mut(), 1.0, cosi.slice(s![f..f + 1]), &own[f], dz).unwrap();
                heat_conduction(alone[f].view_mut(), &own[f], dt, dz).unwrap();
            }
        }
        for f in 0..2 {
            let d = (&t.column(f) - &alone[f].column(0)).mapv(Float::abs).fold(0.0 as Float, |m, &x| m.max(x));
            assert!(d < 1e-2, "facet {f}: {d} K from its own Properties");
        }
        assert!(t[[0, 0]] > t[[0, 1]] + 10.0, "the darker facet is warmer: {} {}", t[[0, 0]], t[[0, 1]]);
    }

    /// A pseudo-random ground of three materials a layer, different under
    /// every facet.
    fn mixed(layers: usize, facets: usize) -> Ground {
        let prop = surface(200.0);
        let mut g = Ground::new(&prop, layers, facets).unwrap();
        let r = |i: usize, f: usize, k: usize| (((i * 7 + f * 13 + k * 3) % 11) as Float) / 10.0;
        for ((i, f), v) in g.conductivity.indexed_iter_mut() {
            *v *= 0.1 + 2.0 * r(i, f, 1);
        }
        for ((i, f), v) in g.density.indexed_iter_mut() {
            *v *= 0.5 + r(i, f, 2);
        }
        for ((i, f), v) in g.heat_capacity.indexed_iter_mut() {
            *v *= 0.5 + r(i, f, 3);
        }
        g
    }

    /// The conservative form: over a step, the heat the interior of a column
    /// gains is what crosses its top boundary less what crosses its bottom
    /// one -- whatever the materials between, none of it made or lost where
    /// they change.
    #[test]
    fn the_interior_gains_what_crosses_its_ends() {
        let (layers, facets, dz) = (12, 3, 2e-3);
        let g = mixed(layers, facets);
        let dt = 0.9 * g.stability_maxdt(dz, 0.5).unwrap();
        let mut t = columns(layers, facets, 0.0);
        for ((i, f), v) in t.indexed_iter_mut() {
            *v = 200.0 + 60.0 * ((i * 5 + f) as Float * 0.7).sin();
        }
        let before = t.clone();
        heat_conduction(t.view_mut(), &g, dt, dz).unwrap();
        let k = |i: usize, f: usize| harmonic(g.conductivity[[i, f]], g.conductivity[[i + 1, f]]);
        for f in 0..facets {
            let gain: Float = (1..layers - 1)
                .map(|i| g.density[[i, f]] * g.heat_capacity[[i, f]] * (t[[i, f]] - before[[i, f]]) * dz)
                .sum();
            let (n, b) = (layers - 1, &before);
            let across = dt * (k(0, f) * (b[[0, f]] - b[[1, f]]) - k(n - 1, f) * (b[[n - 1, f]] - b[[n, f]])) / dz;
            let scale: Float = (1..layers - 1)
                .map(|i| (g.density[[i, f]] * g.heat_capacity[[i, f]] * (t[[i, f]] - before[[i, f]]) * dz).abs())
                .sum();
            assert!((gain - across).abs() <= 1e-4 * scale, "facet {f}: gained {gain}, {across} crossed");
        }
    }

    /// Two materials in series, the column's ends held: it settles with one
    /// heat flow through both, `dT / sum(dz / k)` -- continuous across the
    /// boundary where a diffusivity per layer would break it.
    #[test]
    fn two_materials_in_series_carry_one_flux() {
        let prop = surface(200.0);
        let (layers, dz) = (21, 1e-3);
        let mut g = Ground::new(&prop, layers, 1).unwrap();
        g.conductivity.slice_mut(s![..10, ..]).fill(prop.conductivity);
        g.conductivity.slice_mut(s![10.., ..]).fill(10.0 * prop.conductivity);
        let dt = g.stability_maxdt(dz, 0.5).unwrap();
        let mut t = columns(layers, 1, 250.0);
        for _ in 0..200_000 {
            t[[0, 0]] = 300.0;
            t[[layers - 1, 0]] = 200.0;
            heat_conduction(t.view_mut(), &g, dt, dz).unwrap();
        }
        let k = |i: usize| harmonic(g.conductivity[[i, 0]], g.conductivity[[i + 1, 0]]);
        let flow: Vec<Float> = (0..layers - 1).map(|i| k(i) * (t[[i, 0]] - t[[i + 1, 0]]) / dz).collect();
        let series = 100.0 / (0..layers - 1).map(|i| dz / k(i)).sum::<Float>();
        for (i, q) in flow.iter().enumerate() {
            assert!((q - series).abs() <= 1e-3 * series, "boundary {i}: {q} W/m2 where the series carries {series}");
        }
        // A layer of the fluffy half drops ten times what one of the other
        // does: ten times the resistance, the same flow.
        let ratio = (t[[0, 0]] - t[[1, 0]]) / (t[[layers - 2, 0]] - t[[layers - 1, 0]]);
        assert!((ratio - 10.0).abs() < 0.01, "{ratio}: {:?}", t.column(0));
    }

    /// A property given once, or a row or a column, steps as the full array
    /// holding the same values does -- to the bit.
    #[test]
    fn a_property_given_once_steps_as_one_given_everywhere() {
        let prop = surface(200.0);
        let (layers, facets, dz) = (10, 3, 2e-3);
        let profile = ndarray::Array2::from_shape_fn((layers, 1), |(i, _)| prop.conductivity * (1.0 + i as Float));
        let across = ndarray::Array2::from_shape_fn((1, facets), |(_, f)| prop.density * (1.0 + 0.1 * f as Float));
        let compact = Ground {
            albedo: ndarray::array![0.2],
            emissivity: ndarray::array![0.9, 0.8, 0.7],
            conductivity: profile.clone(),
            density: across.clone(),
            heat_capacity: ndarray::array![[prop.heat_capacity]],
            ..Ground::new(&prop, layers, facets).unwrap()
        };
        let mut full = Ground::new(&prop, layers, facets).unwrap();
        full.albedo.fill(0.2);
        full.emissivity = ndarray::array![0.9, 0.8, 0.7];
        full.conductivity.assign(&profile);
        full.density.assign(&across);
        let dt = full.stability_maxdt(dz, 0.5).unwrap();
        assert_eq!(compact.stability_maxdt(dz, 0.5).unwrap(), dt);
        let mut a = columns(layers, facets, 280.0);
        let mut b = a.clone();
        let cosi = ndarray::array![1.0, 0.6, 0.2];
        for _ in 0..50 {
            solar_bc(a.view_mut(), 1.0, cosi.view(), &full, dz).unwrap();
            heat_conduction(a.view_mut(), &full, dt, dz).unwrap();
            solar_bc(b.view_mut(), 1.0, cosi.view(), &compact, dz).unwrap();
            heat_conduction(b.view_mut(), &compact, dt, dz).unwrap();
        }
        assert_eq!(a, b);
    }

    /// A column under a fluffy top layer, spun up on the equator: over a
    /// rotation it still radiates what it absorbs.
    #[test]
    fn a_layered_spinning_column_gives_back_what_it_absorbs() {
        let prop = surface(200.0);
        let period = 6.0 * 3600.0;
        let (dz, layers) = diurnal(&prop, period);
        let mut g = Ground::new(&prop, layers, 1).unwrap();
        g.conductivity.slice_mut(s![..6, ..]).mapv_inplace(|k| k / 10.0);
        let steps = (period / g.stability_maxdt(dz, 0.5).unwrap()).ceil() as usize;
        let dt = period / steps as Float;
        let mut t = columns(layers, 1, 280.0);
        let mut cosi = ndarray::array![0.0];
        let (mut absorbed, mut emitted) = (0.0, 0.0);
        let spins = 40;
        for it in 0..spins * steps {
            cosi[0] = (2.0 * crate::util::PI * (it % steps) as Float / steps as Float).cos();
            solar_bc(t.view_mut(), 1.0, cosi.view(), &g, dz).unwrap();
            bottom_adiabatic(t.view_mut());
            heat_conduction(t.view_mut(), &g, dt, dz).unwrap();
            if it >= (spins - 1) * steps {
                absorbed += radiation_sun(1.0, cosi[0], prop.albedo);
                emitted += prop.emissivity * crate::util::STEFAN_BOLTZMANN * t[[0, 0]].powi(4);
            }
        }
        let gap = (emitted - absorbed) / absorbed;
        assert!(gap.abs() < 0.01, "emitted {emitted}, absorbed {absorbed}: {:.3} %", 100.0 * gap);
    }

    /// A graded column -- 8 mm at the top, each layer 20 % thicker, down to
    /// 2 pi yearly skin depths -- follows a spinning surface as a fine column
    /// of equal layers does: the last day's curve within half a kelvin, in
    /// 36 layers where equal ones 8 mm thick would take 2,500 to reach the
    /// same depth. Asked for as the logo's seasons with the body spinning,
    /// the day and the year in one column.
    #[test]
    fn a_graded_column_follows_the_day_as_a_fine_one() {
        let prop = surface(800.0);
        let (day, year) = (8136.0, 769.0 * 86400.0);
        let dz = super::super::properties::skin_depth_1(prop.diffusivity, day) / 4.0;
        let depth = super::super::properties::skin_depth_2pi(prop.diffusivity, year);
        let (graded, widths) = Ground::graded(&prop, 1, dz, depth, 1.2).unwrap();
        assert_eq!(&widths[..3], &[dz; 3], "the top three even, for the surface's gradient");
        assert!(widths.len() < 40 && widths.iter().sum::<Float>() >= depth, "{} layers", widths.len());

        // The day's curve at the surface, the last of 20, `n` points.
        let last_day = |steps: usize, mut step: Box<dyn FnMut(&mut Array2<Float>, &Array1<Float>)>, layers: usize| {
            let mut t = columns(layers, 1, 180.0);
            let mut curve = Vec::new();
            for k in 0..20 * steps {
                let cosi = ndarray::array![(2.0 * crate::util::PI * (k % steps) as Float / steps as Float).cos()];
                step(&mut t, &cosi);
                if k >= 19 * steps {
                    curve.push(t[[0, 0]]);
                }
            }
            curve
        };
        let steps = (day / graded.stability_maxdt(dz, 0.5).unwrap()).ceil() as usize;
        let dt = day / steps as Float;
        let got = last_day(
            steps,
            Box::new(|t, cosi| {
                solar_bc(t.view_mut(), 1.6, cosi.view(), &graded, dz).unwrap();
                bottom_adiabatic(t.view_mut());
                heat_conduction(t.view_mut(), &graded, dt, dz).unwrap();
            }),
            widths.len(),
        );
        let fine_dz = dz / 2.0;
        let fine_steps = (day / stability_maxdt(prop.diffusivity, fine_dz * fine_dz, 0.5)).ceil() as usize;
        let fine_dt = day / fine_steps as Float;
        let fine = last_day(
            fine_steps,
            Box::new(|t, cosi| {
                solar_bc(t.view_mut(), 1.6, cosi.view(), &prop, fine_dz).unwrap();
                bottom_adiabatic(t.view_mut());
                heat_conduction(t.view_mut(), &prop, fine_dt, fine_dz).unwrap();
            }),
            (1.0 / fine_dz).round() as usize + 1,
        );
        let at = |curve: &[Float], x: Float| curve[((x * curve.len() as Float) as usize).min(curve.len() - 1)];
        let worst = (0..100)
            .map(|i| (at(&got, i as Float / 100.0) - at(&fine, i as Float / 100.0)).abs())
            .fold(0.0, Float::max);
        assert!(worst < 0.5, "the day's surface {worst} K from the fine column's");
    }

    /// At ratio 1 a graded column is equal layers, and steps as the uniform
    /// `Ground` of the same properties does.
    #[test]
    fn a_graded_column_at_ratio_one_is_the_uniform_one() {
        let prop = surface(200.0);
        let (dz, layers) = diurnal(&prop, 6.0 * 3600.0);
        let (graded, widths) = Ground::graded(&prop, 2, dz, dz * (layers as Float - 0.5), 1.0).unwrap();
        assert_eq!(widths.len(), layers);
        let uniform = Ground::new(&prop, layers, 2).unwrap();
        let dt = uniform.stability_maxdt(dz, 0.5).unwrap();
        let (mut a, mut b) = (columns(layers, 2, 250.0), columns(layers, 2, 250.0));
        let cosi = ndarray::array![1.0, 0.3];
        for _ in 0..200 {
            for (t, g) in [(&mut a, &graded), (&mut b, &uniform)] {
                solar_bc(t.view_mut(), 1.0, cosi.view(), g, dz).unwrap();
                bottom_adiabatic(t.view_mut());
                heat_conduction(t.view_mut(), g, dt, dz).unwrap();
            }
        }
        let worst = (&a - &b).iter().fold(0.0 as Float, |m, d| m.max(d.abs()));
        assert!(worst < 1e-3, "{worst} K apart");
    }

    /// What cannot be graded is refused: no thickness, layers thinning with
    /// depth, a column shallower than its three even top layers.
    #[test]
    fn a_graded_column_refuses_what_it_cannot_grade() {
        let prop = surface(200.0);
        for (dz, depth, ratio) in [(0.0, 1.0, 1.2), (0.01, 1.0, 0.9), (0.01, 0.02, 1.2), (0.01, Float::INFINITY, 1.2)] {
            assert!(Ground::graded(&prop, 1, dz, depth, ratio).is_err(), "dz {dz} depth {depth} ratio {ratio}");
        }
    }

    #[test]
    fn a_ground_refuses_what_would_blow_up_or_does_not_fit() {
        let prop = surface(200.0);
        let (layers, facets, dz) = (8, 3, 1e-3);
        let mut g = Ground::new(&prop, layers, facets).unwrap();
        let mut t = columns(layers, facets, 250.0);
        let limit = g.stability_maxdt(dz, 0.5).unwrap();
        assert!(heat_conduction(t.view_mut(), &g, limit, dz).is_ok(), "at the limit");
        let err = heat_conduction(t.view_mut(), &g, 1.01 * limit, dz).unwrap_err();
        assert!(err.to_string().contains("stability_maxdt"), "{err}");

        g.conductivity = ndarray::Array2::from_elem((layers + 1, facets), prop.conductivity);
        let err = heat_conduction(t.view_mut(), &g, limit, dz).unwrap_err();
        assert!(err.to_string().contains("does not broadcast"), "{err}");
        g.conductivity = ndarray::Array2::from_elem((1, 1), prop.conductivity);
        g.albedo = ndarray::Array1::zeros(facets + 1);
        let err = solar_bc(t.view_mut(), 1.0, ndarray::Array1::zeros(facets).view(), &g, dz).unwrap_err();
        assert!(err.to_string().contains("albedo"), "{err}");

        let raw = Properties { conductivity: 0.0, ..prop };
        assert!(Ground::new(&raw, layers, facets).unwrap_err().to_string().contains("compute_conductivity_diffusivity"));
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
