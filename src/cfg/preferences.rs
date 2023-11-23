use crate::cfg::config::Configuration;

use std::path::PathBuf;
use directories::UserDirs;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgPreferences {
    #[serde(default = "default_runs")]
    pub runs: PathBuf,

    #[serde(default)]
    pub do_not_check_latest_version: bool,

    #[serde(default)]
    pub auto_update: bool,
}

impl Configuration for CfgPreferences {}

impl Default for CfgPreferences {
    fn default() -> Self {
        Self {
            runs: default_runs(),
            auto_update: false,
            do_not_check_latest_version: false,
        }
    }
}

pub fn default_runs() -> PathBuf {
    let dirs = UserDirs::new().unwrap();
    dirs.desktop_dir().unwrap().join("kalast-runs")
}
