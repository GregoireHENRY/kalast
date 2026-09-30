//! Where a body is about the Sun: a Keplerian orbit, for what the Sun does to
//! a body over its year -- its seasons, the distance changing the sunlight
//! and the direction the latitude it falls on.

use crate::{Float, Vec3};
#[cfg(feature = "python")]
use pyo3::prelude::*;

/// A sidereal year (s): Kepler's third law's unit, the period of an orbit
/// 1 AU across.
pub const YEAR: Float = 365.256_36 * crate::util::DAY;

/// An orbit about the Sun from two of its elements: `a`, the semi-major axis
/// (AU), and `e`, the eccentricity -- which is all its shape and period
/// take. Placed in its own plane: x toward perihelion, z along the orbit's
/// angular momentum, so a body goes round counter-clockwise seen from +z.
/// Tilt a body's spin axis in that frame for its obliquity.
#[cfg_attr(feature = "python", pyclass(get_all, set_all, from_py_object))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    /// Semi-major axis (AU).
    pub a: Float,
    /// Eccentricity, `0 <= e < 1`.
    pub e: Float,
}

impl Orbit {
    pub fn new(a: Float, e: Float) -> Self {
        Self { a, e }
    }

    /// Its period (s), by Kepler's third law: `a^1.5` years, the body's own
    /// mass negligible beside the Sun's.
    pub fn period(&self) -> Float {
        self.a.powf(1.5) * YEAR
    }

    /// Where the body is `t` seconds after perihelion, in AU, in the orbit's
    /// plane; any `t`, a later year's the same as the first's.
    pub fn position(&self, t: Float) -> Vec3 {
        let mean = 2.0 * crate::util::PI * (t / self.period()).rem_euclid(1.0);
        let (sin, cos) = eccentric_anomaly(mean, self.e).sin_cos();
        Vec3::new(self.a * (cos - self.e), self.a * (1.0 - self.e * self.e).sqrt() * sin, 0.0)
    }
}

/// Kepler's equation, `M = E - e sin E`, solved for the eccentric anomaly
/// `E` by Newton's method from `M` -- from `pi` past `e = 0.8`, where `M`
/// can take it off the other way -- to the float's precision.
pub fn eccentric_anomaly(mean: Float, e: Float) -> Float {
    let mut big_e = if e < 0.8 { mean } else { crate::util::PI };
    for _ in 0..64 {
        let step = (big_e - e * big_e.sin() - mean) / (1.0 - e * big_e.cos());
        big_e -= step;
        if step.abs() <= 4.0 * Float::EPSILON * big_e.abs().max(1.0) {
            break;
        }
    }
    big_e
}

#[cfg(feature = "python")]
#[pymethods]
impl Orbit {
    /// An orbit about the Sun: `a` its semi-major axis (AU), `e` its
    /// eccentricity. In its own plane, x toward perihelion and z along its
    /// angular momentum: tilt a body's spin axis in that frame for its
    /// obliquity. Didymos's:
    ///
    /// ```python
    /// didymos = kalast.astro.Orbit(a=1.6426, e=0.3832)
    /// year = didymos.period                    # s, 770 days
    /// pos = didymos.position(t)                # AU, t s after perihelion
    /// sim.sun.pos = -pos * kalast.util.AU      # the body at the origin
    /// ```
    #[new]
    #[pyo3(signature = (a, e))]
    fn py_new(a: Float, e: Float) -> PyResult<Self> {
        if !(a > 0.0 && (0.0..1.0).contains(&e)) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "an orbit about the Sun wants a > 0 and 0 <= e < 1: not a = {a}, e = {e}"
            )));
        }
        Ok(Self::new(a, e))
    }

    /// Its period (s), by Kepler's third law: `a^1.5` years.
    #[getter(period)]
    fn py_period(&self) -> Float {
        self.period()
    }

    /// Where the body is `t` seconds after perihelion, in AU, in the orbit's
    /// plane -- the Sun at the origin -- as `(x, y, 0)`.
    #[pyo3(name = "position")]
    fn py_position<'py>(&self, py: Python<'py>, t: Float) -> Bound<'py, numpy::PyArray1<Float>> {
        numpy::PyArray1::from_slice(py, &self.position(t).to_array())
    }

    fn __repr__(&self) -> String {
        format!("Orbit(a={}, e={})", self.a, self.e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kepler's equation holds for what `eccentric_anomaly` gives, round the
    /// orbit and up to eccentricities near a parabola.
    #[test]
    fn keplers_equation_is_solved() {
        for e in [0.0, 0.1, 0.3832, 0.7, 0.95] {
            for k in 0..64 {
                let mean = 2.0 * crate::util::PI * k as Float / 64.0;
                let big_e = eccentric_anomaly(mean, e);
                assert!((big_e - e * big_e.sin() - mean).abs() < 1e-5, "e {e}, M {mean}: E {big_e}");
            }
        }
    }

    /// At perihelion `a (1 - e)` along +x, at aphelion `a (1 + e)` along -x
    /// half a period on; a year 1 AU across; Didymos's 770 days.
    #[test]
    fn an_orbit_goes_from_perihelion_to_aphelion_in_half_its_year() {
        let didymos = Orbit::new(1.6426, 0.3832);
        let year = didymos.period();
        let p = didymos.position(0.0);
        assert!((p - Vec3::new(1.6426 * (1.0 - 0.3832), 0.0, 0.0)).length() < 1e-5, "{p}");
        let q = didymos.position(0.5 * year);
        assert!((q - Vec3::new(-1.6426 * 1.3832, 0.0, 0.0)).length() < 1e-4, "{q}");
        assert!(didymos.position(0.1 * year).y > 0.0, "counter-clockwise seen from +z");
        assert!((didymos.position(3.25 * year) - didymos.position(0.25 * year)).length() < 1e-4);
        assert!((Orbit::new(1.0, 0.0).period() / YEAR - 1.0).abs() < 1e-6);
        assert!((year / crate::util::DAY - 769.6).abs() < 1.0, "{} days", year / crate::util::DAY);
    }

    /// Equal areas in equal times, Kepler's second law: the triangles the
    /// radius sweeps in equal steps of time, fast near perihelion and slow
    /// near aphelion, have one area.
    #[test]
    fn equal_areas_are_swept_in_equal_times() {
        let orbit = Orbit::new(1.6426, 0.3832);
        let steps = 400;
        let dt = orbit.period() / steps as Float;
        let areas: Vec<Float> = (0..steps)
            .map(|k| {
                let (p, q) = (orbit.position(k as Float * dt), orbit.position((k + 1) as Float * dt));
                0.5 * p.cross(q).z
            })
            .collect();
        let (low, high) = areas.iter().fold((Float::MAX, Float::MIN), |(l, h), &x| (l.min(x), h.max(x)));
        assert!(low > 0.0 && (high - low) / high < 1e-3, "swept from {low} to {high}");
    }
}
