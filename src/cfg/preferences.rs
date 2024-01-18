use crate::{
    cfg::config::Configuration, util::*, KEY_BACKWARD, KEY_FORWARD, KEY_LEFT, KEY_RIGHT,
    SENSITIVITY,
};

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

    #[serde(default)]
    pub keys: CfgKeys,

    #[serde(default = "default_sensitivity")]
    pub sensitivity: Float,

    #[serde(default)]
    pub touchpad_controls: bool,
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
            keys: CfgKeys::default(),
            sensitivity: default_sensitivity(),
            touchpad_controls: false,
        }
    }
}

pub fn default_runs() -> PathBuf {
    let dirs = UserDirs::new().unwrap();
    dirs.desktop_dir().unwrap().join("kalast-runs")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgKeys {
    #[serde(default = "default_key_forward")]
    pub forward: String,

    #[serde(default = "default_key_left")]
    pub left: String,

    #[serde(default = "default_key_backward")]
    pub backward: String,

    #[serde(default = "default_key_right")]
    pub right: String,
}

impl Configuration for CfgKeys {}

impl Default for CfgKeys {
    fn default() -> Self {
        Self {
            forward: default_key_forward(),
            left: default_key_left(),
            backward: default_key_backward(),
            right: default_key_right(),
        }
    }
}

fn default_key_forward() -> String {
    KEY_FORWARD.name()
}

fn default_key_left() -> String {
    KEY_LEFT.name()
}

fn default_key_backward() -> String {
    KEY_BACKWARD.name()
}

fn default_key_right() -> String {
    KEY_RIGHT.name()
}

fn default_sensitivity() -> Float {
    SENSITIVITY
}
