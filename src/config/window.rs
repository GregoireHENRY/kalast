use crate::{util::*, Colormap};

use serde::{Deserialize, Serialize};

pub const CMAP_VMIN: Float = 0.0;
pub const CMAP_VMAX: Float = 1.0;
pub const DPI: usize = 100;
pub const NORMAL_LENGTH: Float = 0.02;

pub const COLOR_SELECTION: Vec3 = Vec3::new(1.0, 1.0, 0.0);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CfgWindow {
    #[serde(default)]
    pub width: Option<usize>,

    #[serde(default)]
    pub height: Option<usize>,

    #[serde(default)]
    pub fullscreen: Option<bool>,

    #[serde(default)]
    pub background: Option<Vec3>,

    #[serde(default)]
    pub high_dpi: Option<bool>,

    #[serde(default)]
    pub shadow_dpi: Option<usize>,

    #[serde(default)]
    pub shadows: Option<bool>,

    #[serde(default)]
    pub ambient: Option<Vec3>,

    #[serde(default)]
    pub wireframe: Option<bool>,

    #[serde(default)]
    pub colormap: Option<CfgColormap>,

    #[serde(default)]
    pub normals: Option<bool>,

    #[serde(default)]
    pub normals_length: Option<Float>,

    #[serde(default)]
    pub export_frames: Option<bool>,

    #[serde(default)]
    pub color_selection: Option<Vec3>,

    #[serde(default)]
    pub selecting_facet_shows_view_factor: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

    #[serde(rename = "view_factor")]
    ViewFactor,
}
