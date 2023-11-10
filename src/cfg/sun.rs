use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgSun {
    #[serde(default = "default_sun_position")]
    pub position: Vec3,
}

impl Configuration for CfgSun {}

fn default_sun_position() -> Vec3 {
    Vec3::x()
}
