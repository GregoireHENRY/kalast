use crate::Configuration;

use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use std::path::PathBuf;

pub const DEFAULT_FRAME: &str = "ECLIPJ2000";

pub type CfgSpiceResult<T, E = CfgSpiceError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSpiceError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSpice {
    #[serde(default)]
    pub kernel: Option<PathBuf>,

    #[serde(default)]
    pub frame: Option<String>,

    #[serde(default)]
    pub origin: Option<String>,
}

impl Default for CfgSpice {
    fn default() -> Self {
        Self {
            kernel: None,
            frame: None,
            origin: None,
        }
    }
}

impl Configuration for CfgSpice {}
