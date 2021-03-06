use crate::cfg::config::Configuration;

use directories::UserDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgPreferences {
    #[serde(default = "default_runs")]
    pub runs: PathBuf,

    #[serde(default)]
    #[serde(alias = "no_check")]
    pub do_not_check_latest_version: bool,

    #[serde(default)]
    pub auto_update: bool,

    #[serde(default)]
    pub debug: bool,

    #[serde(default)]
    pub debug_cfg: bool,
}

impl Configuration for CfgPreferences {}

impl Default for CfgPreferences {
    fn default() -> Self {
        Self {
            runs: default_runs(),
            do_not_check_latest_version: false,
            auto_update: false,
            debug: false,
            debug_cfg: false,
        }
    }
}

pub fn default_runs() -> PathBuf {
    let dirs = UserDirs::new().unwrap();
    dirs.desktop_dir().unwrap().join("kalast-runs")
}
