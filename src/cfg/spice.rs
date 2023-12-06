use crate::Configuration;

use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use std::path::PathBuf;

pub type CfgSpiceResult<T, E = CfgSpiceError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSpiceError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSpice {
    #[serde(default)]
    pub kernel: Option<PathBuf>,

    #[serde(default = "default_frame")]
    pub frame: String,
}

impl Default for CfgSpice {
    fn default() -> Self {
        Self {
            kernel: None,
            frame: default_frame(),
        }
    }
}

impl Configuration for CfgSpice {}

pub fn default_frame() -> String {
    "ECLIPJ2000".to_string()
}
