use crate::{util::*, NEWTON_METHOD_THRESHOLD, NUMBER_ITERATION_FAIL};

use regex::Regex;
use snafu::{prelude::*, Location};
use std::str::FromStr;
use uom::{
    si::{angle::radian, f64::Angle},
    str::ParseQuantityError,
};

pub type OrbitResult<T, E = AstronomicalAngleConversionError> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
pub enum AstronomicalAngleConversionError {
    AngleParsing {
        source: ParseQuantityError,
        location: Location,
    },

    RegexParsing {
        source: regex::Error,
        location: Location,
    },

    NotDMS {
        location: Location,
    },

    NotHMS {
        location: Location,
    },

    #[snafu(display("Cannot be parsed, given {given} but expect the format 00h00m00.0s."))]
    CannotBeParsed {
        given: String,
    },
}

#[derive(Debug, Clone)]
pub enum DMS {
    D,
    M,
    S,
}

#[derive(Debug, Clone)]
pub enum HMS {
    H,
    M,
    S,
}

#[derive(Debug, Clone)]
pub enum AstronomicalAngleOptions {
    Value,
    HMS,
    DMS,
}

#[derive(Debug, Clone, Default)]
pub struct AstronomicalAngle {
    pub angle: Angle,
}

impl AstronomicalAngle {
    pub fn new(angle: Angle) -> Self {
        Self { angle }
    }

    pub fn parse(s: &str) -> OrbitResult<Self> {
        match Self::from_value(s) {
            Ok(angle) => return Ok(angle),
            Err(_) => {}
        };

        match Self::from_hms(s) {
            Ok(angle) => return Ok(angle),
            Err(_) => {}
        };

        match Self::from_dms(s) {
            Ok(angle) => return Ok(angle),
            Err(_) => {}
        };

        Err(AstronomicalAngleConversionError::CannotBeParsed {
            given: s.to_string(),
        })
    }

    pub fn from_value(s: &str) -> OrbitResult<Self> {
        s.parse::<Angle>()
            .context(AngleParsingSnafu {})
            .and_then(|angle| Ok(Self::new(angle)))
    }

    pub fn from_hms(s: &str) -> OrbitResult<Self> {
        // To find the int hours: \d{1,2}h
        // To find the int minutes: \d{1,2}m
        // To find the float seconds: [+-]?(\d+([.]\d*)?([eE][+-]?\d+)?|[.]\d+([eE][+-]?\d+)?)s
        let re = Regex::new(r"^(?<h>\d{1,2})h(?<m>\d{1,2})m(?<s>[+-]?(\d+([.]\d*)?([eE][+-]?\d+)?|[.]\d+([eE][+-]?\d+)?))s$").context(RegexParsingSnafu)?;

        let caps = re.captures_iter(s).next().context(CannotBeParsedSnafu {
            given: s.to_string(),
        })?;

        let h = caps
            .name("h")
            .context(NotHMSSnafu {})?
            .as_str()
            .parse::<u8>()
            .unwrap();
        let m = caps
            .name("m")
            .context(NotHMSSnafu {})?
            .as_str()
            .parse::<u8>()
            .unwrap();
        let s = caps
            .name("s")
            .context(NotHMSSnafu {})?
            .as_str()
            .parse::<f64>()
            .unwrap();

        let radians =
            h as f64 * RPH + m as f64 * RADIANS_PER_MINUTE_HMS + s * RADIANS_PER_SECOND_HMS;

        Ok(Self::new(Angle::new::<radian>(radians)))
    }

    pub fn from_dms(s: &str) -> OrbitResult<Self> {
        // To find the int degrees: [+-]?\d{1,2}°
        // To find the int minutes: \d{1,2}'
        // To find the float seconds: [+-]?(\d+([.]\d*)?([eE][+-]?\d+)?|[.]\d+([eE][+-]?\d+)?)"
        let re = Regex::new(r#"^(?<d>[+-]?\d{1,2})°(?<m>\d{1,2})'(?<s>[+-]?(\d+([.]\d*)?([eE][+-]?\d+)?|[.]\d+([eE][+-]?\d+)?))"$"#).context(RegexParsingSnafu)?;

        let caps = re.captures_iter(s).next().context(CannotBeParsedSnafu {
            given: s.to_string(),
        })?;

        let d = caps
            .name("d")
            .context(NotDMSSnafu {})?
            .as_str()
            .parse::<i8>()
            .unwrap();
        let m = caps
            .name("m")
            .context(NotDMSSnafu {})?
            .as_str()
            .parse::<u8>()
            .unwrap();
        let s = caps
            .name("s")
            .context(NotDMSSnafu {})?
            .as_str()
            .parse::<f64>()
            .unwrap();

        let radians =
            d as f64 * RPD + m as f64 * RADIANS_PER_MINUTE_HMS + s * RADIANS_PER_SECOND_HMS;

        Ok(Self::new(Angle::new::<radian>(radians)))
    }
}

impl FromStr for AstronomicalAngle {
    type Err = AstronomicalAngleConversionError;

    fn from_str(s: &str) -> OrbitResult<Self> {
        Self::parse(s)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Equatorial {
    pub ra: AstronomicalAngle,
    pub dec: AstronomicalAngle,
}

impl Equatorial {
    pub fn new(ra: AstronomicalAngle, dec: AstronomicalAngle) -> Self {
        Self { ra, dec }
    }
}

pub fn mean_angular_motion(gravitational_parameter: Float, semi_major_axis: Float) -> Float {
    (gravitational_parameter / semi_major_axis.powi(3)).sqrt()
}

pub fn mean_anomaly(mean_angular_motion: Float, time: Float, time_at_periapsis: Float) -> Float {
    mean_angular_motion * (time - time_at_periapsis)
}

pub fn mean_anomaly_from_eccentric(eccentric_anomaly: Float, eccentricity: Float) -> Float {
    eccentric_anomaly - eccentricity * eccentric_anomaly.sin()
}

pub fn eccentric_anomaly(mean_anomaly: Float, eccentricity: Float) -> Float {
    let mut index = 0;
    let mut eccentric_anomaly_guess: Float = 0.0;

    'convergence: loop {
        // Get new value.
        let value_function =
            eccentric_anomaly_guess - eccentricity * eccentric_anomaly_guess.sin() - mean_anomaly;
        let value_derivative = 1.0 - eccentricity * eccentric_anomaly_guess.cos();
        let eccentric_anomaly = eccentric_anomaly_guess - value_function / value_derivative;

        // Break conditions.
        if index > NUMBER_ITERATION_FAIL
            || (eccentric_anomaly - eccentric_anomaly_guess).abs() < NEWTON_METHOD_THRESHOLD
        {
            eccentric_anomaly_guess = eccentric_anomaly;
            break 'convergence;
        }

        // Prepare for next iteration if break condition didn't pass.
        eccentric_anomaly_guess = eccentric_anomaly;
        index += 1;
    }

    eccentric_anomaly_guess
}

pub fn true_anomaly_from_eccentric_anomaly(eccentricity: Float, eccentric_anomaly: Float) -> Float {
    2.0 * (((1.0 + eccentricity) / (1.0 - eccentricity)).sqrt() * (eccentric_anomaly / 2.0).tan())
        .atan()
}

pub fn true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    time: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
) -> Float {
    let mean_angular_motion = mean_angular_motion(gravitational_parameter, semi_major_axis);
    let mean_anomaly = mean_anomaly(mean_angular_motion, time, time_at_periapsis);
    let eccentric_anomaly = eccentric_anomaly(mean_anomaly, eccentricity);

    true_anomaly_from_eccentric_anomaly(eccentricity, eccentric_anomaly)
}

pub fn radius(semi_major_axis: Float, eccentricity: Float, eccentric_anomaly: Float) -> Float {
    semi_major_axis * (1.0 - eccentricity * eccentric_anomaly.cos())
}

pub fn radius_from_true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    true_anomaly: Float,
) -> Float {
    semi_major_axis * (1.0 - eccentricity.powi(2)) / (1.0 + eccentricity * true_anomaly.cos())
}

pub fn parameter_ellipse(semi_major_axis: Float, eccentricity: Float) -> Float {
    semi_major_axis * (1.0 - eccentricity.powi(2))
}

pub fn angular_momentum(
    gravitational_parameter: Float,
    semi_major_axis: Float,
    eccentricity: Float,
) -> Float {
    (gravitational_parameter * semi_major_axis * (1.0 - eccentricity.powi(2))).sqrt()
}

pub fn position(
    radius: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    true_anomaly: Float,
    inclination: Float,
) -> Vec3 {
    vec3(
        radius
            * (longitude_ascending_node.cos() * (argument_periapsis + true_anomaly).cos()
                - longitude_ascending_node.sin()
                    * (argument_periapsis + true_anomaly).sin()
                    * inclination.cos()),
        radius
            * (longitude_ascending_node.sin() * (argument_periapsis + true_anomaly).cos()
                + longitude_ascending_node.cos()
                    * (argument_periapsis + true_anomaly).sin()
                    * inclination.cos()),
        radius * (inclination.sin() * (argument_periapsis * true_anomaly).sin()),
    )
}

pub fn velocity(
    position: &Vec3,
    angular_momentum: Float,
    eccentricity: Float,
    radius: Float,
    parameter_ellipse: Float,
    true_anomaly: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    inclination: Float,
) -> Vec3 {
    vec3(
        (position.x * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            - angular_momentum / radius
                * (longitude_ascending_node.cos() * (argument_periapsis + true_anomaly).sin()
                    + longitude_ascending_node.sin()
                        * (argument_periapsis + true_anomaly).cos()
                        * inclination.cos()),
        (position.y * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            - angular_momentum / radius
                * (longitude_ascending_node.sin() * (argument_periapsis + true_anomaly).sin()
                    - longitude_ascending_node.cos()
                        * (argument_periapsis + true_anomaly).cos()
                        * inclination.cos()),
        (position.z * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            + angular_momentum / radius
                * inclination.sin()
                * (argument_periapsis + true_anomaly).cos(),
    )
}

pub fn elements_to_state(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    argument_periapsis: Float,
    longitude_ascending_node: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
    time: Float,
) -> Vec6 {
    let mean_angular_motion = mean_angular_motion(gravitational_parameter, semi_major_axis);
    let mean_anomaly = mean_anomaly(mean_angular_motion, time, time_at_periapsis);
    let eccentric_anomaly = eccentric_anomaly(mean_anomaly, eccentricity);
    let radius = radius(semi_major_axis, eccentricity, eccentric_anomaly);

    let true_anomaly = true_anomaly_from_eccentric_anomaly(eccentricity, eccentric_anomaly);
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let angular_momentum = angular_momentum(gravitational_parameter, semi_major_axis, eccentricity);

    let pos = position(
        radius,
        longitude_ascending_node,
        argument_periapsis,
        true_anomaly,
        inclination,
    );
    let vel = velocity(
        &pos,
        angular_momentum,
        eccentricity,
        radius,
        p,
        true_anomaly,
        longitude_ascending_node,
        argument_periapsis,
        inclination,
    );
    Vec6::new(pos.x, pos.y, pos.z, vel.x, vel.y, vel.z)
}

pub fn position_in_perifocal_frame(
    semi_major_axis: Float,
    eccentricity: Float,
    true_anomaly: Float,
) -> Vec3 {
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * true_anomaly.cos());

    r * vec3(true_anomaly.cos(), true_anomaly.sin(), 0.0)
}

pub fn position_in_inertial_frame_from_true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    true_anomaly: Float,
) -> Vec3 {
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * true_anomaly.cos());

    r * vec3(
        (true_anomaly + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos()
                * (true_anomaly + argument_periapsis).sin()
                * longitude_ascending_node.sin(),
        (true_anomaly + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos()
                * (true_anomaly + argument_periapsis).sin()
                * longitude_ascending_node.cos(),
        (true_anomaly + argument_periapsis).sin() * inclination.sin(),
    )
}

pub fn position_in_inertial_frame_from_mean_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    mean_anomaly: Float,
) -> Vec3 {
    #[allow(non_snake_case)]
    let E = eccentric_anomaly(mean_anomaly, eccentricity);
    let v = true_anomaly_from_eccentric_anomaly(eccentricity, E);
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * v.cos());

    r * vec3(
        (v + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.sin(),
        (v + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.cos(),
        (v + argument_periapsis).sin() * inclination.sin(),
    )
}

pub fn position_in_inertial_frame(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    time: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
) -> Vec3 {
    let v = true_anomaly(
        semi_major_axis,
        eccentricity,
        time,
        time_at_periapsis,
        gravitational_parameter,
    );
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * v.cos());

    r * vec3(
        (v + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.sin(),
        (v + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.cos(),
        (v + argument_periapsis).sin() * inclination.sin(),
    )
}
