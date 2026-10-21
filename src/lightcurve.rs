//! Disc-integrated photometry: a light curve.
//!
//! [`crate::scattering`] answers how bright one facet is; [`crate::shadowing`]
//! answers how much of it the Sun and the observer can actually see. This is
//! the sum that turns those into the single number a photometer measures:
//!
//! ```text
//! F = sum_f  r_f(mu0, mu, alpha) * mu0_f * mu_f * A_f * lit_f * vis_f
//! ```
//!
//! with `mu0 = cos i`, `mu = cos e` and `alpha` the Sun-target-observer angle.
//! Both halves existed before this module and there was no way to get a curve
//! out of them: the disc integral lived only inside
//! `examples/analytical/shadow_quantisation.py` and `mutual_event.py`, hand
//! rolled twice in numpy. It belongs in the engine, so that a Rust program and
//! a `.py` script reach the same one.
//!
//! # Units, and what the number means
//!
//! `F` is in mesh area units, per steradian, per unit incident irradiance.
//! Scaling it to a real flux is a multiplication the caller owns, because it
//! needs facts this module is not given -- what the mesh's length unit is, and
//! the two distances:
//!
//! ```text
//! F_observed = F * (SOLAR_CONSTANT / r_au^2) / delta^2
//! ```
//!
//! A light curve does not need any of that. It is a *relative* measurement,
//! and [`magnitudes`] takes the ratio that removes every constant factor. The
//! scaling matters only for an absolute magnitude, where guessing the mesh's
//! units silently would be much worse than asking.
//!
//! # What is not here
//!
//! - **Light-time and aberration.** `sun` and `observer` are directions the
//!   caller supplies per epoch, so the corrections belong in whatever produced
//!   them -- SPICE, in practice.
//! - **A phase function on top of the mix.**  See
//!   [`crate::scattering::LommelSeeligerLambert`], which has no phase
//!   dependence of its own.

use crate::scattering::{Hapke, LommelSeeligerLambert};
use crate::shadowing;
use crate::{Float, Mat3, Vec3};

#[cfg(feature = "python")]
use pyo3::prelude::*;

/// Which reflectance law the disc integral uses.
///
/// Two variants rather than four: `LommelSeeligerLambert` *is* Lambert at
/// `c = 0` and pure Lommel-Seeliger at `c = 1`, exactly, so three of the four
/// laws in [`crate::scattering`] are one parameter set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Law {
    /// The `c LS + (1-c) Lambert` mix of the convex-inversion literature.
    Mix(LommelSeeligerLambert),
    /// Hapke IMSA, with macroscopic roughness when `theta_bar` is not zero.
    Hapke(Hapke),
}

impl Default for Law {
    fn default() -> Self {
        Law::Mix(LommelSeeligerLambert::default())
    }
}

impl Law {
    /// `r(i, e, alpha)`, in the `I = r J mu0` convention of
    /// [`crate::scattering`].
    ///
    /// This is the hot path -- it runs once per facet per epoch -- so it does
    /// not re-check parameters that cannot change inside the loop. Call
    /// [`Law::check`] once before entering it; [`flux_at`] does.
    pub fn reflectance(&self, mu0: Float, mu: Float, alpha: Float) -> Float {
        match self {
            Law::Mix(m) => m.reflectance(mu0, mu),
            Law::Hapke(h) => h.reflectance_unchecked(mu0, mu, alpha),
        }
    }

    /// What the surface reflects of light coming from the whole sky, its
    /// bihemispherical albedo: `2 int A(mu0) mu0 dmu0`, `A(mu0) = int r mu
    /// dOmega` what it reflects of a beam at `mu0`. A Lambert surface's is its
    /// albedo; this law's, with the facet's colour 1 -- how bright a colour
    /// of 1 is under a diffuse sky (`mesh_shadow.wgsl`'s atmosphere). Gauss-
    /// Legendre over both cosines, the azimuth by the midpoint rule, which
    /// holds a Lambert surface to 1e-6.
    pub fn diffuse_albedo(&self) -> Float {
        // Gauss-Legendre on [0, 1], 12 points.
        const X: [f64; 6] = [
            0.125233408511469, 0.367831498998180, 0.587317954286617,
            0.769902674194305, 0.904117256370475, 0.981560634246719,
        ];
        const W: [f64; 6] = [
            0.249147045813403, 0.233492536538355, 0.203167426723066,
            0.160078328543346, 0.106939325995318, 0.047175336386512,
        ];
        let node = |k: usize| {
            let (x, w) = (X[k % 6], W[k % 6]);
            (0.5 + if k < 6 { 0.5 * x } else { -0.5 * x }, 0.5 * w)
        };
        const AZIMUTHS: usize = 24;
        let mut total = 0.0f64;
        for a in 0..12 {
            let (mu0, wa) = node(a);
            let s0 = (1.0 - mu0 * mu0).sqrt();
            let mut out = 0.0f64;
            for b in 0..12 {
                let (mu, wb) = node(b);
                let s = (1.0 - mu * mu).sqrt();
                for k in 0..AZIMUTHS {
                    let psi = std::f64::consts::PI * (k as f64 + 0.5) / AZIMUTHS as f64;
                    let alpha = (mu0 * mu + s0 * s * psi.cos()).clamp(-1.0, 1.0).acos();
                    let r = self.reflectance(mu0 as Float, mu as Float, alpha as Float) as f64;
                    // Both halves of the azimuth: 2 pi / AZIMUTHS each.
                    out += wb * r * mu * 2.0 * std::f64::consts::PI / AZIMUTHS as f64;
                }
            }
            total += 2.0 * wa * out * mu0;
        }
        total as Float
    }

    /// Whether this law can be evaluated at all.
    ///
    /// Only Hapke can fail, and only on a `theta_bar` outside `[0, pi/2)`.
    /// Checked once here rather than per facet per epoch, which is what
    /// [`Law::reflectance`] is free to skip.
    pub fn check(&self) -> Result<(), String> {
        match self {
            Law::Mix(_) => Ok(()),
            Law::Hapke(h) => h.check(),
        }
    }
}

/// What to compute exactly, and what to assume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// Clip along the Sun vector for each facet's lit area.
    pub shadowing: bool,
    /// Clip along the observer vector for each facet's visible area.
    pub visibility: bool,
}

impl Default for Options {
    /// Both on: the answer that is right for any shape.
    ///
    /// **A convex shape may turn both off** -- no facet occludes another, so
    /// `mu0 > 0` already means fully lit and `mu > 0` fully visible, and the
    /// two clipping passes return 1 everywhere at the cost of the run. Most
    /// shape-inversion models are convex by construction, so this is a real
    /// speed-up and not an approximation; `tests/test_lightcurve.py` pins the
    /// two to agree on a sphere, and to *disagree* on a cratered one, so
    /// neither half of that claim can rot.
    fn default() -> Self {
        Self {
            shadowing: true,
            visibility: true,
        }
    }
}

/// The spin state: where the pole points and where the body has got to.
///
/// The convention is convex inversion's (Kaasalainen & Torppa 2001): the
/// ecliptic-frame pole is `(cos b cos l, cos b sin l, sin b)`, the mesh is
/// body-fixed with its rotation axis along `+z`, and the transformation from
/// one to the other is
///
/// ```text
/// M(t) = Rz(-phase(t)) Ry(pole_lat - pi/2) Rz(-pole_lon)
/// ```
///
/// so `M(t) * pole = +z` for every `t`. Everything here is **radians**, which
/// is the unit the rest of the engine uses, while the literature quotes poles
/// in degrees -- `numpy.radians(...)` at the call site is deliberate rather
/// than an oversight.
///
/// `period` and the epochs share whatever time unit the caller likes; only
/// their ratio is used.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "python", pyclass(get_all, set_all, from_py_object))]
pub struct Spin {
    /// Pole ecliptic longitude, radians.
    pub pole_lon: Float,
    /// Pole ecliptic latitude, radians.
    pub pole_lat: Float,
    /// Sidereal rotation period, in the same unit as the epochs.
    pub period: Float,
    /// Rotation phase at `epoch0`, radians.
    pub phase0: Float,
    /// The epoch `phase0` refers to.
    pub epoch0: Float,
}

impl Default for Spin {
    /// Pole at the ecliptic north, unit period.
    ///
    /// A default spin cannot be physical for anybody's target, so it is the
    /// one that makes the algebra easiest to check rather than a guess at a
    /// typical asteroid.
    fn default() -> Self {
        Self {
            pole_lon: 0.0,
            pole_lat: crate::consts::FRAC_PI_2,
            period: 1.0,
            phase0: 0.0,
            epoch0: 0.0,
        }
    }
}

impl Spin {
    /// Rotation phase at `epoch`, radians, unwrapped.
    pub fn phase_at(&self, epoch: Float) -> Float {
        if self.period == 0.0 {
            return self.phase0;
        }
        self.phase0 + 2.0 * crate::util::PI * (epoch - self.epoch0) / self.period
    }

    /// Ecliptic -> body-fixed, at this rotation phase.
    pub fn matrix(&self, phase: Float) -> Mat3 {
        Mat3::from_rotation_z(-phase)
            * Mat3::from_rotation_y(self.pole_lat - crate::consts::FRAC_PI_2)
            * Mat3::from_rotation_z(-self.pole_lon)
    }

    /// Ecliptic -> body-fixed, at `epoch`.
    pub fn matrix_at(&self, epoch: Float) -> Mat3 {
        self.matrix(self.phase_at(epoch))
    }

    /// The spin axis, as a unit vector in the ecliptic frame.
    pub fn pole(&self) -> Vec3 {
        Vec3::new(
            self.pole_lat.cos() * self.pole_lon.cos(),
            self.pole_lat.cos() * self.pole_lon.sin(),
            self.pole_lat.sin(),
        )
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl Spin {
    /// Defaults to a pole at the ecliptic north and a unit period, which is
    /// the spin that makes the algebra checkable rather than a typical one.
    #[new]
    #[pyo3(signature = (pole_lon=0.0, pole_lat=crate::consts::FRAC_PI_2, period=1.0, phase0=0.0, epoch0=0.0))]
    fn py_new(
        pole_lon: Float,
        pole_lat: Float,
        period: Float,
        phase0: Float,
        epoch0: Float,
    ) -> Self {
        Self {
            pole_lon,
            pole_lat,
            period,
            phase0,
            epoch0,
        }
    }

    /// Rotation phase at `epoch`, radians, unwrapped.
    #[pyo3(name = "phase_at")]
    fn py_phase_at(&self, epoch: Float) -> Float {
        self.phase_at(epoch)
    }

    /// The spin axis as a unit vector in the ecliptic frame.
    #[pyo3(name = "pole")]
    fn py_pole(&self) -> [Float; 3] {
        self.pole().into()
    }

    fn __repr__(&self) -> String {
        format!(
            "Spin(pole_lon={}, pole_lat={}, period={}, phase0={}, epoch0={})",
            self.pole_lon, self.pole_lat, self.period, self.phase0, self.epoch0
        )
    }
}

/// One epoch of a light curve.
#[derive(Debug, Clone, Copy, PartialEq)]
// `from_py_object` explicitly, as everywhere else here: pyo3 is changing
// the default for a `#[pyclass]` that derives Clone.
#[cfg_attr(feature = "python", pyclass(get_all, from_py_object))]
pub struct Point {
    /// The disc integral. See the module docs for its units.
    pub flux: Float,
    /// Solar phase angle, radians.
    pub alpha: Float,
    /// Fraction of the illuminated cross-section that is not shadowed,
    /// area-weighted: `sum A mu0 lit / sum A mu0` over facets facing the Sun.
    ///
    /// A diagnostic, not part of the flux. It is the quantity the shadowing
    /// notes tabulate as "% shadowed" (one minus this), so a real target can
    /// be placed on those tables rather than guessed at. Exactly 1 with
    /// `Options::shadowing` off, which is what makes it worth reporting.
    pub lit_fraction: Float,
    /// True if any facet hit [`shadowing::MAX_PIECES`], so its lit area is an
    /// upper bound rather than the answer.
    pub overflowed: bool,
}

/// The disc integral at one epoch, for geometry the caller has already placed.
///
/// `sun` and `observer` point **from the body toward** the Sun and the
/// observer, in the same frame as `tris`. Neither needs to be normalised.
///
/// This is the general entry point: it makes no assumption that the geometry
/// is one rotating body, so a binary -- two meshes concatenated, re-placed per
/// epoch -- goes through it too. [`curve`] is the single-body case with the
/// rotation done for you.
///
/// # Errors
///
/// If the law cannot be evaluated: a [`Hapke`] with `theta_bar` outside
/// `[0, pi/2)`.
pub fn flux_at(
    tris: &[[Vec3; 3]],
    sun: Vec3,
    observer: Vec3,
    law: &Law,
    opts: Options,
) -> Result<Point, String> {
    law.check()?;

    let sun = sun.normalize();
    let observer = observer.normalize();
    let alpha = sun.dot(observer).clamp(-1.0, 1.0).acos();

    // Each facet's area and its two cosines, zero for a degenerate facet.
    // The winding is the normal: the mesh must be wound outward, which is
    // what `mesh::flatten` guarantees and what `flip_facets` repairs.
    let facets: Vec<(Float, Float, Float)> = tris
        .iter()
        .map(|t| {
            let cross = (t[1] - t[0]).cross(t[2] - t[0]);
            let two_area = cross.length();
            if two_area <= 0.0 {
                return (0.0, 0.0, 0.0);
            }
            let n = cross / two_area;
            (0.5 * two_area, n.dot(sun), n.dot(observer))
        })
        .collect();

    // Only what the sum below reads: the lit fraction of a facet facing the
    // Sun, the visible fraction of one facing both. The rest would be
    // clipped and thrown away, and the night side is the costliest of all.
    let lit = opts.shadowing.then(|| {
        shadowing::lit_fractions_where(tris, sun, |i| facets[i].1 > 0.0)
    });
    let vis = opts.visibility.then(|| {
        shadowing::lit_fractions_where(tris, observer, |i| {
            facets[i].1 > 0.0 && facets[i].2 > 0.0
        })
    });

    // Summed in f64, whatever `Float` is. A fine mesh's facets each add a
    // millionth of the total or less, under f32's step once the sum has grown:
    // on DART's 3,145,728-facet models, Dimorphos's facets after Didymos's were
    // rounded away, the pair 7 mmag faint by the order of its facets alone.
    let mut flux = 0.0f64;
    let mut illuminated = 0.0f64;
    let mut lit_area = 0.0f64;
    let mut overflowed = false;

    for (i, &(area, mu0, mu)) in facets.iter().enumerate() {
        if mu0 <= 0.0 {
            continue;
        }
        let f_lit = match &lit {
            Some(l) => {
                overflowed |= l[i].overflowed;
                l[i].fraction()
            }
            None => 1.0,
        };

        illuminated += (area * mu0) as f64;
        lit_area += (area * mu0 * f_lit) as f64;

        // After the lit bookkeeping, not before: a facet the observer cannot
        // see is still illuminated, and `lit_fraction` would otherwise report
        // the shadowing of the visible hemisphere only.
        if mu <= 0.0 {
            continue;
        }
        let f_vis = match &vis {
            Some(v) => {
                overflowed |= v[i].overflowed;
                v[i].fraction()
            }
            None => 1.0,
        };

        flux += (law.reflectance(mu0, mu, alpha) * mu0 * mu * area * f_lit * f_vis) as f64;
    }

    Ok(Point {
        flux: flux as Float,
        alpha,
        lit_fraction: if illuminated > 0.0 {
            (lit_area / illuminated) as Float
        } else {
            0.0
        },
        overflowed,
    })
}

/// A light curve: one rotating body, over a series of epochs.
///
/// `sun` and `observer` are ecliptic-frame directions from the body, either
/// one each (fixed over the series, which is the right assumption over a
/// single rotation) or one per epoch. The mesh is **not** rotated -- the two
/// directions are carried into the body frame instead, which is two vectors
/// per epoch rather than every vertex, and exactly equivalent since occlusion
/// along a direction does not care which frame it is expressed in.
///
/// # Errors
///
/// If `sun` or `observer` is neither length 1 nor `epochs.len()`, or the law
/// cannot be evaluated.
pub fn curve(
    tris: &[[Vec3; 3]],
    spin: &Spin,
    sun: &[Vec3],
    observer: &[Vec3],
    epochs: &[Float],
    law: &Law,
    opts: Options,
) -> Result<Vec<Point>, String> {
    law.check()?;
    let n = epochs.len();
    for (name, v) in [("sun", sun), ("observer", observer)] {
        if v.len() != 1 && v.len() != n {
            return Err(format!(
                "{name} has {} directions, expected 1 or {n} (one per epoch)",
                v.len()
            ));
        }
    }

    let pick = |v: &[Vec3], i: usize| if v.len() == 1 { v[0] } else { v[i] };

    let mut out = Vec::with_capacity(n);
    for (i, &t) in epochs.iter().enumerate() {
        let m = spin.matrix_at(t);
        out.push(flux_at(
            tris,
            m * pick(sun, i),
            m * pick(observer, i),
            law,
            opts,
        )?);
    }
    Ok(out)
}

/// `-2.5 log10(F / reference)`, in magnitudes.
///
/// Negative is brighter, as astronomy insists. A non-positive flux has no
/// magnitude and comes back as NaN rather than as `-inf`, so it plots as a
/// gap instead of dragging an axis to infinity.
pub fn magnitudes(flux: &[Float], reference: Float) -> Vec<Float> {
    flux.iter()
        .map(|&f| {
            if f > 0.0 && reference > 0.0 {
                -2.5 * (f / reference).log10()
            } else {
                Float::NAN
            }
        })
        .collect()
}

/// The median, which is the reference a normalised light curve uses.
///
/// The median rather than the mean because an eclipse is a deep one-sided
/// excursion: `mutual_event.py` measures a 299 mmag event, which drags a mean
/// well off the out-of-event level that the curve should be normalised to.
pub fn median(xs: &[Float]) -> Float {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        0.5 * (v[n / 2 - 1] + v[n / 2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A UV sphere of unit radius, wound outward.
    fn sphere(n_lat: usize, n_lon: usize) -> Vec<[Vec3; 3]> {
        let p = |i: usize, j: usize| {
            let th = crate::util::PI * i as Float / n_lat as Float;
            let ph = 2.0 * crate::util::PI * j as Float / n_lon as Float;
            Vec3::new(th.sin() * ph.cos(), th.sin() * ph.sin(), th.cos())
        };
        let mut tris = Vec::new();
        for i in 0..n_lat {
            for j in 0..n_lon {
                let (a, b, c, d) = (p(i, j), p(i, j + 1), p(i + 1, j + 1), p(i + 1, j));
                if i > 0 {
                    tris.push([a, b, c]);
                }
                if i + 1 < n_lat {
                    tris.push([a, c, d]);
                }
            }
        }
        tris
    }

    /// A Lambert surface's albedo under a diffuse sky is its albedo. An
    /// isotropic smooth Hapke surface's is near Hapke's closed form for the
    /// spherical albedo, the same integral approximated, where that is good,
    /// at high `w` -- at 0.1 the closed form is 16 % over, where the integral
    /// agrees with single scattering, `w (2/3)(1 - ln 2) H^2` -- and near 1
    /// for one that absorbs nothing.
    #[test]
    fn a_diffuse_sky_sees_the_albedo() {
        let lambert = Law::Mix(LommelSeeligerLambert { w: 0.3, c: 0.0 });
        assert!((lambert.diffuse_albedo() - 0.3).abs() < 1e-5, "{}", lambert.diffuse_albedo());
        let isotropic = |w| crate::scattering::Hapke { w, b: 0.0, c: 0.5, b0: 0.0, h: 0.05, theta_bar: 0.0, k: 1.0 };
        let (got, closed) = (Law::Hapke(isotropic(0.9)).diffuse_albedo(), isotropic(0.9).bond_albedo());
        assert!((got / closed - 1.0).abs() < 0.01, "{got} against {closed}");
        let white = Law::Hapke(isotropic(1.0)).diffuse_albedo();
        assert!((white - 1.0).abs() < 0.02, "{white}");
    }

    #[test]
    fn the_pole_maps_to_z_at_every_phase() {
        for (lon, lat) in [(0.0, 1.2), (2.5, -0.7), (-1.0, 0.0), (4.0, 1.5)] {
            let s = Spin {
                pole_lon: lon,
                pole_lat: lat,
                ..Default::default()
            };
            for phase in [0.0, 0.7, 3.0, 6.0] {
                let z = s.matrix(phase) * s.pole();
                assert!(
                    (z - Vec3::Z).length() < 1e-5,
                    "pole -> {z:?} at lon={lon} lat={lat} phase={phase}"
                );
            }
        }
    }

    #[test]
    fn rotation_is_prograde() {
        // Right-handed about the pole: with the pole at +z, a fixed ecliptic
        // direction sweeps *backwards* in the body frame as phase advances,
        // because the body turns forwards under it. A sign slip here leaves
        // every symmetric test passing and mirrors the light curve in time.
        let s = Spin::default();
        let v = Vec3::X;
        let a = s.matrix(0.0) * v;
        let b = s.matrix(0.3) * v;
        assert!((a - Vec3::X).length() < 1e-6);
        assert!(b.y < 0.0, "expected the body-frame azimuth to decrease, got {b:?}");
        assert!((b.x - (0.3 as Float).cos()).abs() < 1e-6);
    }

    #[test]
    fn phase_advances_one_turn_per_period() {
        let s = Spin {
            period: 5.0,
            phase0: 0.25,
            epoch0: 2.0,
            ..Default::default()
        };
        assert!((s.phase_at(2.0) - 0.25).abs() < 1e-6);
        assert!((s.phase_at(7.0) - (0.25 + 2.0 * crate::util::PI)).abs() < 1e-4);
    }

    #[test]
    fn lambert_sphere_follows_the_analytic_phase_function() {
        // integral of mu0 mu dA over a unit sphere = (2/3)[sin a + (pi-a) cos a],
        // so a Lambert sphere's disc integral is that times A/pi. Convex, so
        // the occlusion passes are off: they would only add their own error to
        // a check about the integral.
        let tris = sphere(120, 240);
        let albedo = 0.1;
        let law = Law::Mix(LommelSeeligerLambert { w: albedo, c: 0.0 });
        let opts = Options {
            shadowing: false,
            visibility: false,
        };
        for deg in [0.0, 30.0, 60.0, 90.0, 120.0] {
            let a: Float = deg * crate::util::RPD;
            let sun = Vec3::new(a.sin(), 0.0, a.cos());
            let p = flux_at(&tris, sun, Vec3::Z, &law, opts).unwrap();
            let want =
                albedo / crate::util::PI * (2.0 / 3.0) * (a.sin() + (crate::util::PI - a) * a.cos());
            assert!(
                (p.flux - want).abs() < 2e-3 * want.max(1e-6),
                "alpha={deg}: {} vs {want}",
                p.flux
            );
        }
    }

    #[test]
    fn a_million_small_facets_after_a_large_one_still_count() {
        // A primary's facets summed first, then a secondary's, each a
        // hundred-millionth of the running sum: under f32's step, so an f32
        // sum rounded every one of them away -- what made a binary on DART's
        // 3,145,728-facet models 7 mmag faint, by the order of its facets.
        let law = Law::Mix(LommelSeeligerLambert { w: 0.1, c: 0.0 });
        let opts = Options {
            shadowing: false,
            visibility: false,
        };
        let big = [Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)];
        let s = (2e-8 as Float).sqrt();
        let small = [Vec3::new(5.0, 0.0, 0.0), Vec3::new(5.0 + s, 0.0, 0.0), Vec3::new(5.0, s, 0.0)];
        let n = 1_000_000;
        let mut tris = vec![big];
        tris.extend(std::iter::repeat_n(small, n));
        let sun = Vec3::new(0.3, 0.0, 1.0);
        let one = |t: [Vec3; 3]| flux_at(&[t], sun, Vec3::Z, &law, opts).unwrap().flux;
        let want = one(big) as f64 + n as f64 * one(small) as f64;
        let got = flux_at(&tris, sun, Vec3::Z, &law, opts).unwrap().flux as f64;
        assert!((got / want - 1.0).abs() < 1e-6, "{got} against {want}");
    }

    #[test]
    fn an_impossible_roughness_is_refused_by_the_integral_too() {
        let tris = sphere(6, 12);
        // Past pi/2 the slope distribution's normalisation divides by zero,
        // and a "mean slope" of 90 degrees is not a surface anyway. A usable
        // 0.3 rad must of course go straight through, which is the half of
        // this that would have kept passing after roughness was implemented.
        let bad = Law::Hapke(Hapke {
            theta_bar: 1.6,
            ..Default::default()
        });
        assert!(flux_at(&tris, Vec3::X, Vec3::X, &bad, Options::default()).is_err());
        let good = Law::Hapke(Hapke {
            theta_bar: 0.3,
            ..Default::default()
        });
        assert!(flux_at(&tris, Vec3::X, Vec3::X, &good, Options::default()).is_ok());
    }
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{AllowTypeChange, PyArray1, PyArrayLike2, PyReadonlyArrayDyn};
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::{PyList, PyTuple};

    use super::{Float, Hapke, Law, LommelSeeligerLambert, Options, Point, Spin, Vec3};
    use crate::shadowing::py::triangles;

    /// A computed light curve: one entry per epoch, as arrays.
    ///
    /// Arrays rather than a list of [`Point`]s because everything downstream
    /// -- a plot, a chi-squared, a fit -- wants columns, and unpacking a list
    /// of objects into them is boilerplate the caller should not write.
    #[pyclass]
    pub struct Curve {
        flux: Vec<Float>,
        alpha: Vec<Float>,
        phase: Vec<Float>,
        epoch: Vec<Float>,
        lit: Vec<Float>,
        overflowed: bool,
    }

    #[pymethods]
    impl Curve {
        /// The disc integral per epoch. Units in the module docs.
        #[getter]
        fn flux<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            PyArray1::from_slice(py, &self.flux)
        }

        /// Solar phase angle per epoch, radians.
        #[getter]
        fn alpha<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            PyArray1::from_slice(py, &self.alpha)
        }

        /// Rotation phase per epoch, radians, unwrapped.
        #[getter]
        fn phase<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            PyArray1::from_slice(py, &self.phase)
        }

        /// The epochs, echoed back so a plot needs one object.
        #[getter]
        fn epoch<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            PyArray1::from_slice(py, &self.epoch)
        }

        /// Unshadowed fraction of the illuminated cross-section, per epoch.
        #[getter]
        fn lit_fraction<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            PyArray1::from_slice(py, &self.lit)
        }

        /// True if any facet at any epoch hit the clipper's piece cap.
        #[getter]
        fn overflowed(&self) -> bool {
            self.overflowed
        }

        /// Magnitudes relative to `reference`, or to the median flux.
        ///
        /// The median is the out-of-event level of a curve with an eclipse in
        /// it, which a mean is not.
        #[pyo3(signature = (reference=None))]
        fn magnitude<'py>(
            &self,
            py: Python<'py>,
            reference: Option<Float>,
        ) -> Bound<'py, PyArray1<Float>> {
            let r = reference.unwrap_or_else(|| super::median(&self.flux));
            PyArray1::from_slice(py, &super::magnitudes(&self.flux, r))
        }

        fn __len__(&self) -> usize {
            self.flux.len()
        }

        fn __repr__(&self) -> String {
            format!(
                "Curve({} epochs, flux {:.4e}..{:.4e}{})",
                self.flux.len(),
                self.flux.iter().cloned().fold(Float::INFINITY, Float::min),
                self.flux
                    .iter()
                    .cloned()
                    .fold(Float::NEG_INFINITY, Float::max),
                if self.overflowed { ", OVERFLOWED" } else { "" }
            )
        }
    }

    /// `Hapke` or `LommelSeeligerLambert`, whichever was passed.
    ///
    /// Written out rather than derived, so the error names both types instead
    /// of reporting a failed extraction of the last one tried.
    pub(crate) fn law_of(obj: &Bound<'_, PyAny>) -> PyResult<Law> {
        if let Ok(h) = obj.extract::<Hapke>() {
            return Ok(Law::Hapke(h));
        }
        if let Ok(m) = obj.extract::<LommelSeeligerLambert>() {
            return Ok(Law::Mix(m));
        }
        Err(PyValueError::new_err(
            "law must be a kalast.scattering.Hapke or a \
             kalast.scattering.LommelSeeligerLambert",
        ))
    }

    /// One direction, or one per epoch.
    fn directions(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<Vec3>> {
        if let Ok(d) = obj.extract::<[Float; 3]>() {
            return Ok(vec![Vec3::from(d)]);
        }
        let v: Vec<[Float; 3]> = obj.extract().map_err(|_| {
            PyValueError::new_err(format!(
                "{name} must be 3 numbers, or a sequence of 3-number directions"
            ))
        })?;
        Ok(v.into_iter().map(Vec3::from).collect())
    }

    /// One body's `vertices` and `indices`, or several bodies' as two lists,
    /// each body's indices its own: all their facets in one list, so that
    /// each body shadows and hides the others.
    ///
    /// A list whose first item is 2-D is a list of bodies; a list of
    /// 3-number rows is one body's vertices, as before.
    fn bodies<'py>(
        vertices: &Bound<'py, PyAny>,
        indices: &Bound<'py, PyAny>,
    ) -> PyResult<Vec<[Vec3; 3]>> {
        let listed = |o: &Bound<'py, PyAny>| o.is_instance_of::<PyList>() || o.is_instance_of::<PyTuple>();
        if listed(vertices) {
            let vs: Vec<Bound<'py, PyAny>> = vertices.extract()?;
            let two_d = |v: &Bound<'py, PyAny>| v.extract::<PyArrayLike2<'py, Float, AllowTypeChange>>().is_ok();
            if vs.first().is_some_and(two_d) {
                let fs: Vec<Bound<'py, PyAny>> = if listed(indices) { indices.extract()? } else { Vec::new() };
                if fs.len() != vs.len() {
                    return Err(PyValueError::new_err(format!(
                        "{} bodies' vertices but {} lists of indices: give each body its own indices",
                        vs.len(),
                        fs.len()
                    )));
                }
                let mut tris = Vec::new();
                for (v, f) in vs.iter().zip(&fs) {
                    tris.extend(triangles(v.extract()?, f.extract()?)?);
                }
                return Ok(tris);
            }
        }
        triangles(vertices.extract()?, indices.extract()?)
    }

    /// The disc-integrated flux at one epoch.
    ///
    /// `vertices` is `(n, 3)` in any float dtype, `indices` is `(m, 3)` or
    /// flat `(3m,)` uint32 -- `kalast.mesh.Mesh` hands back the flat form, and
    /// `mesh.positions * axes` is float64 -- and
    /// `sun` and `observer` point from the body toward each, in the mesh's own
    /// frame. `law` is a `Hapke` or a `LommelSeeligerLambert`.
    ///
    /// Several bodies -- a binary -- are two lists of the same length, each
    /// body's vertices placed in one frame and its own indices:
    /// `flux([v1, v2], [i1, i2], sun, observer, law)`. Their facets are
    /// summed together, so each shadows and hides the others.
    ///
    /// Use this when the geometry is placed already -- a binary, or a body
    /// whose vertices you move yourself. `lightcurve` is the rotating
    /// single-body case.
    ///
    /// Set `shadowing` and `visibility` to False for a convex shape: no facet
    /// occludes another there, so both passes return 1 and cost the run.
    #[pyfunction]
    #[pyo3(name = "flux")]
    #[pyo3(signature = (vertices: "numpy.ndarray | Sequence[numpy.ndarray]", indices: "numpy.ndarray | Sequence[numpy.ndarray]", sun, observer, law, shadowing=true, visibility=true))]
    #[allow(clippy::too_many_arguments)]
    pub fn py_flux<'py>(
        py: Python<'py>,
        vertices: &Bound<'py, PyAny>,
        indices: &Bound<'py, PyAny>,
        sun: &Bound<'py, PyAny>,
        observer: &Bound<'py, PyAny>,
        law: &Bound<'py, PyAny>,
        shadowing: bool,
        visibility: bool,
    ) -> PyResult<Point> {
        let tris = bodies(vertices, indices)?;
        let s = directions(sun, "sun")?;
        let o = directions(observer, "observer")?;
        if s.len() != 1 || o.len() != 1 {
            return Err(PyValueError::new_err(
                "flux takes one sun and one observer direction; use lightcurve for a series",
            ));
        }
        let law = law_of(law)?;
        let opts = Options {
            shadowing,
            visibility,
        };
        // Python's lock released, so a script's threads run epochs at once.
        py.detach(|| super::flux_at(&tris, s[0], o[0], &law, opts))
            .map_err(PyValueError::new_err)
    }

    /// A light curve: one rotating body over a series of epochs.
    ///
    /// `vertices` is `(n, 3)` in any float dtype and `indices` `(m, 3)` or
    /// flat `(3m,)` uint32, in the
    /// body-fixed frame with the rotation axis along `+z`. `sun` and
    /// `observer` are **ecliptic-frame** directions from the body, either one
    /// each or one per epoch. `epochs` shares its unit with `spin.period`.
    ///
    /// The mesh is not rotated -- the two directions are carried into the body
    /// frame instead, which is exactly equivalent and two vectors per epoch
    /// rather than every vertex.
    #[pyfunction]
    #[pyo3(name = "lightcurve")]
    #[pyo3(signature = (vertices, indices, spin, sun, observer, epochs, law, shadowing=true, visibility=true))]
    #[allow(clippy::too_many_arguments)]
    pub fn py_lightcurve<'py>(
        py: Python<'py>,
        vertices: PyArrayLike2<'py, Float, AllowTypeChange>,
        indices: PyReadonlyArrayDyn<'py, u32>,
        spin: Spin,
        sun: &Bound<'py, PyAny>,
        observer: &Bound<'py, PyAny>,
        epochs: Vec<Float>,
        law: &Bound<'py, PyAny>,
        shadowing: bool,
        visibility: bool,
    ) -> PyResult<Curve> {
        let tris = triangles(vertices, indices)?;
        let (sun, observer) = (directions(sun, "sun")?, directions(observer, "observer")?);
        let law = law_of(law)?;
        let opts = Options {
            shadowing,
            visibility,
        };
        let points = py
            .detach(|| super::curve(&tris, &spin, &sun, &observer, &epochs, &law, opts))
            .map_err(PyValueError::new_err)?;

        Ok(Curve {
            flux: points.iter().map(|p| p.flux).collect(),
            alpha: points.iter().map(|p| p.alpha).collect(),
            phase: epochs.iter().map(|&t| spin.phase_at(t)).collect(),
            lit: points.iter().map(|p| p.lit_fraction).collect(),
            overflowed: points.iter().any(|p| p.overflowed),
            epoch: epochs,
        })
    }
}
