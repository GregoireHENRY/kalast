use crate::{util::*, Configuration};

use serde::{Deserialize, Serialize};

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
#[serde(untagged)]
pub enum CfgCamera {
    #[serde(rename = "position")]
    Position(Vec3),

    // The Float is distance from origin of frame.
    #[serde(rename = "sun_direction")]
    SunDirection(Float),
}

impl Default for CfgCamera {
    fn default() -> Self {
        Self::SunDirection(5.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSun {
    #[serde(default = "default_sun_position")]
    pub position: Vec3,
}

impl Default for CfgSun {
    fn default() -> Self {
        Self {
            position: default_sun_position(),
        }
    }
}

fn default_sun_position() -> Vec3 {
    Vec3::x()
}
