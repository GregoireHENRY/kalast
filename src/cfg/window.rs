use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgWindow {
    #[serde(default = "default_width")]
    pub width: usize,

    #[serde(default = "default_height")]
    pub height: usize,

    #[serde(default)]
    pub fullscreen: bool,

    #[serde(default)]
    pub high_dpi: bool,

    #[serde(default = "default_dpi")]
    pub shadow_dpi: usize,

    #[serde(default)]
    pub shadows: bool,

    #[serde(default)]
    pub orthographic: bool,

    #[serde(default = "default_camera_speed")]
    pub camera_speed: Float,

    #[serde(default)]
    pub ambient: Vec3,

    #[serde(default)]
    pub wireframe: bool,

    #[serde(default)]
    pub colormap: CfgColormap,

    #[serde(default)]
    pub normals: bool,

    #[serde(default = "default_normals_length")]
    pub normals_length: Float,

    #[serde(default)]
    pub export_frames: bool,
}

impl Configuration for CfgWindow {}

impl Default for CfgWindow {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            fullscreen: false,
            high_dpi: false,
            shadow_dpi: default_dpi(),
            shadows: false,
            orthographic: false,
            camera_speed: default_camera_speed(),
            ambient: Vec3::zeros(),
            wireframe: false,
            colormap: CfgColormap::default(),
            normals: false,
            normals_length: default_normals_length(),
            export_frames: false,
        }
    }
}

fn default_width() -> usize {
    crate::win::window_settings::WINDOW_WIDTH
}

fn default_height() -> usize {
    crate::win::window_settings::WINDOW_HEIGHT
}

fn default_camera_speed() -> Float {
    0.5
}

pub fn default_dpi() -> usize {
    100
}

fn default_normals_length() -> Float {
    0.02
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgColormap {
    #[serde(default)]
    pub name: Colormap,

    #[serde(default = "default_colormap_vmin")]
    pub vmin: Float,

    #[serde(default = "default_colormap_vmax")]
    pub vmax: Float,

    #[serde(default)]
    pub scalar: Option<CfgScalar>,

    #[serde(default)]
    pub reverse: bool,
}

impl Default for CfgColormap {
    fn default() -> Self {
        Self {
            name: Colormap::default(),
            vmin: default_colormap_vmin(),
            vmax: default_colormap_vmax(),
            scalar: None,
            reverse: false,
        }
    }
}

fn default_colormap_vmin() -> Float {
    0.0
}

fn default_colormap_vmax() -> Float {
    1.0
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
}

impl Default for CfgScalar {
    fn default() -> Self {
        Self::AngleIncidence
    }
}
