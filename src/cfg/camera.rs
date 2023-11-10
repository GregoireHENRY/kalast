use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgCamera {
    #[serde(rename = "position")]
    Position(Vec3),
    
    // The Float is distance from origin of frame.
    #[serde(rename = "sun_direction")]
    SunDirection(Float),
}

impl Configuration for CfgCamera {}

impl Default for CfgCamera {
    fn default() -> Self {
        Self::SunDirection(1.0)
    }
}