//! Bidirectional reflectance: how much sunlight a facet sends to the observer.
//!
//! The thermal side of this engine was complete and the optical side was not.
//! `tpm::emit` turns a temperature into radiance and `tpm::roughness` corrects
//! it for craters, but nothing here answered the other question a
//! ground-based observer asks -- how bright is the *reflected* sunlight -- so
//! a visible-band light curve could not be computed at all. This is that
//! missing half.
//!
//! Every law here returns the **bidirectional reflectance** `r(i, e, alpha)`,
//! defined so that the radiance leaving a facet is
//!
//! ```text
//! I = r(i, e, alpha) * J * mu0
//! ```
//!
//! with `J` the incident solar irradiance at the body, `mu0 = cos i` and
//! `mu = cos e`. That is Hapke's convention, and it is the one that makes the
//! laws below directly comparable: each is a different `r`, and nothing else
//! about the integration changes when you swap one for another.
//!
//! Disc-integrated flux is then the facet sum
//!
//! ```text
//! F = sum_f  r_f * J * mu0_f * mu_f * A_f * lit_f * vis_f / d^2
//! ```
//!
//! where `lit_f` and `vis_f` are the illuminated and visible fractions. Those
//! fractions are the *other* half of the problem and are quantised to quarters
//! today -- see `notes/2026-09-10_polygonal_shadowing_assessment.md`, which
//! measures what that costs. Exact reflectance feeding a quantised area and
//! vice versa are both half-answers; this is one of the two halves.
//!
//! # Which law
//!
//! - [`Lambert`] -- isotropic. Wrong for regoliths at any real phase angle,
//!   but it is the reference every other law is compared against and it costs
//!   nothing to keep.
//! - [`LommelSeeliger`] -- single scattering from a semi-infinite, dark,
//!   particulate surface. Two lines long, and it captures the *limb
//!   darkening* behaviour of a low-albedo asteroid far better than Lambert.
//!   The standard cheap choice in lightcurve inversion.
//! - [`LommelSeeligerLambert`] -- the `c * LS + (1-c) * L` mix used through
//!   the convex-inversion literature, where the Lambert part stands in for
//!   multiple scattering on brighter surfaces.
//! - [`Hapke`] -- the physical model, with single-scattering albedo, a
//!   two-lobe particle phase function, an opposition surge and multiple
//!   scattering through the Chandrasekhar `H` function. What Brož uses.
//!
//! # What is deliberately not here
//!
//! **Hapke's macroscopic roughness `theta_bar` is not implemented**, and this
//! is the one omission worth knowing about. The 1984 roughness correction
//! replaces `mu0` and `mu` with effective values and multiplies by a shadowing
//! function `S(i, e, alpha, theta_bar)`; it is a page of case analysis, easy
//! to get subtly wrong, and untestable against a closed form. Setting
//! `theta_bar = 0` is exact, not approximate, so what is here is a complete
//! Hapke model *for a smooth surface* rather than an approximate one for a
//! rough surface. [`Hapke::theta_bar`] exists and is rejected if non-zero,
//! rather than being silently ignored.
//!
//! Note that kalast already has a roughness treatment for the *thermal* side
//! in `tpm::roughness` (Kuehrt spherical craters). The two are not
//! interchangeable and must not be confused.

use crate::Float;
#[cfg(feature = "python")]
use pyo3::prelude::*;

/// Isotropic scattering: brightness independent of viewing geometry.
///
/// `r = A / pi`. The `mu0` that makes this Lambert's cosine law lives in the
/// `I = r J mu0` convention above, not in `r` itself.
#[cfg_attr(feature = "python", pyfunction)]
pub fn lambert(albedo: Float) -> Float {
    albedo / crate::util::PI
}

/// Single scattering from a dark particulate half-space.
///
/// `r = (w / 4pi) / (mu0 + mu)`, so the emergent radiance carries
/// `mu0 / (mu0 + mu)` -- the limb-darkening signature of a regolith, and
/// visibly different from Lambert's `mu0` away from opposition.
///
/// `w` here is the single-scattering albedo, not the geometric albedo.
#[cfg_attr(feature = "python", pyfunction)]
pub fn lommel_seeliger(w: Float, mu0: Float, mu: Float) -> Float {
    let d = mu0 + mu;
    if d <= 0.0 {
        return 0.0;
    }
    w / (4.0 * crate::util::PI * d)
}

/// The `c * LS + (1 - c) * Lambert` mix of the convex-inversion literature.
///
/// The Lambert term stands in for multiple scattering, which Lommel-Seeliger
/// omits by construction and which matters as the surface brightens. `c = 1`
/// is pure Lommel-Seeliger.
#[cfg_attr(feature = "python", pyfunction)]
pub fn lommel_seeliger_lambert(w: Float, c: Float, mu0: Float, mu: Float) -> Float {
    c * lommel_seeliger(w, mu0, mu) + (1.0 - c) * lambert(w)
}

/// The same mix as a *parameter set*, so a light curve can be handed "a law".
///
/// [`lommel_seeliger_lambert`] is the formula; this is the pair of numbers it
/// is fitted with. Convex inversion quotes exactly these two, and the
/// endpoints are the other two laws exactly -- `c = 1` is pure
/// Lommel-Seeliger, `c = 0` is pure Lambert -- so this one struct covers three
/// of the four laws in this module and [`Hapke`] covers the fourth. That is
/// why [`crate::lightcurve::Law`] has two variants rather than four.
///
/// **It has no phase dependence of its own.** `r` here is a function of `mu0`
/// and `mu` only, so the phase curve it produces comes entirely from the
/// changing geometry: no opposition surge, no phase reddening. That is the
/// known shape of the model rather than an omission -- inversion work
/// multiplies it by a separate empirical phase function -- but it does mean a
/// fit to data spanning a range of `alpha` wants [`Hapke`], or a phase
/// function applied outside this module.
#[derive(Debug, Clone, Copy, PartialEq)]
// `from_py_object` for the same reason as `Hapke` below: pyo3 is changing the
// default for a `#[pyclass]` that derives Clone, and this is the argument of
// a function, so it has to stay extractable.
#[cfg_attr(feature = "python", pyclass(get_all, set_all, from_py_object))]
pub struct LommelSeeligerLambert {
    /// Single-scattering albedo, `0..1`. Not the geometric albedo.
    pub w: Float,
    /// Lommel-Seeliger fraction: `1` is pure LS, `0` pure Lambert.
    pub c: Float,
}

impl Default for LommelSeeligerLambert {
    /// Pure Lommel-Seeliger off a dark regolith.
    ///
    /// `c = 1` rather than a mix, because adding Lambert is a choice a fit
    /// makes; starting from it would put an unrequested multiple-scattering
    /// term in every default answer.
    fn default() -> Self {
        Self { w: 0.1, c: 1.0 }
    }
}

impl LommelSeeligerLambert {
    /// `r(i, e)`, in the `I = r J mu0` convention of this module.
    pub fn reflectance(&self, mu0: Float, mu: Float) -> Float {
        if mu0 <= 0.0 || mu <= 0.0 {
            return 0.0;
        }
        lommel_seeliger_lambert(self.w, self.c, mu0, mu)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl LommelSeeligerLambert {
    #[new]
    #[pyo3(signature = (w=0.1, c=1.0))]
    fn py_new(w: Float, c: Float) -> Self {
        Self { w, c }
    }

    #[pyo3(name = "reflectance")]
    fn py_reflectance(&self, mu0: Float, mu: Float) -> Float {
        self.reflectance(mu0, mu)
    }

    fn __repr__(&self) -> String {
        format!("LommelSeeligerLambert(w={}, c={})", self.w, self.c)
    }
}

/// Chandrasekhar's `H` function, Hapke's 2002 rational approximation.
///
/// `H` carries the multiple scattering, and it is defined implicitly by
///
/// ```text
/// H(x) = 1 + w x H(x) / 2 * integral_0^1 H(u) / (x + u) du
/// ```
///
/// which has no closed form. Hapke's 1993 approximation
/// `H = (1 + 2x) / (1 + 2x sqrt(1-w))` is good to about 4 %; the 2002 form
/// below is good to under 1 %, for the same cost, and `tests/test_scattering.py`
/// checks it against the integral equation directly rather than taking that
/// on trust.
#[cfg_attr(feature = "python", pyfunction)]
pub fn h_function(w: Float, x: Float) -> Float {
    if w <= 0.0 {
        return 1.0;
    }
    let gamma = (1.0 - w).max(0.0).sqrt();
    let r0 = (1.0 - gamma) / (1.0 + gamma);
    if x <= 0.0 {
        // H(0) = 1 exactly: a ray along the surface sees no column to
        // scatter through.
        return 1.0;
    }
    let ln = ((1.0 + x) / x).ln();
    1.0 / (1.0 - w * x * (r0 + (1.0 - 2.0 * r0 * x) / 2.0 * ln))
}

/// Two-lobe Henyey-Greenstein particle phase function, normalised so that its
/// average over the sphere is 1.
///
/// `b` in `[0, 1)` is the lobe width and `c` in `[0, 1]` the backward
/// fraction: `c = 0` is purely forward-scattering, `c = 1` purely backward.
/// `alpha` is the phase angle in radians, so `alpha = 0` is opposition.
///
/// **Mind the sign convention**, which this got wrong first time round.
/// `alpha` is the Sun-target-observer angle, so `alpha = 0` is *back*scatter,
/// while Henyey-Greenstein is normally written in the scattering angle
/// `theta = pi - alpha`, where `theta = 0` is *forward*. The two lobes below
/// are therefore the opposite way round from the textbook expression, and
/// writing them the textbook way leaves the normalisation perfectly intact
/// while pointing the asymmetry backwards -- a phase curve that brightens
/// away from opposition. `tests/test_scattering.py` checks the direction
/// separately from the normalisation for exactly that reason, and that is
/// what caught it.
#[cfg_attr(feature = "python", pyfunction)]
pub fn henyey_greenstein(b: Float, c: Float, alpha: Float) -> Float {
    let ca = alpha.cos();
    let b2 = b * b;
    // Largest at alpha = 0, i.e. toward the Sun: this is the backward lobe.
    let back = (1.0 - b2) / (1.0 - 2.0 * b * ca + b2).powf(1.5);
    // Largest at alpha = pi: the forward lobe.
    let fwd = (1.0 - b2) / (1.0 + 2.0 * b * ca + b2).powf(1.5);
    c * back + (1.0 - c) * fwd
}

/// The shadow-hiding opposition surge.
///
/// `B(alpha) = B0 / (1 + tan(alpha/2) / h)`, a sharp brightening within a few
/// degrees of opposition as particles stop shadowing one another. `B0` is its
/// amplitude at exactly zero phase and `h` its angular width.
#[cfg_attr(feature = "python", pyfunction)]
pub fn opposition_surge(b0: Float, h: Float, alpha: Float) -> Float {
    if b0 <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    // tan(alpha/2) diverges at alpha = pi, where the surge is zero anyway.
    let t = (alpha * 0.5).tan();
    if !t.is_finite() {
        return 0.0;
    }
    b0 / (1.0 + t / h)
}

/// Hapke's bidirectional reflectance, IMSA form, for a smooth surface.
#[derive(Debug, Clone, Copy, PartialEq)]
// `from_py_object` explicitly: pyo3 is changing the default for a
// `#[pyclass]` that derives Clone, and inheriting a default that is about
// to flip is how a binding quietly stops accepting an argument.
#[cfg_attr(feature = "python", pyclass(get_all, set_all, from_py_object))]
pub struct Hapke {
    /// Single-scattering albedo, `0..1`.
    pub w: Float,
    /// Henyey-Greenstein lobe width, `0..1`.
    pub b: Float,
    /// Henyey-Greenstein backward fraction, `0..1`.
    pub c: Float,
    /// Opposition surge amplitude.
    pub b0: Float,
    /// Opposition surge angular width, radians.
    pub h: Float,
    /// Macroscopic roughness, radians. **Must be zero**; see the module docs.
    /// It is carried so that a parameter set from the literature can be
    /// stored without silently losing a term, and rejected on use.
    pub theta_bar: Float,
}

impl Default for Hapke {
    /// A dark, backscattering regolith -- roughly a C-type asteroid.
    ///
    /// Deliberately not all-zeros: an all-zero Hapke is black, and a default
    /// that returns zero everywhere is a default that looks like a bug.
    fn default() -> Self {
        Self {
            w: 0.1,
            b: 0.3,
            c: 0.6,
            b0: 1.0,
            h: 0.05,
            theta_bar: 0.0,
        }
    }
}

impl Hapke {
    /// `r(i, e, alpha)`, with `mu0 = cos i`, `mu = cos e`, `alpha` in radians.
    ///
    /// Returns zero where the facet faces away from either the Sun or the
    /// observer, which keeps a caller summing over all facets honest without
    /// needing its own guard.
    ///
    /// # Errors
    ///
    /// Returns `Err` if `theta_bar` is non-zero: the roughness correction is
    /// not implemented and silently dropping it would change a fitted albedo
    /// without saying so.
    pub fn reflectance(&self, mu0: Float, mu: Float, alpha: Float) -> Result<Float, String> {
        if self.theta_bar != 0.0 {
            return Err(format!(
                "Hapke macroscopic roughness is not implemented (theta_bar = {}). \
                 Set theta_bar = 0 for the smooth case, which is exact, or add \
                 the 1984 correction. See src/scattering.rs.",
                self.theta_bar
            ));
        }
        Ok(self.reflectance_smooth(mu0, mu, alpha))
    }

    /// The smooth-surface reflectance, without the `theta_bar` check.
    ///
    /// Separate so the hot loop of a disc integration does not re-test a
    /// parameter that cannot change inside it.
    pub fn reflectance_smooth(&self, mu0: Float, mu: Float, alpha: Float) -> Float {
        if mu0 <= 0.0 || mu <= 0.0 {
            return 0.0;
        }
        let p = henyey_greenstein(self.b, self.c, alpha);
        let bg = opposition_surge(self.b0, self.h, alpha);
        let h0 = h_function(self.w, mu0);
        let he = h_function(self.w, mu);

        // The `- 1` is not decoration: H(mu0) H(mu) counts the singly
        // scattered light as well as the multiply scattered, and the first
        // term already has it with its proper phase function. Dropping the
        // subtraction double-counts single scattering, which is a several
        // per cent error at low albedo and much more at high.
        //
        // **`1 / (mu0 + mu)`, where Hapke writes `mu0 / (mu0 + mu)`.** Hapke's
        // own `r` is defined by `I = J r`, with the incidence cosine folded
        // in; every law in this module is defined by `I = J r mu0`, with it
        // factored out. Keeping Hapke's form here would leave this one law
        // carrying an extra `mu0` that `lambert` and `lommel_seeliger` do not,
        // so swapping laws would silently change the answer by `cos i` -- the
        // sort of thing that shows up as a wrong pole solution rather than as
        // an error. The convention is stated once at the top of the module and
        // `test_hapke_reduces_to_lommel_seeliger` is what holds all four laws
        // to it.
        self.w / (4.0 * crate::util::PI * (mu0 + mu)) * ((1.0 + bg) * p + h0 * he - 1.0)
    }

    /// Bond albedo of a semi-infinite surface with these particle properties.
    ///
    /// `A_s = r0 (1 + r0/3) ...` -- Hapke's closed form for the spherical
    /// albedo, useful as a sanity check on a fitted `w` and as the quantity a
    /// thermal model actually wants when it asks for "the albedo".
    pub fn bond_albedo(&self) -> Float {
        let gamma = (1.0 - self.w).max(0.0).sqrt();
        let r0 = (1.0 - gamma) / (1.0 + gamma);
        r0 * (1.0 - gamma / 3.0 + gamma * gamma * (1.0 - r0) / 3.0)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl Hapke {
    #[new]
    #[pyo3(signature = (w=0.1, b=0.3, c=0.6, b0=1.0, h=0.05, theta_bar=0.0))]
    fn py_new(w: Float, b: Float, c: Float, b0: Float, h: Float, theta_bar: Float) -> Self {
        Self {
            w,
            b,
            c,
            b0,
            h,
            theta_bar,
        }
    }

    #[pyo3(name = "reflectance")]
    fn py_reflectance(&self, mu0: Float, mu: Float, alpha: Float) -> PyResult<Float> {
        self.reflectance(mu0, mu, alpha)
            .map_err(pyo3::exceptions::PyValueError::new_err)
    }

    #[pyo3(name = "bond_albedo")]
    fn py_bond_albedo(&self) -> Float {
        self.bond_albedo()
    }

    fn __repr__(&self) -> String {
        format!(
            "Hapke(w={}, b={}, c={}, b0={}, h={}, theta_bar={})",
            self.w, self.b, self.c, self.b0, self.h, self.theta_bar
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lambert_is_albedo_over_pi() {
        assert!((lambert(0.1) - 0.1 / crate::util::PI).abs() < 1e-12);
    }

    #[test]
    fn lommel_seeliger_carries_the_limb_darkening_ratio() {
        // The whole point of the law: r * mu0 goes as mu0/(mu0+mu), so at
        // equal incidence and emission it is flat in mu0 where Lambert is not.
        let w = 0.1;
        for (mu0, mu) in [(0.9, 0.9), (0.5, 0.5), (0.2, 0.2)] {
            let i = lommel_seeliger(w, mu0, mu) * mu0;
            assert!((i - w / (8.0 * crate::util::PI)).abs() < 1e-9);
        }
    }

    #[test]
    fn h_function_is_one_when_nothing_scatters() {
        for x in [0.0, 0.1, 0.5, 1.0] {
            assert!((h_function(0.0, x) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn opposition_surge_has_its_stated_amplitude_and_half_width() {
        let (b0, h) = (1.5, 0.05);
        assert!((opposition_surge(b0, h, 0.0) - b0).abs() < 1e-9);

        // The half-width is exact, not approximate: B = B0 / (1 + tan(a/2)/h),
        // so tan(a/2) = h gives B0/2 by construction. A first version of this
        // test guessed "nearly gone by 29 degrees" instead and failed against
        // correct code -- at h = 0.05 the surge is still 16 % there, because
        // the tail is wide even when the core is narrow.
        let a_half = 2.0 * h.atan();
        assert!((opposition_surge(b0, h, a_half) - b0 / 2.0).abs() < 1e-9);

        // Monotone decreasing, and gone at opposition's antipode.
        let mut last = b0 + 1.0;
        for k in 0..12 {
            let a = crate::util::PI * k as Float / 12.0;
            let v = opposition_surge(b0, h, a);
            assert!(v < last, "surge not monotone at alpha = {a}");
            last = v;
        }
    }

    #[test]
    fn roughness_is_refused_rather_than_ignored() {
        let mut h = Hapke::default();
        h.theta_bar = 0.3;
        assert!(h.reflectance(0.5, 0.5, 0.1).is_err());
        h.theta_bar = 0.0;
        assert!(h.reflectance(0.5, 0.5, 0.1).is_ok());
    }

    /// The limit that proves all four laws share one convention.
    ///
    /// With no multiple scattering (`w -> 0`, so `H -> 1`), no opposition
    /// surge and an isotropic particle phase function, Hapke's bracket
    /// collapses to `1` and the whole thing must *be* Lommel-Seeliger. If the
    /// `mu0` convention is inconsistent between the two, this is where it
    /// shows.
    #[test]
    fn hapke_reduces_to_lommel_seeliger() {
        for w in [1e-6, 1e-4, 1e-3] {
            let h = Hapke {
                w,
                b: 0.0,
                c: 0.0,
                b0: 0.0,
                h: 0.0,
                theta_bar: 0.0,
            };
            for (mu0, mu) in [(0.9, 0.7), (0.5, 0.5), (0.3, 0.9), (0.1, 0.2)] {
                let got = h.reflectance_smooth(mu0, mu, 0.4);
                let want = lommel_seeliger(w, mu0, mu);
                // The residual is the multiple scattering, which is O(w) and
                // not zero: `H = 1 + O(w)`, so `H(mu0) H(mu) - 1` is O(w) too.
                // A fixed tolerance would either fail here or pass vacuously
                // at small w; scaling it with w is the actual claim, and it
                // makes the test tighten automatically as w shrinks.
                let tol = 3.0 * w * want;
                assert!(
                    (got - want).abs() <= tol,
                    "w={w} mu0={mu0} mu={mu}: {got} vs {want} (tol {tol})"
                );
            }
        }
    }

    #[test]
    fn a_facet_turned_away_reflects_nothing() {
        let h = Hapke::default();
        assert_eq!(h.reflectance_smooth(-0.1, 0.5, 0.2), 0.0);
        assert_eq!(h.reflectance_smooth(0.5, -0.1, 0.2), 0.0);
    }

    #[test]
    fn bond_albedo_rises_with_single_scattering_albedo() {
        let mut last = -1.0;
        for w in [0.05, 0.1, 0.3, 0.6, 0.9] {
            let h = Hapke {
                w,
                ..Hapke::default()
            };
            let a = h.bond_albedo();
            assert!(a > last, "bond albedo not monotone at w={w}");
            assert!(a < 1.0, "bond albedo must stay under 1, got {a} at w={w}");
            last = a;
        }
    }
}
