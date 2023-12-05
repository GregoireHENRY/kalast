use crate::{util::*, CfgBodyError, Configuration, Equatorial};

use serde::{Deserialize, Serialize};
use snafu::{prelude::*, Location};
use std::path::PathBuf;
use strum::{Display, EnumString};

pub type SceneResult<T, E = CfgSceneError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSceneError {
    CfgParsingEquatorial {
        source: CfgBodyError,
        location: Location,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgScene {
    #[serde(default)]
    pub spice: Option<PathBuf>,

    #[serde(default)]
    pub camera: CfgCamera,

    #[serde(default)]
    pub sun: CfgSun,
}

impl Default for CfgScene {
    fn default() -> Self {
        Self {
            spice: None,
            camera: CfgCamera::default(),
            sun: CfgSun::default(),
        }
    }
}

impl Configuration for CfgScene {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgCamera {
    #[serde(rename = "position")]
    Position(Vec3),

    /// Camera in from the direction of the Sun (Sun behind camera).
    /// The float is the distance from the center of frame to the camera.
    ///
    /// TODO: COMPLETE DOC
    #[serde(rename = "from")]
    From(CfgCameraFrom),
}

impl CfgCamera {
    pub fn default_position() -> Vec3 {
        Vec3::x() * 5.0
    }
}

impl Default for CfgCamera {
    fn default() -> Self {
        Self::Position(Self::default_position())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgCameraFrom {
    #[serde(default)]
    pub from: CfgCameraFromOptions,

    #[serde(default)]
    #[serde(alias = "distance")]
    pub distance_origin: Float,
}

#[derive(Clone, Debug, Serialize, Deserialize, EnumString, Display)]
pub enum CfgCameraFromOptions {
    #[serde(rename = "sun")]
    #[serde(alias = "Sun")]
    Sun,

    #[serde(rename = "earth")]
    #[serde(alias = "Earth")]
    Earth,
}

impl Default for CfgCameraFromOptions {
    fn default() -> Self {
        Self::Sun
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgSun {
    #[serde(rename = "position")]
    Position(Vec3),

    #[serde(rename = "equatorial")]
    Equatorial(Equatorial),

    #[serde(rename = "from")]
    From(CfgSunFrom),
}

impl CfgSun {
    pub fn default_position() -> Vec3 {
        Vec3::x() * Self::default_distance()
    }

    pub fn default_distance() -> Float {
        1.0
    }
}

impl Default for CfgSun {
    fn default() -> Self {
        Self::Position(Self::default_position())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgSunFrom {
    #[serde(rename = "from_spice")]
    Spice,

    #[serde(rename = "from_orbit_body")]
    OrbitBody,
}
