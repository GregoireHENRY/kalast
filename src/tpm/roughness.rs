//! Thermal-infrared roughness correction: the Kuehrt spherical-crater model.
//!
//! A facet is dressed with spherical-cap craters and the ratio of the infrared
//! flux it sends to the detector, against that of a flat Lambertian facet at
//! the same temperature, is returned. That ratio is the `R` of section 8.2 of
//! `notes/2026-08-27_conduction_solvers/`, and it is what makes a rough surface
//! read hotter and flatter than a smooth model at low phase -- beaming.
//!
//! Ported from `kalast-utils/roughness-kuehrt/corr_rough_surface.m`, itself a
//! MATLAB port of Kuehrt's FORTRAN. Cite the lineage when it is used:
//!
//! - Giese and Kuehrt (1990), Kuehrt et al. (1992): single scattering in the
//!   crater, visible and infrared.
//! - Lagerros (1996): multiple scattering of thermal radiation.
//! - Mueller (PhD 2007): multiply scattered flux reaching the detector.
//!
//! Released by Ekkehard Kuehrt, 30.11.2015; ported to MATLAB by M. Grott,
//! reorganised by J.-B. Vincent, adapted at ROB.
//!
//! **No heat conduction.** Every sub-facet sits at instantaneous radiative
//! equilibrium, which is what makes a lookup table possible: the correction
//! then depends only on geometry and material, not on thermal history. It is
//! the "step one" of section 8.2 and it corrects what is *emitted* while
//! leaving the subsurface field smooth.
//!
//! Two things carried over from the ROB adaptation rather than the original,
//! both of which matter:
//!
//! - **`flat == 0` returns 1.** At emission angles of 90 degrees or more the
//!   flat facet emits nothing toward the detector and the ratio is 0/0.
//! - **Parameters are arguments, not constants.** The original hardcoded
//!   `lamb = 5 um` and `alb = 0.05`, which is wrong for anything but the body
//!   it was written for -- TIRI's wide band is 10.27 um and Deimos is 0.068.
//!
//! And one bug fixed in passing: the MATLAB returns an undefined `out` when
//! `gamma == 0`, since that branch sets `correction` but never assigns the
//! output. Here it returns 1, which is the physically right answer.

/// Everything the correction depends on besides the three angles.
#[derive(Copy, Clone, Debug)]
pub struct Crater {
    /// Crater opening angle, radians. `pi` is a hemisphere; 0 is flat.
    pub gamma: f64,
    /// Fraction of the facet covered by craters, 0 to 1.
    pub density: f64,
    pub emissivity: f64,
    /// Bond albedo.
    pub albedo: f64,
    /// Heliocentric distance, AU.
    pub rh: f64,
    /// Effective wavelength of the band, metres.
    pub wavelength: f64,
    /// Integration nodes in theta and phi. Convergence is checked in
    /// `examples/analytical/roughness.py`.
    pub ntheta: usize,
    pub nphi: usize,
    /// Solar constant at 1 AU, W/m2.
    pub solar_constant: f64,
}

impl Default for Crater {
    fn default() -> Self {
        Self {
            gamma: std::f64::consts::PI,
            density: 0.25,
            emissivity: 0.95,
            albedo: 0.12,
            rh: 1.0,
            wavelength: 8.0e-6,
            ntheta: 32,
            nphi: 32,
            solar_constant: 1369.0,
        }
    }
}

const SIGMA: f64 = 5.67e-8;
/// `2 h c^2`, for radiance per unit wavelength.
const C1: f64 = 1.191e-16;
/// `h c / k_B`, metre-kelvin.
const C2: f64 = 1.438769e-2;

/// Planck radiance, W / m2 / m / sr. Zero where the exponent would overflow,
/// which is the same cut the MATLAB makes at `C2 / (lambda T) >= 100`.
fn planck(t: f64, lambda: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    let x = C2 / (lambda * t);
    if x >= 100.0 {
        return 0.0;
    }
    C1 * lambda.powi(-5) / (x.exp() - 1.0)
}

/// Normalised dot product, clamped. Returns 1 for a zero-length vector, which
/// is the fictitious value the original uses to keep the integrand finite.
fn cos_between(a: [f64; 3], b: [f64; 3]) -> f64 {
    let na = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let nb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if na * nb == 0.0 {
        return 1.0;
    }
    ((a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) / (na * nb)).clamp(-1.0, 1.0)
}

impl Crater {
    fn solar_flux(&self) -> f64 {
        (1.0 - self.albedo) * self.solar_constant / (self.rh * self.rh)
    }

    /// `C` of Lagerros: the fraction of a sub-facet's sky filled by the rest
    /// of the crater. Depends on the opening angle alone.
    fn c_factor(&self) -> f64 {
        (self.gamma / 4.0).sin().powi(2)
    }

    /// Flat Lambertian facet at instantaneous equilibrium, W / m2 / m / sr.
    pub fn flat_flux(&self, sun: f64, det: f64) -> f64 {
        let cos_sun = sun.cos().max(0.0);
        let cos_det = det.cos().max(0.0);
        let t = (self.solar_flux() * cos_sun / (self.emissivity * SIGMA)).powf(0.25);
        self.emissivity * cos_det * planck(t, self.wavelength)
    }

    /// Equilibrium temperature of one crater node, including the mean-field
    /// self-heating and the visible and infrared multiple scattering.
    fn node_temperature(&self, cos_sun_local: f64, sun: f64) -> f64 {
        let c = self.c_factor();
        let local = self.solar_flux()
            * (cos_sun_local
                + sun.cos() * c * (self.emissivity + self.albedo * (1.0 - c))
                    / (1.0 - self.albedo * c));
        (local / (self.emissivity * SIGMA)).powf(0.25)
    }

    /// The integrand: emission from the node at `(theta, phi)` toward the
    /// detector, including shadowing and blocking by the crater rim.
    fn integrand(&self, theta: f64, phi: f64, sun: f64, det: f64, psi: f64) -> f64 {
        let c = self.c_factor();
        let z_rim = -(self.gamma / 2.0).cos();

        let rpoi = [
            theta.sin() * phi.cos(),
            theta.sin() * phi.sin(),
            -theta.cos(),
        ];
        let rsun = [sun.sin(), 0.0, sun.cos()];
        let rdet = [det.sin() * psi.cos(), det.sin() * psi.sin(), det.cos()];

        let mut cos_sun_local = cos_between(rpoi, [-rsun[0], -rsun[1], -rsun[2]]);
        let mut cos_det_local = cos_between(rpoi, [-rdet[0], -rdet[1], -rdet[2]]);

        // Second intersection of the node-to-detector line with the crater
        // sphere. Past the rim, the rim is in the way.
        let t1 = -2.0 * cos_between(rpoi, rdet);
        if rpoi[2] + t1 * rdet[2] <= z_rim {
            cos_det_local = 0.0;
        }
        // Same for the node-to-Sun line: shadow.
        let t2 = -2.0 * cos_between(rpoi, rsun);
        if rpoi[2] + t2 * rsun[2] <= z_rim {
            cos_sun_local = 0.0;
        }

        let t = self.node_temperature(cos_sun_local, sun);
        let e = self.emissivity;
        // The second term is Mueller's multiply scattered contribution
        // reaching the detector.
        self.emissivity
            * planck(t, self.wavelength)
            * theta.sin()
            * (cos_det_local + det.cos() * (1.0 - e) * c * (1.0 - c) / (1.0 - c * (1.0 - e)))
    }

    /// Flux from the cratered surface toward the detector, W / m2 / m / sr.
    pub fn crater_flux(&self, sun: f64, det: f64, psi: f64) -> f64 {
        let half = self.gamma / 2.0;
        let mut outer = 0.0;
        // Trapezium in phi over an inner trapezium in theta, matching the
        // MATLAB's `trapz` so the two can be compared node for node.
        for i in 0..self.nphi {
            let phi = 2.0 * std::f64::consts::PI * i as f64 / (self.nphi - 1) as f64;
            let mut inner = 0.0;
            for j in 0..self.ntheta {
                let theta = half * j as f64 / (self.ntheta - 1) as f64;
                let w = if j == 0 || j == self.ntheta - 1 { 0.5 } else { 1.0 };
                inner += w * self.integrand(theta, phi, sun, det, psi);
            }
            inner *= half / (self.ntheta - 1) as f64;
            let w = if i == 0 || i == self.nphi - 1 { 0.5 } else { 1.0 };
            outer += w * inner;
        }
        outer *= 2.0 * std::f64::consts::PI / (self.nphi - 1) as f64;
        outer / (std::f64::consts::PI * half.sin().powi(2))
    }

    /// `(correction, crater_flux, flat_flux)`.
    ///
    /// `correction` already carries `density`, so the caller multiplies the
    /// smooth radiance by it directly -- as `observer.m` does with
    /// `fr = R .* f`. Do not also apply `(1 + density (R - 1))`, which is the
    /// same thing expressed for a `R` that excludes the density.
    ///
    /// `sun` is the incidence angle, `det` the emission angle, `psi` the angle
    /// between the projections of the Sun and detector directions onto the
    /// facet. All radians.
    pub fn correction(&self, sun: f64, det: f64, psi: f64) -> (f64, f64, f64) {
        if self.gamma == 0.0 {
            return (1.0, 0.0, self.flat_flux(sun, det));
        }
        let psi = if det == 0.0 && sun == 0.0 { 0.0 } else { psi };
        // Past the horizon nothing is emitted toward the detector and nothing
        // absorbed from the Sun, so the ratio is 0/0 and the answer is "no
        // correction". Tested with a tolerance rather than against zero: at
        // exactly 90 degrees `cos` returns 6e-17, not 0, so the MATLAB's
        // `flat == 0.0` misses and the ratio runs to the cap.
        //
        // **Approaching** grazing the divergence is real, not numerical. The
        // flat reference falls as `cos(e)` while the crater walls still face
        // the detector, so `R` genuinely grows without bound -- 1.10 at 85
        // degrees, 1.13 at 89, past 10 by 89.999. The cap at 10 is a modelling
        // choice inherited from ROB, and it is safe only because the smooth
        // radiance it multiplies is heading to zero at the same time. Facets
        // within a degree or so of the limb should not be trusted to better
        // than that cap.
        const EPS_COS: f64 = 1.0e-12;
        if det.cos() <= EPS_COS || sun.cos() <= EPS_COS {
            return (1.0, 0.0, 0.0);
        }
        let flat = self.flat_flux(sun, det);
        let crater = self.crater_flux(sun, det, psi);
        // Relative, not `flat == 0.0`. Floating point leaves 1.5e-09 where the
        // algebra says zero, which the exact test misses and the ratio then
        // amplifies. Measured against what the same facet would emit face-on,
        // so the threshold means "a vanishing fraction of the most this facet
        // could send", independent of band, distance or units.
        const REL_FLOOR: f64 = 1.0e-9;
        let reference = self.flat_flux(0.0, 0.0);
        if flat <= REL_FLOOR * reference.max(f64::MIN_POSITIVE) {
            return (1.0, crater, flat);
        }
        let r = (self.density * crater + (1.0 - self.density) * flat) / flat;
        // Non-finite or zero means the geometry fell outside what the model
        // covers, and 1 -- no correction -- is the safe value. That guard
        // stays.
        //
        // **The cap at 10 does not.** `observer.m` clamped there on the view
        // that such values were unphysical. They are not: as emission
        // approaches grazing the flat reference falls as `cos(e)` while the
        // crater walls still face the detector, so the *ratio* diverges
        // honestly. What matters is that the product it multiplies stays
        // finite, and it does -- `R * flat` tends to the crater flux, which is
        // bounded. Clamping truncated real signal near the limb and hid the
        // divergence instead of showing it.
        let r = if !r.is_finite() || r == 0.0 { 1.0 } else { r };
        (r, crater, flat)
    }
}
