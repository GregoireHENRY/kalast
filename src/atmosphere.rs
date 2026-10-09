//! A planet's dusty atmosphere over its surface, as a camera sees it.
//!
//! What reaches the camera from a point of a planet with air over it is the
//! light the dust scatters straight back, once and many times, and the
//! surface seen through the dust, lit by the beam that got through and by
//! the sky. Mars's dust makes its limb bright and its terminator soft, lights
//! the floor of a shadow, and veils the surface's contrast: at AFC's 655 nm
//! during the Hera swing-by (vertical optical depth about 0.45), the
//! surface alone was half of what AFC read near the limb.
//!
//! The model, for one point of the surface, the atmosphere a plane-parallel
//! slab along the local vertical but its paths through it taken over a
//! sphere:
//!
//! - **Airmass**, Chapman's for an exponential atmosphere over a sphere,
//!   `m(mu) = 2 / (mu + sqrt(mu^2 + 8 H / (pi R)))`: `1 / mu` overhead and
//!   `sqrt(pi R / 2 H)`, about 22 on Mars, at the horizon where a slab's
//!   runs to infinity.
//! - **The dust**: single-scattering albedo `omega` and a phase function of
//!   two forward Henyey-Greenstein lobes, `q HG(g1) + (1 - q) HG(g2)`; its
//!   asymmetry `g` is their weighted mean. Delta-Eddington scaling takes the
//!   forward peak, `f = g^2`, out of the optical depth for the multiple
//!   scattering.
//! - **Single scattering**, exact for the slab: `omega P(alpha) / 4
//!   mu0 / (mu0 + mu) (1 - exp(-tau' (m0 + m)))`, with the full phase
//!   function and the scaled depth (the truncated-multiple-scattering form).
//! - **Multiple scattering**, Eddington's two-stream solution for the beam
//!   (Meador and Weaver 1980), its source function integrated along the line
//!   of sight, times `1.05 + 0.33 ln(1 + 1/tau')`: the two-stream source
//!   short of a Monte Carlo of the same slab by that much.
//! - **The surface**, Lambert with the facet's albedo, lit by the direct beam
//!   that got through (`exp(-tau' m0)`, and only where the shadow map lets
//!   it) and by the sky (the two-stream diffuse transmission), seen directly
//!   (`exp(-tau' m)`) and through the dust, with the light the surface and
//!   the atmosphere send each other (the atmosphere's spherical albedo).
//!   The sky lights a facet as it would a level one; the beam by the facet's
//!   own incidence.
//! - **Twilight**, past the local horizon, `exp(-0.24 d - 0.0235 d^2)` of
//!   what the point would have with the Sun on the horizon, `d` the Sun's
//!   depression in degrees: Mars's at an 11 km scale height.
//!
//! Against the formulas as the research note wrote them
//! (`notes/2026-10-07_mars_atmosphere/`): equal on a level surface. The
//! renderer evaluates the same in `mesh_shadow.wgsl`; the two are held to
//! each other by `tests/test_atmosphere_render.py`.

#[cfg(feature = "python")]
use pyo3::prelude::*;

use crate::Float;

/// A dusty atmosphere over a body, per `body.atmosphere`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "python", pyclass(get_all, set_all, from_py_object))]
pub struct Atmosphere {
    /// Vertical optical depth at the surface.
    pub tau: Float,
    /// Scale height of the dust, in the scene's units.
    pub scale_height: Float,
    /// The planet's radius, in the scene's units: with the scale height, how
    /// long a slanted path through the air is, and with `polar_radius` the
    /// level the optical depth `tau` is given at -- the equatorial radius of
    /// that ellipsoid.
    pub radius: Float,
    /// The ellipsoid's polar radius, about the body's own z axis; `None`, a
    /// sphere of `radius`. Above the ellipsoid the air is thinner, below it
    /// thicker: the dust's optical depth goes as the surface pressure,
    /// `tau exp(-height / scale_height)` -- on Mars twice as much over
    /// Hellas as over the uplands round it. A sphere would put the poles
    /// 20 km under it.
    pub polar_radius: Option<Float>,
    /// The dust's single-scattering albedo, `0..1`.
    pub omega: Float,
    /// The phase function's first forward lobe's asymmetry, `0..1`.
    pub g1: Float,
    /// The second's.
    pub g2: Float,
    /// The first lobe's weight, `0..1`.
    pub q: Float,
    /// The surface's mean albedo round about, for the light the surface and
    /// the atmosphere send each other. `None`: each facet's own.
    pub albedo: Option<Float>,
}

impl Default for Atmosphere {
    /// Mars in a clear season, at 655 nm, in km: optical depth 0.45, the
    /// dust's 11 km scale height and properties from the rovers' sky
    /// surveys, over the IAU's ellipsoid of Mars, for a scene in kilometres.
    fn default() -> Self {
        Self {
            tau: 0.45,
            scale_height: 11.0,
            radius: 3396.19,
            polar_radius: Some(3376.20),
            omega: 0.975,
            g1: 0.889,
            g2: 0.094,
            q: 0.743,
            albedo: None,
        }
    }
}

/// The two-stream solution at one direction, `beam` of `Atmosphere::parts`.
struct Beam {
    ap: f64,
    bp: f64,
    a: f64,
    b: f64,
    /// The direct transmission, `exp(-tau' m)`.
    direct: f64,
    /// The diffuse transmission, over the cosine.
    diffuse: f64,
}

/// One point's I/F in pieces: what the dust sends, and how the surface is
/// lit and seen through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parts {
    /// Single and multiple scattering by the dust, I/F.
    pub dust: Float,
    /// The direct beam's transmission down, and the view's direct
    /// transmission up.
    pub down: Float,
    pub up: Float,
    /// The sky's irradiance on a level surface, over the Sun's at normal
    /// incidence above the air.
    pub sky: Float,
    /// The view's diffuse transmission up.
    pub up_diffuse: Float,
    /// The atmosphere's spherical albedo.
    pub spherical: Float,
    /// What twilight leaves, 1 with the Sun above the horizon.
    pub twilight: Float,
}

impl Atmosphere {
    /// Whether the numbers describe an atmosphere at all.
    pub fn check(&self) -> Result<(), String> {
        let unit = |x: Float| (0.0..=1.0).contains(&x);
        if !(self.tau >= 0.0 && self.tau.is_finite()) {
            return Err(format!("tau is a non-negative optical depth, got {}", self.tau));
        }
        if !(self.scale_height > 0.0 && self.radius > 0.0 && self.polar_radius.is_none_or(|c| c > 0.0)) {
            return Err(format!(
                "scale_height and radius are lengths above zero, got {} and {}",
                self.scale_height, self.radius
            ));
        }
        if !unit(self.omega) || !unit(self.q) {
            return Err(format!("omega and q are in [0, 1], got {} and {}", self.omega, self.q));
        }
        if !(self.g1.abs() < 1.0 && self.g2.abs() < 1.0) {
            return Err(format!("g1 and g2 are in (-1, 1), got {} and {}", self.g1, self.g2));
        }
        if let Some(a) = self.albedo.filter(|a| !unit(*a)) {
            return Err(format!("albedo is in [0, 1], got {a}"));
        }
        Ok(())
    }

    /// Chapman's airmass at `mu`, over a sphere.
    pub fn airmass(&self, mu: Float) -> Float {
        self.airmass64(mu as f64) as Float
    }

    // In f64 whatever `Float` is: this is the reference the shader is held to.
    fn airmass64(&self, mu: f64) -> f64 {
        let mu = mu.max(0.0);
        let x = 8.0 * self.scale_height as f64 / (std::f64::consts::PI * self.radius as f64);
        2.0 / (mu + (mu * mu + x).sqrt())
    }

    /// The phase function at phase angle `alpha`, a mean of 1 over the sphere.
    pub fn phase(&self, alpha: Float) -> Float {
        self.phase64(alpha as f64) as Float
    }

    fn phase64(&self, alpha: f64) -> f64 {
        let c = alpha.cos();
        let hg = |g: f64| (1.0 - g * g) / (1.0 + g * g + 2.0 * g * c).powf(1.5);
        let q = self.q as f64;
        q * hg(self.g1 as f64) + (1.0 - q) * hg(self.g2 as f64)
    }

    /// The ellipsoid's radius at a point of latitude `lat` (geocentric).
    pub fn level(&self, lat: Float) -> Float {
        let (a, c) = (self.radius as f64, self.polar_radius.unwrap_or(self.radius) as f64);
        let (s, k) = ((lat as f64).sin(), (lat as f64).cos());
        (a * c / ((c * k).powi(2) + (a * s).powi(2)).sqrt()) as Float
    }

    /// The pieces of one point's I/F, `height` above the ellipsoid. `mu0`
    /// and `mu` are the cosines of the Sun's and the camera's angles from the
    /// local vertical -- of the planet, not of the facet -- and `alpha` the
    /// phase angle.
    pub fn parts(&self, mu0: Float, mu: Float, alpha: Float, height: Float) -> Parts {
        let (mu0, mu, alpha) = (mu0 as f64, mu as f64, alpha as f64);
        let tau = self.tau as f64 * (-(height as f64) / self.scale_height as f64).exp();
        // Past the horizon: twilight, of the Sun on it.
        let depression = (-mu0).clamp(0.0, 1.0).asin().to_degrees();
        let twilight = (-0.24 * depression - 0.0235 * depression * depression).exp();
        let (n0, n) = (self.airmass64(mu0), self.airmass64(mu));
        let (u0, u) = (1.0 / n0, 1.0 / n);
        let (q, g1, g2) = (self.q as f64, self.g1 as f64, self.g2 as f64);
        let g = q * g1 + (1.0 - q) * g2;
        let (om, f) = (self.omega as f64, g * g);
        let tp = ((1.0 - om * f) * tau).max(1e-9);
        let wp = (1.0 - f) * om / (1.0 - om * f);
        let gp = g / (1.0 + g);

        let single = om / (1.0 - om * f) * self.phase64(alpha) / 4.0 * u0 / (u0 + u) * (1.0 - (-tp * (n0 + n)).exp());

        // Eddington's two-stream coefficients.
        let c1 = (7.0 - wp * (4.0 + 3.0 * gp)) / 4.0;
        let c2 = -(1.0 - wp * (4.0 - 3.0 * gp)) / 4.0;
        let k = (c1 * c1 - c2 * c2).max(0.0).sqrt();
        let r = c2 / (c1 + k);
        let e = (-k * tp).exp();
        let beam = |uu: f64, nn: f64| {
            let c3 = (2.0 - 3.0 * gp * uu) / 4.0;
            let c4 = 1.0 - c3;
            let d = k * k - nn * nn;
            let ap = wp * (c3 * (c1 - nn) + c2 * c4) / d;
            let bp = wp * (c4 * (c1 + nn) + c2 * c3) / d;
            let direct = (-tp * nn).exp();
            let a = (bp * r * e - ap * direct) / (1.0 - r * r * e * e);
            let b = -bp - a * r * e;
            let diffuse = (a * r + b * e + bp * direct) / uu;
            Beam { ap, bp, a, b, direct, diffuse }
        };
        let sun = beam(u0, n0);
        let lp = (1.0 - (-tp * (n - k)).exp()) / (1.0 - k * u);
        let lm = (1.0 - (-tp * (n + k)).exp()) / (1.0 + k * u);
        let l0 = u0 / (u0 + u) * (1.0 - (-tp * (n + n0)).exp());
        let sig = (1.0 + r) * (sun.a * e * lp + sun.b * lm) + (sun.ap + sun.bp) * l0;
        let dif = (1.0 - r) * (sun.a * e * lp - sun.b * lm) + (sun.ap - sun.bp) * l0;
        let calibration = 1.05 + 0.33 * (1.0 + 1.0 / tp).ln();
        let multiple = calibration * wp * (0.5 * sig + 0.75 * gp * u * dif);

        let view = beam(u, n);
        Parts {
            dust: (single + multiple) as Float,
            down: sun.direct as Float,
            up: view.direct as Float,
            // The flux into the top of the air is the geometric cosine's; the
            // airmass is the path's length through it.
            sky: (mu0.max(0.0) * sun.diffuse) as Float,
            up_diffuse: view.diffuse as Float,
            spherical: (r * (1.0 - e * e) / (1.0 - r * r * e * e)) as Float,
            twilight: twilight as Float,
        }
    }

    /// The I/F at a point of a Lambert surface of albedo `albedo` under this
    /// atmosphere, `height` above its ellipsoid. `mu0_facet` is the Sun's
    /// cosine from the facet's normal, `mu0` and `mu` the Sun's and the
    /// camera's from the local vertical, `lit` how much of the direct beam
    /// reaches the facet (a shadow's 0).
    #[allow(clippy::too_many_arguments)]
    pub fn iof(&self, mu0_facet: Float, mu0: Float, mu: Float, alpha: Float, albedo: Float, lit: Float, height: Float) -> Float {
        self.iof_with(albedo * mu0_facet.max(0.0), mu0_facet, mu0, mu, alpha, albedo, lit, height)
    }

    /// As [`iof`](Self::iof), with the surface's own I/F under the beam,
    /// `direct`, from a law other than Lambert; the sky's light is taken as
    /// Lambert's.
    #[allow(clippy::too_many_arguments)]
    pub fn iof_with(
        &self,
        direct: Float,
        mu0_facet: Float,
        mu0: Float,
        mu: Float,
        alpha: Float,
        albedo: Float,
        lit: Float,
        height: Float,
    ) -> Float {
        let p = self.parts(mu0, mu, alpha, height);
        let around = self.albedo.unwrap_or(albedo);
        let sun_up = if mu0 > 0.0 { lit } else { 0.0 };
        let beam = mu0_facet.max(0.0) * sun_up * p.down;
        let surface = (direct * sun_up * p.down * p.up + albedo * p.sky * p.up + (beam + p.sky) * around * p.up_diffuse)
            / (1.0 - around * p.spherical);
        p.twilight * (p.dust + surface)
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl Atmosphere {
    #[new]
    #[pyo3(signature = (tau=0.45, scale_height=11.0, radius=3396.19, polar_radius=Some(3376.20), omega=0.975, g1=0.889, g2=0.094, q=0.743, albedo=None))]
    #[allow(clippy::too_many_arguments)]
    fn py_new(
        tau: Float,
        scale_height: Float,
        radius: Float,
        polar_radius: Option<Float>,
        omega: Float,
        g1: Float,
        g2: Float,
        q: Float,
        albedo: Option<Float>,
    ) -> Self {
        Self { tau, scale_height, radius, polar_radius, omega, g1, g2, q, albedo }
    }

    /// The I/F at a point of a Lambert surface of albedo `albedo` under this
    /// atmosphere: `mu0_facet` the Sun's cosine from the facet's normal,
    /// `mu0` and `mu` the Sun's and the camera's from the local vertical,
    /// `alpha` the phase angle, `lit` how much of the direct beam reaches the
    /// facet, `height` above the ellipsoid.
    #[pyo3(name = "iof", signature = (mu0_facet, mu0, mu, alpha, albedo, lit=1.0, height=0.0))]
    #[allow(clippy::too_many_arguments)]
    fn py_iof(&self, mu0_facet: Float, mu0: Float, mu: Float, alpha: Float, albedo: Float, lit: Float, height: Float) -> PyResult<Float> {
        self.check().map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(self.iof(mu0_facet, mu0, mu, alpha, albedo, lit, height))
    }

    /// The ellipsoid's radius at geocentric latitude `lat`, radians.
    #[pyo3(name = "level")]
    fn py_level(&self, lat: Float) -> Float {
        self.level(lat)
    }

    /// Chapman's airmass at `mu`, over a sphere.
    #[pyo3(name = "airmass")]
    fn py_airmass(&self, mu: Float) -> Float {
        self.airmass(mu)
    }

    fn __repr__(&self) -> String {
        format!(
            "Atmosphere(tau={}, scale_height={}, radius={}, polar_radius={}, omega={}, g1={}, g2={}, q={}, albedo={})",
            self.tau,
            self.scale_height,
            self.radius,
            self.polar_radius.map_or("None".to_string(), |c| c.to_string()),
            self.omega,
            self.g1,
            self.g2,
            self.q,
            self.albedo.map_or("None".to_string(), |a| a.to_string())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formulas as the research note wrote them, for a level surface:
    /// numbers from its code (numpy, f64), Mars at tau 0.45, the asymmetry
    /// the two lobes' weighted mean.
    #[test]
    fn a_level_surface_gives_the_notes_numbers() {
        let a = Atmosphere { radius: 3390.0, polar_radius: None, ..Atmosphere::default() };
        // (mu0, mu, alpha deg, albedo) -> I/F, from the note's code.
        let cases = [
            (0.8, 0.9, 20.0, 0.16, 0.145_268_808),
            (0.5, 0.95, 35.0, 0.20, 0.112_955_366),
            (0.2, 0.6, 60.0, 0.16, 0.059_095_457),
            (0.05, 0.3, 80.0, 0.10, 0.041_864_674),
            (0.9, 0.1, 70.0, 0.16, 0.231_221_158),
        ];
        for (mu0, mu, alpha, albedo, want) in cases {
            let got = a.iof(mu0, mu0, mu, Float::to_radians(alpha), albedo, 1.0, 0.0);
            assert!((got / want - 1.0).abs() < 1e-5, "mu0 {mu0} mu {mu}: {got} for {want}");
        }
    }

    /// No dust is the bare surface: Lambert, and black in shadow.
    #[test]
    fn no_dust_is_lambert() {
        let a = Atmosphere { tau: 0.0, ..Atmosphere::default() };
        let got = a.iof(0.6, 0.7, 0.8, 0.5, 0.2, 1.0, 0.0);
        assert!((got - 0.2 * 0.6).abs() < 1e-6, "{got}");
        assert!(a.iof(0.6, 0.7, 0.8, 0.5, 0.2, 0.0, 0.0).abs() < 1e-6);
    }

    /// The dust lights a shadow, brightens the limb and fades past the
    /// terminator.
    #[test]
    fn the_dust_lights_shadows_limbs_and_twilight() {
        let a = Atmosphere::default();
        let lit = a.iof(0.7, 0.7, 0.9, 0.4, 0.16, 1.0, 0.0);
        let shadow = a.iof(0.7, 0.7, 0.9, 0.4, 0.16, 0.0, 0.0);
        assert!(shadow > 0.1 * lit && shadow < 0.6 * lit, "shadow {shadow} of {lit}");
        let (centre, limb) = (a.iof(0.5, 0.5, 0.95, 0.5, 0.16, 1.0, 0.0), a.iof(0.5, 0.5, 0.05, 1.2, 0.16, 1.0, 0.0));
        assert!(limb > centre, "limb {limb} under the centre {centre}");
        let dusk = |mu0: Float| a.iof(0.0, mu0, 0.8, 1.5, 0.16, 1.0, 0.0);
        assert!(dusk(0.0) > dusk(-0.05) && dusk(-0.05) > dusk(-0.15) && dusk(-0.3) < 1e-3 * dusk(0.0));
    }

    /// The air thins with height as the pressure does, and the ellipsoid is
    /// the level: Mars's poles 20 km under its equator.
    #[test]
    fn the_air_thins_with_height_above_the_ellipsoid() {
        let a = Atmosphere::default();
        assert!((a.level(0.0) - 3396.19).abs() < 1e-2 && (a.level(crate::util::PI / 2.0) - 3376.20).abs() < 1e-2);
        let at = |h: Float| a.parts(0.8, 0.9, 0.3, h).dust;
        let (low, level, high) = (at(-7.0), at(0.0), at(11.0));
        assert!(low > level && level > high, "{low} {level} {high}");
        let thin = Atmosphere { tau: a.tau * (-1.0 as Float).exp(), ..a };
        assert!((high / thin.parts(0.8, 0.9, 0.3, 0.0).dust - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_airmass_runs_from_overhead_to_the_horizon() {
        let a = Atmosphere::default();
        // Chapman's overhead is 1 / (1 + 2 H / (pi R)): 0.998 on Mars.
        assert!((a.airmass(1.0) - 1.0).abs() < 3e-3);
        let horizon = (crate::util::PI * a.radius / (2.0 * a.scale_height)).sqrt();
        assert!((a.airmass(0.0) / horizon - 1.0).abs() < 1e-6, "{} for {horizon}", a.airmass(0.0));
    }

    #[test]
    fn nonsense_is_refused() {
        for bad in [
            Atmosphere { tau: -1.0, ..Default::default() },
            Atmosphere { omega: 1.2, ..Default::default() },
            Atmosphere { g1: 1.0, ..Default::default() },
            Atmosphere { scale_height: 0.0, ..Default::default() },
            Atmosphere { albedo: Some(2.0), ..Default::default() },
        ] {
            assert!(bad.check().is_err(), "{bad:?}");
        }
        assert!(Atmosphere::default().check().is_ok());
    }
}
