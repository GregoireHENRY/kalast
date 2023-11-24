use crate::{util::*, CfgStateCartesian, CfgStateEquatorial, Configuration, Equatorial, AstronomicalAngle};

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
    pub fn as_equatorial(&self) -> Equatorial {
        match self {
            Self::Equatorial(CfgStateEquatorial { ra, dec }) => {
                let ra = AstronomicalAngle::from_hms(ra).unwrap();
                let dec = AstronomicalAngle::from_dms(dec).unwrap();
                Equatorial::new(ra, dec)
            }
            _ => panic!("nono"),
        }
    }
}

impl Default for CfgSun {
    fn default() -> Self {
        Self::Cartesian(CfgStateCartesian::default())
    }
}
