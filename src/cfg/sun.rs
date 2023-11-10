use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgSun {
    #[serde(default)]
    pub position: Option<Vec3>,
}

impl Configuration for CfgSun {}