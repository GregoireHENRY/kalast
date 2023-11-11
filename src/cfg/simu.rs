use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgSimulation {
    #[serde(default)]
    pub routines: CfgRoutines,

    #[serde(default)]
    pub jd0: Float,

    #[serde(default)]
    pub step: usize,

    #[serde(default)]
    pub duration: usize,

    #[serde(default)]
    pub export: CfgTimeExport,
}

impl Configuration for CfgSimulation {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CfgRoutines {
    #[serde(rename = "viewer")]
    Viewer,

    #[serde(rename = "thermal")]
    Thermal,
}

impl Default for CfgRoutines {
    fn default() -> Self {
        Self::Viewer
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgTimeExport {
    #[serde(default)]
    pub step: usize,

    #[serde(default)]
    pub duration: usize,

    #[serde(default)]
    pub period: usize,

    #[serde(default)]
    pub cooldown_start: Option<usize>,
}
