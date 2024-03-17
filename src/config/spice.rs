use serde::{Deserialize, Serialize};
use snafu::prelude::*;
use std::path::PathBuf;

pub const DEFAULT_FRAME: &str = "ECLIPJ2000";
pub const DEFAULT_ABCORR: &str = "NONE";

pub type CfgSpiceResult<T, E = CfgSpiceError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgSpiceError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSpice {
    #[serde(default)]
    pub kernel: Option<PathBuf>,

    #[serde(default)]
    pub origin: Option<String>,

    #[serde(default)]
    pub frame: Option<String>,

    #[serde(default)]
    pub abcorr: Option<String>,
}

impl CfgSpice {
    pub fn is_loaded(&self) -> bool {
        self.kernel.is_some()
    }
}

impl Default for CfgSpice {
    fn default() -> Self {
        Self {
            kernel: None,
            origin: None,
            frame: None,
            abcorr: None,
        }
    }
}
