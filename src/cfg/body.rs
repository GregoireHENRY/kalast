use crate::cfg::config::Configuration;
use crate::prelude::*;

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

/**
# Configure the bodies of your scenario.

You can configure the properties of your Bodies, for example the [mesh][CfgMesh], [interior][CfgInterior],
[materials][Material], [spin][CfgSpin], [orbit][CfgState], and many more.

Each yaml file in the folder `cfg/bodies/` will load one body to the simulation. For example, for binary system of
asteroids the scenario expect two body config files with different names.
Bodies take their name from the name of the file, but it can be forced by a variable called [`id`][CfgBody::id].

The [fields of `CfgBody`][CfgBody#fields] are all optionals.

### Simplest example for viewer

File `body.yaml`

```yaml
mesh:
  shape: sphere
```

With the [`mesh`][CfgMesh] keyword, we are simply using the [shape of the sphere already included][Shapes::Sphere] (see the
[list of of shape models integrated to kalast][Shapes#variants]).

## Simple example for thermophysical simulation

File `body.yaml`

```yaml
mesh:
  shape: sphere
material:
  albedo: 0.1
  emissivity: 0.9
  thermal_inertia: 500.0
  density: 2100.0
  heat_capacity: 600.0
color: data
interior:
  type: linear
  size: 40
  a: 2e-2
spin:
  period: 7200
record:
  columns: [114]
```

- also using the [sphere][Shapes::Sphere].
- the [`material`][Material] sets thermophysical properties of the surface of the sphere.
- we change [`color`][ColorMode] to [`data`][ColorMode::Data] to show the temperature. If not mentioned it defaults to
  [`diffuse_light`][ColorMode::DiffuseLight] to show the diffuse light.
- we want temperature to conduct inside the body so we set an [`interior`][CfgInterior]. Here it is a
  [`linear` (constant grid)][CfgInteriorGrid1D::Linear] of 40 layers each separated by 2 cm (total depth 80 cm).
- we want the asteroid to [`spin`][CfgSpin] at a period of 2 hours.
- we want to [`record`][CfgRecord] the simulated vertical temperatures in depth at longitude/latitude=0° (which correspond to the
  facet index #114 for the sphere shape)

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgBody {
    /// Unique name for a body. By default it takes the name of the yaml file but can be forced by setting a value to
    /// this field.
    /// 
    /// ### Example
    ///
    /// ```yaml
    /// id: Didymos
    /// ```
    #[serde(default = "default_body_id")]
    pub id: String,

    /// Surface mesh for the body.
    #[serde(default)]
    pub mesh: CfgMesh,

    /// Optional second surface mesh for the body.
    /// For instance, it can be used for faster shadow computation with a lower resolution mesh.
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
    pub state: CfgState,

    #[serde(default)]
    pub mass: Option<Float>,

    #[serde(default)]
    pub temperature: CfgTemperatureInit,

    #[serde(default)]
    pub record: CfgRecord,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Configuration for CfgBody {}

fn default_body_id() -> String {
    "!empty".to_string()
}

/// Surface mesh for the body.
/// 
/// ### Example Sphere
/// 
/// See [list of already included shapes][Shapes#variants].
///
/// ```yaml
/// mesh:
///   shape: sphere
/// ```
/// 
/// ### Example Ellipsoid
/// 
/// A sphere can be rescaled into an ellipsoid with factor multiplication on x, y & z axes.
///
/// ```yaml
/// mesh:
///   shape: sphere
///   factor: [1.0, 0.9, 0.6]
/// ```
///
/// ### Example Smooth Sphere
/// 
/// The shape model can be smoothed for rendering.
/// The thermophysical model only works with `flat` (as opposed to `smooth`) so try it only for "viewer" routines.
///
/// ```yaml
/// mesh:
///   shape: sphere
///   smooth: true
/// ```
///
/// ### Example Custom Shape
/// 
/// The shape model can be loaded from user files as Wavefront format.
///
/// ```yaml
/// mesh:
///   shape: path/to/ryugu.obj
/// ```
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgMesh {
    /// Options for the source of the mesh.
    /// Default is [sphere][`Shapes::Sphere`].
    #[serde(default)]
    pub shape: CfgMeshSource,

    /// Resize factor to be applied to the mesh.
    /// Default is `[1.0, 1.0, 1.0]`.
    #[serde(default = "default_mesh_factor")]
    pub factor: Vec3,

    /// Wether to render vertex- (smooth) or facet-wise (flat). 
    /// Default is flat, smooth is `false`.
    #[serde(default)]
    pub smooth: bool,
}

impl Default for CfgMesh {
    fn default() -> Self {
        Self {
            shape: CfgMeshSource::default(),
            factor: default_mesh_factor(),
            smooth: false,
        }
    }
}

fn default_mesh_factor() -> Vec3 {
    vec3(1.0, 1.0, 1.0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgMeshSource {
    #[serde(rename = "shape")]
    Shape(Shapes),

    #[serde(rename = "path")]
    Path(PathBuf),
}

impl Default for CfgMeshSource {
    fn default() -> Self {
        Self::Shape(Shapes::Sphere)
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

#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Default for CfgSpin {
    fn default() -> Self {
        Self {
            period: 0.0,
            axis: default_spin_axis(),
            obliquity: 0.0,
            spin0: 0.0,
        }
    }
}

fn default_spin_axis() -> Vec3 {
    Vec3::z()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CfgState {
    #[serde(rename = "position")]
    Position(Vec3),

    #[serde(rename = "path")]
    Path(PathBuf),

    #[serde(rename = "orbit")]
    Orbit(CfgOrbitKepler),
}

impl Default for CfgState {
    fn default() -> Self {
        Self::Position(Vec3::zeros())
    }
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

impl Default for CfgOrbitKepler {
    fn default() -> Self {
        Self {
            a: default_orbit_a(),
            e: 0.0,
            i: 0.0,
            peri: default_orbit_peri(),
            node: 0.0,
            tp: 0.0,
            frame: CfgFrameCenter::default(),
            control: CfgOrbitSpeedControl::default(),
        }
    }
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
    pub faces: Vec<usize>,

    #[serde(default)]
    pub columns: Vec<usize>,

    #[serde(default)]
    pub rows: Vec<usize>,

    #[serde(default)]
    pub cells: Vec<usize>,
}
