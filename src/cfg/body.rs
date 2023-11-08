use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgBody {
    /// ID of the body as a unique string name.
    #[serde(default = "default_body_id")]
    pub id: String,

    #[serde(default)]
    pub mesh: CfgMesh,

    #[serde(default)]
    pub mesh_low: Option<CfgMesh>,

    #[serde(default)]
    pub material: Material,

    #[serde(default)]
    pub color: ColorMode,

    #[serde(default)]
    pub interior: Option<CfgInterior>,

    #[serde(default)]
    pub spin: CfgSpin,

    #[serde(default)]
    pub state: Option<CfgState>,

    #[serde(default)]
    pub mass: Option<Float>,

    #[serde(default)]
    pub temperature_init: CfgTemperatureInit,

    #[serde(default)]
    pub record: CfgRecord,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Configuration for CfgBody {}

fn default_body_id() -> String {
    "!empty".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgMesh {
    pub shape: CfgMeshSource,

    #[serde(default = "default_mesh_factor")]
    pub factor: Vec3,
}

fn default_mesh_factor() -> Vec3 {
    vec3(1.0, 1.0, 1.0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgMeshSource {
    #[serde(rename = "shape")]
    Shape(IntegratedShapeModel),

    #[serde(rename = "path")]
    Path(PathBuf),
}

impl Default for CfgMeshSource {
    fn default() -> Self {
        Self::Shape(IntegratedShapeModel::Sphere)
    }
}

#[derive(Clone, Debug)]
pub enum CfgMeshKind {
    Main,
    Low,
}

impl Default for CfgMeshKind {
    fn default() -> Self {
        Self::Main
    }
}

// #[serde(tag = "type")]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgInterior {
    Grid1D(CfgInteriorGrid1D),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CfgInteriorGrid1D {
    #[serde(rename = "linear")]
    Linear { size: usize, a: Float },

    #[serde(rename = "pow")]
    Pow { size: usize, a: Float, n: Float },

    #[serde(rename = "exp")]
    Exp { size: usize, a: Float },

    #[serde(rename = "file")]
    File { path: PathBuf },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgSpin {
    #[serde(default)]
    pub period: Float,

    #[serde(default = "default_spin_axis")]
    pub axis: Vec3,

    #[serde(default)]
    pub obliquity: Float,

    /// Pre-rotation to consider around spin axis, in degrees.
    #[serde(default)]
    pub spin0: Float,
}

fn default_spin_axis() -> Vec3 {
    Vec3::z()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgState {
    #[serde(rename = "path")]
    Path(PathBuf),

    #[serde(rename = "orbit")]
    Orbit(CfgOrbitKepler),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgOrbitKepler {
    /// In AU if in Sun-centered frame.
    /// In km if else in body-centered frame.
    #[serde(default = "default_orbit_a")]
    pub a: Float,

    #[serde(default)]
    pub e: Float,

    /// In degrees.
    #[serde(default)]
    pub i: Float,

    /// In degrees.
    #[serde(default = "default_orbit_peri")]
    pub peri: Float,

    /// In degrees.
    #[serde(default)]
    pub node: Float,

    /// In seconds.
    #[serde(default)]
    pub tp: Float,

    #[serde(default)]
    pub frame: CfgFrameCenter,

    #[serde(default)]
    pub control: CfgOrbitSpeedControl,
}

fn default_orbit_a() -> Float {
    1.0
}

fn default_orbit_peri() -> Float {
    90.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgFrameCenter {
    Sun,

    /// String is id of the body as unique string name.
    #[serde(rename = "body")]
    Body(String),
}

impl Default for CfgFrameCenter {
    fn default() -> Self {
        Self::Sun
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgOrbitSpeedControl {
    #[serde(rename = "mass")]
    Mass(Option<Float>),

    #[serde(rename = "period")]
    Period(Float),
}

impl Default for CfgOrbitSpeedControl {
    fn default() -> Self {
        Self::Mass(None)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CfgTemperatureInit {
    #[serde(rename = "scalar")]
    Scalar(Float),

    /// The Float is the ratio between in & out surface area for the formula of effective temperature.
    #[serde(rename = "effective")]
    Effective(Float),

    #[serde(rename = "path")]
    Path(PathBuf),
}

impl Default for CfgTemperatureInit {
    fn default() -> Self {
        Self::Scalar(0.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CfgRecord {
    #[serde(default)]
    pub images: bool,

    #[serde(default)]
    pub all_once: bool,

    #[serde(default)]
    pub faces: Vec<usize>,

    #[serde(default)]
    pub columns: Vec<usize>,

    #[serde(default)]
    pub rows: Vec<usize>,

    #[serde(default)]
    pub cells: Vec<usize>,
}
