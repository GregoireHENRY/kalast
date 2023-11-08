use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgPreferences {
    #[serde(default = "default_runs")]
    pub runs: PathBuf,
}

impl Configuration for CfgPreferences {}

pub fn default_runs() -> PathBuf {
    let dirs = UserDirs::new().unwrap();
    dirs.desktop_dir().unwrap().join("kalast-runs")
}
