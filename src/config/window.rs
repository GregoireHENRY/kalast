use crate::{util::*, Colormap, WINDOW_HEIGHT, WINDOW_WIDTH};

use serde::{Deserialize, Serialize};

pub const CMAP_VMIN: Float = 0.0;
pub const CMAP_VMAX: Float = 1.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgWindow {
    #[serde(default = "default_width")]
    pub width: usize,

    #[serde(default = "default_height")]
    pub height: usize,

    #[serde(default)]
    pub fullscreen: bool,

    #[serde(default)]
    pub background: Vec3,

    #[serde(default)]
    pub high_dpi: bool,

    #[serde(default = "default_dpi")]
    pub shadow_dpi: usize,

    #[serde(default)]
    pub shadows: bool,

    #[serde(default)]
    pub ambient: Vec3,

    #[serde(default)]
    pub wireframe: bool,

    #[serde(default)]
    pub colormap: Option<CfgColormap>,

    #[serde(default)]
    pub normals: bool,

    #[serde(default = "default_normals_length")]
    pub normals_length: Float,

    #[serde(default)]
    pub export_frames: bool,

    #[serde(default)]
    pub color_selection: Vec3,
}

impl Default for CfgWindow {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            fullscreen: false,
            background: Vec3::zeros(),
            high_dpi: false,
            shadow_dpi: default_dpi(),
            shadows: false,
            ambient: Vec3::zeros(),
            wireframe: false,
            colormap: None,
            normals: false,
            normals_length: default_normals_length(),
            export_frames: false,
            color_selection: default_color_selection(),
        }
    }
}

fn default_width() -> usize {
    WINDOW_WIDTH
}

fn default_height() -> usize {
    WINDOW_HEIGHT
}

pub fn default_dpi() -> usize {
    100
}

fn default_normals_length() -> Float {
    0.02
}

pub fn default_color_selection() -> Vec3 {
    Vec3::new(1.0, 1.0, 0.0)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgColormap {
    #[serde(default)]
    pub name: Option<Colormap>,

    #[serde(default)]
    pub vmin: Option<Float>,

    #[serde(default)]
    pub vmax: Option<Float>,

    #[serde(default)]
    pub scalar: Option<CfgScalar>,

    #[serde(default)]
    pub reverse: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgScalar {
    #[serde(rename = "angle_incidence")]
    AngleIncidence,

    #[serde(rename = "angle_emission")]
    AngleEmission,

    #[serde(rename = "angle_phase")]
    AnglePhase,

    #[serde(rename = "flux_solar")]
    FluxSolar,

    #[serde(rename = "flux_emitted")]
    FluxEmitted,

    #[serde(rename = "flux_surface")]
    FluxSurface,

    #[serde(rename = "flux_self")]
    FluxSelf,

    #[serde(rename = "flux_mutual")]
    FluxMutual,

    #[serde(rename = "temperature")]
    Temperature,

    #[serde(rename = "file")]
    File,
}

impl Default for CfgScalar {
    fn default() -> Self {
        Self::AngleIncidence
    }
}
