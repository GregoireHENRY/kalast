use crate::util::*;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Preferences {
    pub debug: Debug,
    pub path_runs: Option<PathBuf>,
    pub do_not_check_latest_version: Option<bool>,
    pub auto_update: Option<bool>,
    pub keys: Keys,
    pub sensitivity: Option<Float>,
    pub touchpad_controls: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Debug {
    pub config: Option<bool>,
    pub general: Option<bool>,
    pub window: Option<bool>,
    pub simulation: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Keys {
    pub forward: Option<String>,
    pub left: Option<String>,
    pub backward: Option<String>,
    pub right: Option<String>,
}
