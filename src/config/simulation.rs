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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSimulation {
    #[serde(default)]
    pub routines: CfgRoutines,

    // In seconds.
    #[serde(default)]
    pub start: TimeOption,

    // In seconds.
    #[serde(default)]
    pub start_offset: isize,

    // In seconds.
    #[serde(default)]
    pub elapsed: usize,

    #[serde(default)]
    pub step: usize,

    #[serde(default)]
    pub duration: usize,

    #[serde(default)]
    pub export: CfgTimeExport,

    #[serde(default)]
    pub pause_first_it: Option<bool>,

    #[serde(default)]
    pub file: Option<FileSetup>,

    #[serde(default)]
    pub read_file_data_only: bool,
}

impl Default for CfgSimulation {
    fn default() -> Self {
        Self {
            routines: CfgRoutines::default(),
            start: TimeOption::default(),
            start_offset: 0,
            elapsed: 0,
            step: 0,
            duration: 0,
            export: CfgTimeExport::default(),
            pause_first_it: None,
            file: None,
            read_file_data_only: false,
        }
    }
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
