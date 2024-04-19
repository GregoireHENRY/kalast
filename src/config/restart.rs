use crate::util::*;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Restart {
    pub path: Option<PathBuf>,
    pub time_step_factor: Option<Float>,
    pub time_step_export_factor: Option<Float>,
    pub depth_step_factor: Option<Float>,
}
