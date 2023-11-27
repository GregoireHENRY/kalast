use crate::{
    util::*, CfgBodyError, CfgStateCartesian, CfgStateEquatorial, Configuration, Equatorial,
};

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
    pub camera: CfgCamera,

    #[serde(default)]
    pub sun: CfgSun,
}

impl Default for CfgScene {
    fn default() -> Self {
        Self {
            camera: CfgCamera::default(),
            sun: CfgSun::default(),
        }
    }
}

impl Configuration for CfgScene {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgCamera {
    #[serde(rename = "position")]
    Position(Vec3),

    /// Camera in from the direction of the Sun (Sun behind camera).
    /// The float is the distance from the center of frame to the camera.
    #[serde(rename = "sun")]
    Sun(Float),

    #[serde(rename = "earth")]
    Earth(Float),
}

impl Default for CfgCamera {
    fn default() -> Self {
        Self::Sun(5.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CfgSun {
    #[serde(rename = "cartesian")]
    #[serde(alias = "cart")]
    Cartesian(CfgStateCartesian),

    #[serde(rename = "equatorial")]
    #[serde(alias = "astro")]
    Equatorial(CfgStateEquatorial),
}

impl CfgSun {
    pub fn as_equatorial(&self) -> SceneResult<Equatorial> {
        match self {
            Self::Equatorial(coords) => coords.parse().context(CfgParsingEquatorialSnafu),
            _ => panic!("nono"),
        }
    }
}

impl Default for CfgSun {
    fn default() -> Self {
        Self::Cartesian(CfgStateCartesian::default())
    }
}
