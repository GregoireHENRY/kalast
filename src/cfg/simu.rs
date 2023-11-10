use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgSimulation {
    #[serde(default)]
    pub jd0: Float,

    #[serde(default)]
    pub step: usize,

    #[serde(default)]
    pub duration: usize,

    #[serde(default)]
    pub export: CfgTimeExport,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Configuration for CfgSimulation {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgSimulationType {
    #[serde(rename = "viewer")]
    Viewer,

    #[serde(rename = "light")]
    Light,

    #[serde(rename = "thermal")]
    Thermal,
}

impl Default for CfgSimulationType {
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
