use crate::{util::*, CfgError, Configuration};

use serde::{Deserialize, Serialize};
use snafu::prelude::*;

pub type CfgSimulationResult<T, E = CfgSimulationError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSimulationError {
    CfgSpiceError { source: CfgError },
}

impl From<CfgError> for CfgSimulationError {
    fn from(value: CfgError) -> Self {
        Self::CfgSpiceError { source: value }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSimulation {
    #[serde(default)]
    pub routines: CfgRoutines,

    // In seconds.
    #[serde(default)]
    pub start: TimeOption,

    #[serde(default)]
    pub step: usize,

    #[serde(default)]
    pub duration: usize,

    #[serde(default)]
    pub export: CfgTimeExport,

    #[serde(default)]
    pub pause_after_first_iteration: bool,
}

impl Default for CfgSimulation {
    fn default() -> Self {
        Self {
            routines: CfgRoutines::default(),
            start: TimeOption::default(),
            step: 0,
            duration: 0,
            export: CfgTimeExport::default(),
            pause_after_first_iteration: false,
        }
    }
}

impl Configuration for CfgSimulation {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum TimeOption {
    #[serde(rename = "seconds")]
    Seconds(Float),

    #[serde(rename = "string")]
    String(String),
}

impl TimeOption {
    pub fn seconds(&self) -> CfgSimulationResult<Float> {
        match self {
            Self::Seconds(v) => Ok(*v),
            Self::String(_s) => {
                #[cfg(feature = "spice")]
                return Ok(spice::str2et(_s));
                #[cfg(not(feature = "spice"))]
                return Err(CfgError::FeatureSpiceNotEnabled {}).context(CfgSpiceSnafu);
            }
        }
    }
}

impl Default for TimeOption {
    fn default() -> Self {
        Self::Seconds(0.0)
    }
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Default for CfgTimeExport {
    fn default() -> Self {
        Self {
            step: 0,
            duration: 0,
            period: 0,
            cooldown_start: None,
        }
    }
}
