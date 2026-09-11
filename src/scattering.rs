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
//! # Macroscopic roughness
//!
//! [`Hapke::theta_bar`] is the mean slope angle of roughness the shape model
//! does not resolve, and it **is** implemented -- Hapke (1984), the version in
//! chapter 12 of *Theory of Reflectance and Emittance Spectroscopy*. `mu0` and
//! `mu` are replaced by effective cosines and the result multiplied by a
//! shadowing function `S(i, e, psi, theta_bar)`, with separate expressions for
//! `i <= e` and `i > e`.
//!
//! It was left out at first because it is a page of case analysis with no
//! closed form to test against. Half of that objection was wrong: **Hapke
//! constructed it to preserve Helmholtz reciprocity**, and the two branches
//! exist for exactly that reason -- so `r(i, e) == r(e, i)` is a sharp test of
//! the case analysis, and it is the one that matters, because the way to get a
//! page of cases wrong is to take the wrong branch. `tests/test_scattering.py`
//! checks it across the `i = e` boundary, along with the `theta_bar -> 0`
//! reduction and continuity at that boundary.
//!
//! The correction is **not** a function of the phase angle alone: it needs the
//! azimuth `psi` between the planes of incidence and emergence, which this
//! module recovers from `(mu0, mu, alpha)` through
//! `cos alpha = mu0 mu + sin i sin e cos psi`. That is why roughness does
//! almost nothing at opposition -- where `psi = 0`, `i = e` and `S = 1` --
//! while darkening the surface increasingly toward large phase angles.
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
    /// Macroscopic roughness: the mean slope angle of sub-facet relief, in
    /// radians. `0` is a smooth surface; the literature quotes 20-30 deg for
    /// most asteroids. Must be in `[0, pi/2)`.
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
    /// Returns `Err` if `theta_bar` is outside `[0, pi/2)`. A mean slope of
    /// 90 degrees is not a rough surface, it is a division by zero.
    pub fn reflectance(&self, mu0: Float, mu: Float, alpha: Float) -> Result<Float, String> {
        self.check()?;
        Ok(self.reflectance_unchecked(mu0, mu, alpha))
    }

    /// Whether the parameters can be evaluated at all.
    ///
    /// Separated from the reflectance so the hot loop of a disc integration
    /// can test once, outside, rather than per facet per epoch.
    pub fn check(&self) -> Result<(), String> {
        if !(0.0..crate::consts::FRAC_PI_2).contains(&self.theta_bar) {
            return Err(format!(
                "Hapke theta_bar must be in [0, pi/2) radians, got {}. It is a \
                 mean slope angle -- the literature quotes 20-30 degrees, i.e. \
                 0.35 to 0.52 -- and is in radians here, not degrees.",
                self.theta_bar
            ));
        }
        Ok(())
    }

    /// The reflectance without the parameter check: smooth or rough as
    /// `theta_bar` says.
    pub fn reflectance_unchecked(&self, mu0: Float, mu: Float, alpha: Float) -> Float {
        if self.theta_bar == 0.0 {
            self.reflectance_smooth(mu0, mu, alpha)
        } else {
            self.reflectance_rough(mu0, mu, alpha)
        }
    }

    /// The smooth-surface reflectance: the `theta_bar = 0` formula.
    ///
    /// Exact rather than approximate at that value, and the branch
    /// [`Hapke::reflectance_unchecked`] takes when roughness is off -- which
    /// is most of the time, and is much the cheaper of the two.
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

    /// The rough-surface reflectance: Hapke's 1984 macroscopic roughness.
    ///
    /// `theta_bar` is the mean slope angle of relief the shape model does not
    /// resolve. The correction has two parts, and neither is a fudge factor:
    ///
    /// 1. **Effective cosines.** A tilted facet within the rough surface is
    ///    not illuminated or viewed at the angles the *mean* surface implies,
    ///    so `mu0` and `mu` are replaced by `mu0e` and `mue`, averages over the
    ///    visible and illuminated parts of the slope distribution.
    /// 2. **A shadowing function `S`**, for the parts of the relief that hide
    ///    each other.
    ///
    /// Both need the **azimuth** `psi` between the planes of incidence and
    /// emergence, not just the phase angle, so it is recovered from
    /// `cos alpha = mu0 mu + sin i sin e cos psi`. Two consequences worth
    /// knowing: `S = 1` at `psi = 0`, so roughness does almost nothing at
    /// opposition; and the effect grows toward large phase angles, where it
    /// darkens.
    ///
    /// # The branches, and why reciprocity tests them
    ///
    /// `mu0e`, `mue` and `S` are written differently for `i <= e` and `i > e`.
    /// That asymmetry is not a special case to be tidied away -- it is what
    /// makes the pair **reciprocal**. Swapping `i` and `e` maps one branch onto
    /// the other and carries `(mu0e, mue)` to `(mue, mu0e)`, and the two
    /// shadowing denominators coincide, so `r(i, e) == r(e, i)` exactly. Take
    /// the wrong branch and that identity breaks, which is why
    /// `tests/test_scattering.py` checks it either side of `i = e` rather than
    /// checking the formula against a table.
    ///
    /// Reference: Hapke (1984), Icarus 59, 41; the same as chapter 12 of
    /// *Theory of Reflectance and Emittance Spectroscopy*, eqs. 12.45-12.55.
    pub fn reflectance_rough(&self, mu0: Float, mu: Float, alpha: Float) -> Float {
        if mu0 <= 0.0 || mu <= 0.0 {
            return 0.0;
        }
        if self.theta_bar == 0.0 {
            return self.reflectance_smooth(mu0, mu, alpha);
        }
        let (mu0e, mue, shadow) = self.roughness_terms(mu0, mu, alpha);
        if mu0e <= 0.0 || mue <= 0.0 {
            return 0.0;
        }
        let p = henyey_greenstein(self.b, self.c, alpha);
        let bg = opposition_surge(self.b0, self.h, alpha);
        // Same bracket as the smooth case, on the *effective* cosines -- and
        // divided by the *true* `mu0`, because this module's convention keeps
        // the incidence cosine outside `r` while Hapke's own folds it in.
        self.w / (4.0 * crate::util::PI) * mu0e / (mu0 * (mu0e + mue))
            * ((1.0 + bg) * p + h_function(self.w, mu0e) * h_function(self.w, mue) - 1.0)
            * shadow
    }

    /// The three roughness quantities: `(mu0e, mue, S)`.
    ///
    /// Exposed because they are what another Hapke implementation can be
    /// compared against term by term, and because **`S` is the only thing that
    /// distinguishes the two branches from each other**. Swapping the `i <= e`
    /// and `i > e` cases wholesale leaves the reflectance *reciprocal* -- the
    /// branches are each other's mirror image, so exchanging them preserves
    /// the very symmetry they exist to provide -- and it is caught instead by
    /// `S = 1` at zero azimuth, which holds on the `i <= e` branch only.
    ///
    /// At `theta_bar = 0` this is `(mu0, mu, 1)`.
    pub fn roughness_terms(&self, mu0: Float, mu: Float, alpha: Float) -> (Float, Float, Float) {
        if self.theta_bar == 0.0 || mu0 <= 0.0 || mu <= 0.0 {
            return (mu0, mu, 1.0);
        }
        let pi = crate::util::PI;

        let sin_i = (1.0 - mu0 * mu0).max(0.0).sqrt();
        let sin_e = (1.0 - mu * mu).max(0.0).sqrt();

        // At normal incidence or normal emergence there is no plane to measure
        // an azimuth from, and `cos psi` comes out 0/0. The formulas do not
        // actually need it there -- every term it multiplies carries a `sin i`
        // or a `sin e` that has already gone to zero -- so anything finite
        // serves, and 0 is the value that keeps `cos psi` and `tan(psi/2)`
        // well behaved.
        let psi = if sin_i * sin_e < 1e-9 {
            0.0
        } else {
            ((alpha.cos() - mu0 * mu) / (sin_i * sin_e))
                .clamp(-1.0, 1.0)
                .acos()
        };

        let tan_tb = self.theta_bar.tan();
        let cot_tb = 1.0 / tan_tb;
        // chi(theta_bar): the normalisation of the slope distribution.
        let chi = 1.0 / (1.0 + pi * tan_tb * tan_tb).sqrt();

        // cot(x) from the cosine and sine we already have. Both are finite
        // here: `mu > 0` keeps the angle strictly under pi/2, so `sin` is
        // never zero and `cot` never infinite.
        let e1 = |c: Float, sn: Float| (-(2.0 / pi) * cot_tb * (c / sn)).exp();
        let e2 = |c: Float, sn: Float| {
            let q = cot_tb * (c / sn);
            (-(1.0 / pi) * q * q).exp()
        };
        let eta = |c: Float, sn: Float| chi * (c + sn * tan_tb * e2(c, sn) / (2.0 - e1(c, sn)));

        let (e1i, e2i) = (e1(mu0, sin_i), e2(mu0, sin_i));
        let (e1e, e2e) = (e1(mu, sin_e), e2(mu, sin_e));
        let eta_i = eta(mu0, sin_i);
        let eta_e = eta(mu, sin_e);

        let cos_psi = psi.cos();
        let half = 0.5 * psi;
        let sin2_half = half.sin() * half.sin();
        let f = (-2.0 * half.tan()).exp();

        // The two branches. `ratio` is the term inside S's denominator, and it
        // is the *same quantity* on both sides once i and e are swapped --
        // which is the algebraic reason reciprocity survives the split.
        let (mu0e, mue, ratio) = if mu0 >= mu {
            // i <= e
            let d = 2.0 - e1e - (psi / pi) * e1i;
            (
                chi * (mu0 + sin_i * tan_tb * (cos_psi * e2e + sin2_half * e2i) / d),
                chi * (mu + sin_e * tan_tb * (e2e - sin2_half * e2i) / d),
                mu0 / eta_i,
            )
        } else {
            // i > e
            let d = 2.0 - e1i - (psi / pi) * e1e;
            (
                chi * (mu0 + sin_i * tan_tb * (e2i - sin2_half * e2e) / d),
                chi * (mu + sin_e * tan_tb * (cos_psi * e2i + sin2_half * e2e) / d),
                mu / eta_e,
            )
        };

        let shadow = (mue / eta_e) * (mu0 / eta_i) * chi / (1.0 - f + f * chi * ratio);
        (mu0e, mue, shadow)
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

    /// The roughness terms `(mu0e, mue, S)` at this geometry.
    ///
    /// `(mu0, mu, 1)` when `theta_bar` is zero. `S = 1` at zero azimuth when
    /// `i <= e`, which is the identity that pins which branch is which.
    #[pyo3(name = "roughness_terms")]
    fn py_roughness_terms(&self, mu0: Float, mu: Float, alpha: Float) -> (Float, Float, Float) {
        self.roughness_terms(mu0, mu, alpha)
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
    fn only_an_impossible_roughness_is_refused() {
        let mut h = Hapke::default();
        for good in [0.0, 0.3, 1.5] {
            h.theta_bar = good;
            assert!(h.reflectance(0.5, 0.5, 0.1).is_ok(), "refused {good}");
        }
        for bad in [-0.1, crate::consts::FRAC_PI_2, 1.6] {
            h.theta_bar = bad;
            assert!(h.reflectance(0.5, 0.5, 0.1).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn roughness_reduces_to_the_smooth_case() {
        // Not just at zero -- approaching it. A correction that is right at
        // exactly 0 because of an early return, and wrong just above it, is
        // the shape of bug an equality test at 0 cannot see.
        let h = Hapke::default();
        for (mu0, mu, alpha) in [(0.8, 0.6, 0.4), (0.3, 0.9, 1.1), (0.55, 0.55, 0.2)] {
            let smooth = h.reflectance_smooth(mu0, mu, alpha);
            let mut last = Float::INFINITY;
            for tb in [1e-1, 1e-2, 1e-3, 1e-4] {
                let r = Hapke { theta_bar: tb, ..h }.reflectance_rough(mu0, mu, alpha);
                let d = (r - smooth).abs() / smooth;
                // Non-increasing, and only down to the f32 floor: by 1e-3 the
                // gap is ~6e-8 relative, which is rounding and not the limit,
                // and one geometry reaches *exactly* zero. Demanding strict
                // improvement past that fails on a correct result -- the same
                // trap as testing an exact method against a sampled one.
                assert!(
                    d <= last.max(2e-7),
                    "not converging at theta_bar = {tb}: {d} vs {last}"
                );
                last = d;
            }
            assert!(last < 1e-6, "limit not reached: {last}");
        }
    }

    #[test]
    fn roughness_is_reciprocal_across_the_branch() {
        // The i <= e and i > e branches are what make this hold, so a pair
        // straddling i = e is the case that matters. `r` in this module has
        // `mu0` factored out, so reciprocity is plain symmetry.
        let h = Hapke {
            theta_bar: 0.45,
            ..Default::default()
        };
        for (mu0, mu) in [(0.9, 0.2), (0.2, 0.9), (0.5001, 0.5), (0.5, 0.5001), (0.7, 0.7)] {
            for alpha in [0.0, 0.3, 1.0, 2.0] {
                let a = h.reflectance_rough(mu0, mu, alpha);
                let b = h.reflectance_rough(mu, mu0, alpha);
                if a == 0.0 && b == 0.0 {
                    continue;
                }
                assert!(
                    (a - b).abs() / a.abs().max(b.abs()) < 1e-4,
                    "not reciprocal at mu0={mu0} mu={mu} alpha={alpha}: {a} vs {b}"
                );
            }
        }
    }

    #[test]
    fn the_shadowing_function_is_one_at_zero_azimuth() {
        // psi = 0 means the Sun and the observer share an azimuth, i.e.
        // alpha = |i - e|. There, and only on the `i <= e` branch, Hapke's S
        // is exactly 1 -- the `i > e` branch gives `mue mu0 / (mu0e mu)`
        // instead, which is what reciprocity needs it to be.
        //
        // That asymmetry is what makes this the test an inverted `i <= e`
        // condition cannot survive, and **reciprocity cannot**: the two
        // branches are each other's mirror image, so exchanging them wholesale
        // preserves the very symmetry they exist to provide. Verified by doing
        // exactly that -- inverting the condition leaves every reciprocity
        // check green and fails this one.
        let h = Hapke {
            theta_bar: 0.45,
            ..Default::default()
        };
        // Both angles have to be far enough from normal for `E2` to matter.
        // Near normal incidence `E2(i)` underflows, the correction terms drop
        // out of *both* branches, and S comes out 1 either way -- so a test
        // built only on small `i` sits in a blind spot and catches nothing.
        // The first version of this test did exactly that.
        for (i, e) in [
            (0.8, 1.1),
            (0.9, 1.2),
            (1.0, 1.4),
            (1.1, 1.3),
            (0.7, 0.7),
            (0.3, 0.9),
        ] {
            let (mu0, mu) = (Float::cos(i), Float::cos(e));
            let alpha = (i - e).abs();
            let (_, _, s) = h.roughness_terms(mu0, mu, alpha);
            assert!(
                (s - 1.0).abs() < 1e-4,
                "S = {s} at i={i}, e={e}, psi=0 -- should be exactly 1"
            );
        }
    }

    #[test]
    fn roughness_terms_are_the_identity_when_smooth() {
        let h = Hapke::default();
        let (a, b, s) = h.roughness_terms(0.8, 0.4, 0.5);
        assert_eq!((a, b, s), (0.8, 0.4, 1.0));
    }

    #[test]
    fn roughness_is_continuous_across_i_equals_e() {
        // The branch boundary itself: approaching mu0 = mu from either side
        // must not step. A swapped term in one branch shows up here as a jump
        // even when both sides are individually smooth.
        let h = Hapke {
            theta_bar: 0.45,
            ..Default::default()
        };
        for alpha in [0.2, 0.8, 1.5] {
            let m = 0.6;
            for d in [1e-3, 1e-4, 1e-5] {
                let below = h.reflectance_rough(m - d, m, alpha);
                let above = h.reflectance_rough(m + d, m, alpha);
                assert!(
                    (below - above).abs() / below < 40.0 * d,
                    "jump at i = e, alpha={alpha}, d={d}: {below} vs {above}"
                );
            }
        }
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
