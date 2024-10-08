use crate::config::Config;

use semver::Version;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, env};

pub use nalgebra::{
    self as na, DMatrix, DVector, DVectorView, Dyn, Matrix, Matrix3xX, MatrixView, RowDVector,
    SMatrix, SVector, VecStorage, ViewStorage, U1, U2, U3, U4,
};
pub use nalgebra_glm::{self as glm, vec2, vec3, vec4};

pub type Float = f64;
pub const PI: Float = std::f64::consts::PI;
pub const TAU: Float = std::f64::consts::TAU;

pub type DRVector<T> = RowDVector<T>;
pub type DRVectorView<'a, T> = Matrix<T, U1, Dyn, ViewStorage<'a, Float, U1, Dyn, Dyn, Dyn>>;
pub type DRVectorRef<'a, T> = Matrix<T, U1, Dyn, ViewStorage<'a, Float, U1, Dyn, U1, Dyn>>;

pub type Matrix4xX<T> = Matrix<T, U4, Dyn, VecStorage<T, U4, Dyn>>;

pub type DMatrix2xXView<'a, T> = Matrix<T, U2, Dyn, ViewStorage<'a, Float, U2, Dyn, Dyn, Dyn>>;
pub type DMatrix3xXView<'a, T> = Matrix<T, U3, Dyn, ViewStorage<'a, Float, U3, Dyn, Dyn, Dyn>>;
pub type DMatrixView<'a, T> = Matrix<T, Dyn, Dyn, ViewStorage<'a, Float, Dyn, Dyn, Dyn, Dyn>>;

pub type Vec2 = glm::DVec2;
pub type Vec3 = glm::DVec3;
pub type Vec4 = glm::DVec4;
pub type Vec6 = nalgebra::SVector<Float, 6>;

pub type Mat2 = glm::DMat2;
pub type Mat3 = glm::DMat3;
pub type Mat4 = glm::DMat4;

/// [Astronomical unit](https://en.wikipedia.org/wiki/Astronomical_unit).
pub const ASTRONOMICAL_UNIT: Float = 1.495978707e11;

/// Alias to ASTRONOMICAL_UNIT.
pub const AU: Float = ASTRONOMICAL_UNIT;
pub const AU_KM: Float = AU * 1e-3;

/// [Solar flux constant](https://en.wikipedia.org/wiki/Solar_constant)
pub const SOLAR_CONSTANT: Float = 1361.0;

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

/// Radians per hour.
pub const RPH: Float = PI / 12.0;

/// ...
pub const RADIANS_PER_MINUTE_HMS: Float = RPH / 60.0;

/// ...
pub const RADIANS_PER_MINUTE_DMS: Float = RPD / 60.0;

/// ...
pub const RADIANS_PER_SECOND_HMS: Float = RADIANS_PER_MINUTE_HMS / 60.0;

/// ...
pub const RADIANS_PER_SECOND_DMS: Float = RADIANS_PER_MINUTE_DMS / 60.0;

/// After this number of iterations is reached, consider the numerical method has failed to converge.
pub const NUMBER_ITERATION_FAIL: usize = 1e4 as usize;

/// Threshold that defines the convergence condition of the numerical Newton method.
// pub const NEWTON_METHOD_THRESHOLD: Float = 1e-4;
pub const NEWTON_METHOD_THRESHOLD: Float = 0.1;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const NAME: &str = env!("CARGO_PKG_NAME");

pub const DATETIME: &str = compile_time::datetime_str!();

pub const RUSTC_VERSION: &str = compile_time::rustc_version_str!();

pub fn version() -> Version {
    Version::parse(VERSION).unwrap()
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

pub fn check_if_latest_version(config: &Config) {
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get("https://api.github.com/repos/GregoireHENRY/kalast/tags")
        .header("User-Agent", "request")
        .send()
        .unwrap()
        .json::<Vec<ReqwGitHubTag>>()
        .unwrap();

    let latest = resp
        .iter()
        .map(|t| t.version())
        .filter(|v| v.pre == semver::Prerelease::EMPTY)
        .max()
        .unwrap();

    if latest > version() {
        println!("A more recent version of kalast is available: {}.", latest);

        if let Some(true) = config.preferences.auto_update {
            unimplemented!("Auto-update is not yet implemented. Install newest version manually.");
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReqwGitHubTag {
    commit: HashMap<String, String>,
    name: String,
    node_id: String,
    tarball_url: String,
    zipball_url: String,
}

impl ReqwGitHubTag {
    pub fn version(&self) -> Version {
        Version::parse(&self.name.chars().skip(1).collect::<String>()).unwrap()
    }
}

pub fn numdigits(number: Float) -> usize {
    numdigits_all(number) as usize + 1
}

pub fn numdigits_all(number: Float) -> isize {
    number.log10().floor() as isize
}

pub fn numdigits_comma(number: Float) -> usize {
    let d = numdigits_all(number);
    if number < 1.0 {
        d.abs() as usize
    } else {
        0
    }
}
