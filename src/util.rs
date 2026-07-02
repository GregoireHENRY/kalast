use crate::Float;

pub const EPSILON: Float = crate::fmod::EPSILON;

pub const HOUR: Float = 3600.0;
pub const DAY: Float = 24.0 * HOUR;

pub const PI: Float = crate::consts::PI;
/// degrees per radian
pub const DPR: Float = 180.0 / crate::consts::PI;
/// radians per degree
pub const RPD: Float = 1.0 / DPR;

pub const AU: Float = 1.495978707e11;
pub const AU_KM: Float = 1.495978707e8;

// integrated solar flux at 1 AU (W/m2)
pub const SOLAR_CONSTANT: Float = 1369.0;

pub const STEFAN_BOLTZMANN: Float = 5.670374419e-8;

pub const PLANK_CONSTANT: Float = 6.62607015e-34;
pub const SPEED_LIGHT: Float = 299792458.0;
pub const BOLTZMANN_CONSTANT: Float = 1.380649e-23;
pub const TWO_C: Float = 2.0 * SPEED_LIGHT;
pub const HC: Float = PLANK_CONSTANT * SPEED_LIGHT;
pub const HC2: Float = HC * SPEED_LIGHT;
pub const HC_PER_K: Float = HC / BOLTZMANN_CONSTANT;
pub const TWO_HC2: Float = 2.0 * HC2;
pub const TEMP_SUN: Float = 5778.0;
pub const RADIUS_SUN: Float = 696340e3;
pub const JANSKY: Float = 1e26;

/// Zero-point flux in the Johnson V-band (W/m3) (see Bessell+ 1998)
pub const BAND_V0: Float = 3.631e-2;

pub const MASS_SUN: Float = 1.989e30;
pub const GRAVITATIONAL_CONSTANT: Float = 6.6743e-11;

pub const NEWTON_METHOD_MAX_ITERATION: usize = 1000;
pub const NEWTON_METHOD_THRESHOLD: Float = 0.1;

pub const SPICE_PICTUR_1: &str = "YYYY-MM-DD HR:MN ::RND";
pub const SPICE_PICTUR_2: &str = "YYYY-MM-DD ::RND";
pub const SPICE_PICTUR_3: &str = "YYYYMMDDTHRMNSC ::RND";

// incident spectral solar flux at 1 AU at 545 nm
pub const SFLUX_545: Float = 1896.0;

pub fn bool_to_on_off(b: bool) -> String {
    if b { "ON" } else { "OFF" }.to_string()
}
