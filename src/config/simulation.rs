use crate::{config::Error, config::FileSetup};

use serde::{Deserialize, Serialize};
use snafu::prelude::*;

pub type CfgSimulationResult<T, E = CfgSimulationError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSimulationError {
    CfgSpiceError { source: Error },
}

impl From<Error> for CfgSimulationError {
    fn from(value: Error) -> Self {
        Self::CfgSpiceError { source: value }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CfgSimulation {
    #[serde(default)]
    pub routines: Option<CfgRoutines>,

    // In seconds.
    #[serde(default)]
    pub start: Option<TimeOption>,

    // In seconds.
    #[serde(default)]
    pub start_offset: Option<isize>,

    // In seconds.
    #[serde(default)]
    pub elapsed: Option<usize>,

    #[serde(default)]
    pub step: Option<usize>,

    #[serde(default)]
    pub duration: Option<usize>,

    #[serde(default)]
    pub export: Option<CfgTimeExport>,

    #[serde(default)]
    pub pause_first_it: Option<bool>,

    #[serde(default)]
    pub file: Option<FileSetup>,

    #[serde(default)]
    pub read_file_data_only: Option<bool>,

    #[serde(default)]
    pub self_shadowing: Option<bool>,

    #[serde(default)]
    pub mutual_shadowing: Option<bool>,

    #[serde(default)]
    pub self_heating: Option<bool>,

    #[serde(default)]
    pub mutual_heating: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
// #[serde(tag = "type", content = "args")]
pub enum TimeOption {
    #[serde(rename = "seconds")]
    Seconds(usize),

    #[serde(rename = "time")]
    Time(String),
}

impl TimeOption {
    pub fn seconds(&self) -> CfgSimulationResult<usize> {
        match self {
            Self::Seconds(v) => Ok(*v),
            Self::Time(_s) => {
                #[cfg(feature = "spice")]
                return Ok(spice::str2et(_s) as usize);
                #[cfg(not(feature = "spice"))]
                return Err(CfgError::FeatureSpiceNotEnabled {}).context(CfgSpiceSnafu);
            }
        }
    }
}

impl Default for TimeOption {
    fn default() -> Self {
        Self::Seconds(0)
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CfgTimeExport {
    #[serde(default)]
    pub step: Option<usize>,

    #[serde(default)]
    pub duration: Option<usize>,

    #[serde(default)]
    pub period: Option<usize>,

    #[serde(default)]
    pub cooldown_start: Option<usize>,
}
