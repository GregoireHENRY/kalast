use crate::{config::SpicePosition, util::*, Equatorial, Error, ProjectionMode};

use serde::{Deserialize, Serialize};
use snafu::{prelude::*, Location};

pub type SceneResult<T, E = CfgSceneError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSceneError {
    CfgParsingEquatorial { source: Error, location: Location },
}

/// Position vectors are expected in km.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgScene {
    #[serde(default)]
    pub sun: CfgSun,

    #[serde(default)]
    pub camera: CfgCamera,
}

/// Position vectors are expected in km.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
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

/// Position vectors are expected in km.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgSunPosition {
    #[serde(rename = "cartesian")]
    Cartesian(Vec3),

    #[serde(rename = "equatorial")]
    Equatorial(Equatorial),

    #[serde(rename = "spice")]
    Spice,

    #[serde(rename = "origin")]
    Origin,

    #[serde(rename = "file")]
    File,
}

impl Default for CfgSunPosition {
    fn default() -> Self {
        Self::Cartesian(CfgSun::default_position())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgCamera {
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub position: Option<CfgCameraPosition>,

    // In case position OPT is not cartesian we need that.
    #[serde(default)]
    #[serde(alias = "distance")]
    pub distance_origin: Option<Float>,

    #[serde(default)]
    #[serde(rename = "direction")]
    pub direction: CfgCameraDirection,

    #[serde(default = "default_camera_anchor")]
    pub anchor: Vec3,

    #[serde(default = "default_camera_up")]
    pub up: Vec3,

    #[serde(default)]
    pub projection: Option<ProjectionMode>,

    #[serde(default)]
    pub near: Option<Float>,

    #[serde(default)]
    pub far: Option<Float>,
}

impl CfgCamera {
    pub fn default_position() -> Vec3 {
        crate::POSITION
    }

    pub fn default_distance() -> Float {
        Self::default_position().magnitude()
    }
}

impl Default for CfgCamera {
    fn default() -> Self {
        Self {
            name: None,
            position: None,
            distance_origin: None,
            direction: CfgCameraDirection::default(),
            anchor: default_camera_anchor(),
            up: default_camera_up(),
            projection: None,
            near: None,
            far: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
// #[serde(untagged)]
// #[serde(tag = "type", content = "args")]
pub enum CfgCameraPosition {
    #[serde(rename = "cartesian")]
    Cartesian(Vec3),

    #[serde(rename = "sun")]
    #[serde(alias = "Sun")]
    #[serde(alias = "from_sun")]
    #[serde(alias = "from_Sun")]
    FromSun,

    #[serde(rename = "spice")]
    Spice,

    #[serde(rename = "spice_pos")]
    SpicePos(SpicePosition),

    #[serde(rename = "reference")]
    Reference, // distance origin
}

impl Default for CfgCameraPosition {
    fn default() -> Self {
        Self::Cartesian(CfgCamera::default_position())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CfgCameraDirection {
    #[serde(rename = "cartesian")]
    #[serde(alias = "vector")]
    Cartesian(Vec3),

    #[serde(rename = "target_anchor")]
    #[serde(alias = "anchor")]
    TargetAnchor,
}

impl Default for CfgCameraDirection {
    fn default() -> Self {
        Self::TargetAnchor
    }
}

fn default_camera_anchor() -> Vec3 {
    crate::ANCHOR
}

fn default_camera_up() -> Vec3 {
    crate::UP
}
