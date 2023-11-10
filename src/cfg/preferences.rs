use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgPreferences {
    #[serde(default = "default_runs")]
    pub runs: PathBuf,
}

impl Configuration for CfgPreferences {}

impl Default for CfgPreferences {
    fn default() -> Self {
        Self {
            runs: default_runs(),
        }
    }
}

pub fn default_runs() -> PathBuf {
    let dirs = UserDirs::new().unwrap();
    dirs.desktop_dir().unwrap().join("kalast-runs")
}
