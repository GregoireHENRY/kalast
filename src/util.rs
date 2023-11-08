use crate::prelude::*;
use crate::python::*;

use std::{env, path::PathBuf};

/// [Astronomical unit](https://en.wikipedia.org/wiki/Astronomical_unit).
pub const ASTRONOMICAL_UNIT: Float = 1.495978707e11;

/// Alias to ASTRONOMICAL_UNIT.
pub const AU: Float = ASTRONOMICAL_UNIT;

/// [Solar flux constant](https://en.wikipedia.org/wiki/Solar_constant)
pub const SOLAR_CONSTANT: Float = 1363.0;

/// [Stefan-Boltzmann constant](https://en.wikipedia.org/wiki/Stefan-Boltzmann_constant)
pub const STEFAN_BOLTZMANN: Float = 5.670374419e-8;

/// ...
pub const SPEED_LIGHT: Float = 299792458.0;

/// ...
pub const GRAVITATIONAL_CONSTANT: Float = 6.6743e-11;

/// ...
pub const MASS_SUN: Float = 1.989e30;

/// ...
pub const MU_SUN: Float = GRAVITATIONAL_CONSTANT * MASS_SUN;

/// ... J.s
pub const PLANK_CONSTANT: Float = 6.62607015e-34;

/// ... J/K
pub const BOLTZMANN_CONSTANT: Float = 1.380649e-23;

/// Duration of a second in seconds.
pub const SECOND: u64 = 1;

/// Duration of a minute in seconds.
pub const MINUTE: u64 = 60;

/// Duration of an hour in seconds.
pub const HOUR: u64 = MINUTE * 60;

/// Duration of an Earth day in seconds.
pub const DAY: u64 = HOUR * 24;

/// Year alias for 365.25 days.
pub const YEAR: u64 = (DAY as f64 * 365.25) as _;

/// SPICE time representation for general purpose.
pub const SPICE_DATE_FORMAT: &str = "YYYY-MON-DD HR:MN:SC ::RND";

/// SPICE time representation for file name.
pub const SPICE_DATE_FORMAT_FILE: &str = "YYYY-MM-DDTHR:MN:SC ::RND";

/// Degrees per radian.
pub const DPR: Float = 180. / PI;

/// Radians per degree.
pub const RPD: Float = 1. / DPR;

/// After this number of iterations is reached, consider the numerical method has failed to converge.
pub const NUMBER_ITERATION_FAIL: usize = 1e4 as usize;

/// Threshold that defines the convergence condition of the numerical Newton method.
pub const NEWTON_METHOD_THRESHOLD: Float = 1e-4;

#[allow(unused)]
pub(crate) fn package_name() -> String {
    env::var("CARGO_PKG_NAME").unwrap()
}

/// Find the location of the `target/` directory. Note that this may be
/// overridden by `cmake`, so we also need to check the `CARGO_TARGET_DIR`
/// variable.
#[allow(unused)]
pub(crate) fn target_dir() -> PathBuf {
    if let Ok(target) = env::var("CARGO_TARGET_DIR") {
        PathBuf::from(target)
    } else {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("target")
    }
}

pub fn cartesian_to_spherical(cartesian: &Vec3) -> Vec3 {
    let r = glm::magnitude(&cartesian);
    let theta = cartesian.y.atan2(cartesian.x);
    let phi = cartesian.z.atan2(glm::magnitude(&cartesian.xy()));

    vec3(r, theta, phi)
}

pub fn spherical_to_cartesian(spherical: &Vec3) -> Vec3 {
    let x = spherical.x * spherical.z.cos() * spherical.y.cos();
    let y = spherical.x * spherical.z.cos() * spherical.y.sin();
    let z = spherical.x * spherical.z.sin();

    vec3(x, y, z)
}

pub fn vec3_to_4_one(v: &Vec3) -> Vec4 {
    vec4(v.x, v.y, v.z, 1.0)
}

pub fn fmt_str_tab(text: &str, tab: usize) -> String {
    let mut lines = text.lines();
    let tabs = "  ".repeat(tab);

    let mut vec = vec![];
    vec.push(format!("{}", lines.next().unwrap()));

    for line in lines {
        vec.push(format!("{}{}", tabs, line));
    }

    vec.join("\n")
}

#[allow(non_snake_case, unused)]
pub(crate) mod python {
    use super::*;

    #[pyfunction]
    pub(crate) const fn ASTRONOMICAL_UNIT() -> Float {
        super::ASTRONOMICAL_UNIT
    }

    #[pyfunction]
    pub(crate) const fn AU() -> Float {
        super::AU
    }

    #[pyfunction]
    pub(crate) const fn SECOND() -> u64 {
        super::SECOND
    }

    #[pyfunction]
    pub(crate) const fn MINUTE() -> u64 {
        super::MINUTE
    }

    #[pyfunction]
    pub(crate) const fn HOUR() -> u64 {
        super::HOUR
    }

    #[pyfunction]
    pub(crate) const fn DAY() -> u64 {
        super::DAY
    }

    #[pyfunction]
    pub(crate) const fn YEAR() -> u64 {
        super::YEAR
    }

    #[pyfunction]
    pub(crate) const fn SPICE_DATE_FORMAT<'a>() -> &'a str {
        super::SPICE_DATE_FORMAT
    }

    #[pyfunction]
    pub(crate) const fn SPICE_DATE_FORMAT_FILE<'a>() -> &'a str {
        super::SPICE_DATE_FORMAT_FILE
    }

    #[pyfunction]
    pub(crate) const fn DPR() -> Float {
        super::DPR
    }

    #[pyfunction]
    pub(crate) const fn RPD() -> Float {
        super::RPD
    }
}
