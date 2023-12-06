use crate::{util::*, CfgBodyError, Configuration, Equatorial};

use serde::{Deserialize, Serialize};
use snafu::{prelude::*, Location};

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
    pub sun: CfgSun,

    #[serde(default)]
    pub camera: CfgCamera,
}

impl Default for CfgScene {
    fn default() -> Self {
        Self {
            sun: CfgSun::default(),
            camera: CfgCamera::default(),
        }
    }
}

impl Configuration for CfgScene {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSun {
    #[serde(default)]
    pub position: CfgSunPosition,
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
        Self {
            position: CfgSunPosition::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgSunPosition {
    #[serde(rename = "cartesian")]
    Cartesian(Vec3),

    #[serde(rename = "equatorial")]
    Equatorial(Equatorial),

    #[serde(rename = "spice")]
    #[serde(alias = "from_spice")]
    Spice,

    #[serde(rename = "body")]
    #[serde(alias = "from_body")]
    FromBody,
}

impl Default for CfgSunPosition {
    fn default() -> Self {
        Self::Cartesian(CfgSun::default_position())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgCamera {
    #[serde(default)]
    pub position: CfgCameraPosition,

    #[serde(default)]
    #[serde(alias = "distance")]
    pub distance_origin: Option<Float>,
}

impl CfgCamera {
    pub fn default_position() -> Vec3 {
        Vec3::x() * Self::default_distance()
    }

    pub fn default_distance() -> Float {
        5.0
    }
}

impl Default for CfgCamera {
    fn default() -> Self {
        Self {
            position: CfgCameraPosition::default(),
            distance_origin: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgCameraPosition {
    #[serde(rename = "cartesian")]
    Cartesian(Vec3),

    #[serde(rename = "sun")]
    #[serde(alias = "Sun")]
    #[serde(alias = "from_sun")]
    #[serde(alias = "from_Sun")]
    FromSun,

    #[serde(rename = "spice")]
    #[serde(alias = "from_spice")]
    Spice(String),

    #[serde(rename = "reference")]
    Reference,
}

impl Default for CfgCameraPosition {
    fn default() -> Self {
        Self::Cartesian(CfgCamera::default_position())
    }
}
