use crate::{util::*, AstronomicalAngleConversionError, ColorMode, Equatorial, Material, Shapes};

use figment::value::Value;
use serde::{Deserialize, Serialize};
use snafu::{prelude::*, Location};
use std::{collections::HashMap, ops::Range, path::PathBuf};

pub type BodyResult<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
pub enum Error {
    AngleParsing {
        source: AstronomicalAngleConversionError,
        location: Location,
    },
}

/**
# Configuration of Body

Each config file in the folder `cfg/bodies/` will be a new body, given that they are defined with unique name.
Bodies take their name from the name of the file, but it can be forced by a variable called [`id`][CfgBody::id].

You can configure the properties of your bodies, for example the [mesh][CfgMesh], [interior][CfgInterior],
[materials][Material], [spin][CfgSpin], [orbit][CfgState], and many more.
All the [fields of `CfgBody`][CfgBody#fields] are optionals in config file.

## Simple Configuration for Viewer

File `body.yaml`

```yaml
mesh:
  shape: sphere
```

With the [`mesh`][CfgMesh] keyword, we are simply using the
[shape of the sphere already included in **kalast**][Shapes::Sphere] (see the [see the list][Shapes#variants]).

All the other fields are using their default values.
Only the field [`id`][CfgBody::id] takes the value `body` from the file name.

## Simple Configuration for Thermophysical Simulation

File `body.yaml`

```yaml
mesh:
  shape: sphere
material:
  albedo: 0.1
  emissivity: 0.9
  thermal_inertia: 50
  density: 2100
  heat_capacity: 600
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
- we change [`color` mode][ColorMode] to [`data`][ColorMode::Data] to show the temperature.
  If not mentioned it defaults to [`diffuse_light`][ColorMode::DiffuseLight] to show the diffuse light.
- we want the heat to transfer inside the body so we set an [`interior`][CfgInterior]. Here it is a
  [`linear` (constant grid)][CfgInteriorGrid1D::Linear] of 40 layers each separated by 2 cm (total depth 80 cm).
- we want the asteroid to [`spin`][CfgSpin] at a period of 2 hours.
- we want to [`record`][CfgRecord] the simulated vertical temperatures in depth at longitude/latitude=0° (which correspond to the
  facet index #114 for the sphere shape)

Same as the simple example for viewer, not all fields are mentioned here, so all the other fields are using their
default values.

## Default Configuration

```yaml
id: 0

mesh:
  shape: sphere
  factor: [1, 1, 1]
  smooth: false

mesh_low: None

material:
  albedo: 0
  emissivity: 1
  thermal_inertia:
  density: 0
  heat_capacity: 0

color: diffuse_light

interior: None

spin:
  period: 0
  axis: [0, 0, 1]
  obliquity: 0
  spin0: 0

state:
  type: manual
  position: [0.0, 0.0, 0.0]
  orientation: [
    1.0, 0.0, 0.0,
    0.0, 1.0, 0.0,
    0.0, 0.0, 1.0
  ]

mass: None

temperature: 0

record:
  faces: []
  columns: []
  rows: []
  cells: []
```

*/
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Body {
    /// Unique name for a body.
    ///
    /// By default it uses the name of the yaml file but can be forced to a string by setting a value to this field.
    /// If the id is not unique, this body replaces the other one.
    ///
    /// ### Example
    ///
    /// ```yaml
    /// name: My body
    /// ```
    #[serde(default = "default_body_name")]
    pub name: String,

    #[serde(default)]
    pub frame: Option<String>,

    /// Surface mesh of the body.
    /// Read [`CfgMesh`] for configuration options and examples.
    #[serde(default)]
    pub mesh: Mesh,

    /// Optional second surface mesh for body.
    /// It can be used for faster shadow computation with a lower resolution mesh.
    /// Default is `None`.
    #[serde(default)]
    pub mesh_low: Option<Mesh>,

    /// Material of the surface.
    /// Read [`Material`] for configuration options and examples.
    #[serde(default)]
    pub material: Material,

    /// Color mode. See [possibilities][ColorMode#variants].
    #[serde(default)]
    pub color: ColorMode,

    /// Interior for body.
    /// Read [`CfgInterior`] for configuration options and examples.
    /// Default is `None`.
    #[serde(default)]
    pub interior: Option<Interior>,

    /// Body spin.
    /// Read [`CfgSpin`] for configuration options and examples.
    #[serde(default)]
    pub spin: Spin,

    /// State (position & orientation) of the body.
    /// Read [`CfgState`] for configuration options and examples.
    #[serde(default)]
    pub state: State,

    /// If you need to mention the mass of the body, it's here (in kg).
    #[serde(default)]
    pub mass: Option<Float>,

    /// Configuration of the initialisation of the temperature of the body. Default is `0` for zero everywhere
    /// on the surface.
    /// Read [`CfgTemperatureInit`] for configuration options and examples.
    #[serde(default)]
    pub temperature: TemperatureInit,

    /// Configuration of the record of the data. Default is nothing is recorded.
    /// Read [`CfgRecord`] for configuration options and examples.
    #[serde(default)]
    pub record: Record,

    #[serde(default)]
    pub file_data: Option<FileSetup>,

    #[serde(default)]
    pub faces_selected: Vec<usize>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl Body {
    pub fn extra(&self) -> &HashMap<String, Value> {
        &self.extra
    }
}

pub fn default_body_name() -> String {
    "".to_string()
}

/**
# Configuration of Surface Mesh for Body.

## Example Sphere

See [list of already included shapes][Shapes#variants].

```yaml
shape: sphere
```

## Example Ellipsoid

A sphere can be rescaled into an ellipsoid with factor multiplication on x, y & z axes.

```yaml
shape: sphere
factor: [1.0, 0.9, 0.6]
```

## Example Smooth Sphere

The rendering of the mesh can be smoothed.
The thermophysical model only works with `flat` (as opposed to `smooth`) so try it only for "viewer" routines.

```yaml
shape: sphere
smooth: true
```

## Example Custom Shape

The shape model can be loaded from user files as Wavefront format.

```yaml
shape: path/to/ryugu.obj
```

## Default

```yaml
shape: sphere
factor: [1, 1, 1]
smooth: false
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mesh {
    /// Can be a [shape already included][Shapes] in **kalast** or a path to a custom shape model.
    /// Default is [sphere][`Shapes::Sphere`].
    #[serde(default)]
    pub shape: MeshSource,

    /// Resize correction to be applied to the mesh.
    /// Default is `[1, 1, 1]`.
    #[serde(default = "default_mesh_factor")]
    pub factor: Vec3,

    /// Position correction to be applied to the mesh.
    /// Default is `[0, 0, 0]`.
    #[serde(default = "default_vec3_zeros")]
    pub position: Vec3,

    /// Orientation correction to be applied to the mesh.
    /// Default Mat3 identity.
    #[serde(default = "default_mat3_identity")]
    pub orientation: Mat3,

    /// Wether to render vertex- (smooth) or facet-wise (flat).
    /// Default is flat, smooth is `false`.
    /// Smooth does not work for thermophysical model, only for viewer.
    #[serde(default)]
    pub smooth: bool,
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            shape: MeshSource::default(),
            factor: default_mesh_factor(),
            position: default_vec3_zeros(),
            orientation: default_mat3_identity(),
            smooth: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MeshSource {
    #[serde(rename = "shape")]
    Shape(Shapes),

    #[serde(rename = "path")]
    Path(PathBuf),
}

impl Default for MeshSource {
    fn default() -> Self {
        Self::Shape(Shapes::Sphere)
    }
}

#[derive(Clone, Debug)]
pub enum MeshKind {
    Main,
    Low,
}

impl Default for MeshKind {
    fn default() -> Self {
        Self::Main
    }
}

/**
# Configuration of Interior for Body.

There is no real option here for the moment, except whether you want interior or not.

The only interior available as of now is 1D grid from each facet of the surface mesh toward center of body.

We plan to implement tetrahedral interior for FEM thermophysical model and this the place where it will be put.

See [the 1D grid][CfgInteriorGrid1D] for more details.
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Interior {
    #[serde(rename = "grid1d")]
    Grid1D(InteriorGrid1D),
}

/**
# Configuration of Interior as 1D Grid

Depth step with parameter `a` is in meter.

## Example: Constant depth grid

Constant depth grid of 2 cm step until total 80 cm depth.

```yaml
type: linear
size: 40
a: 2e-2
```

Yielding:
```
indices:     0     1     2     3     4     5     ... 40
depth (m):   0     0.02  0.04  0.06  0.08  0.1   ... 0.8
```

## Example: Grid depth step variation with Pow function

```yaml
type: pow
size: 10
a: 2e-2
```

Yielding:
```
indices:     0     1     2     3     4     5     ... 10
depth (m):   0     0.02  0.08  0.18  0.32  0.5   ... 2
```

## Example: Grid depth step variation with Exponential function

```yaml
type: exp
size: 10
a: 1e-3
```

Yielding:
```
indices:     0     1     2     3     4     5     ... 10
depth (m):   0     0.002 0.006 0.019 0.054 0.147 ... 22.025
```

## Example: from File

Unimplemented.

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InteriorGrid1D {
    /// Linear depth function:
    ///
    /// ```
    /// z(x)=a*x
    /// ```
    ///
    /// with, `x` integer from `0` to `size`
    #[serde(rename = "linear")]
    Linear { size: usize, a: Float },

    /// Pow depth function:
    ///
    /// ```
    /// z(x)=a*x^n
    /// ```
    ///
    /// with, `x` integer from `0` to `size`, and `n` a float.
    #[serde(rename = "pow")]
    Pow { size: usize, a: Float, n: Float },

    /// Exponential depth function:
    ///
    /// ```
    /// z(x)=a*exp(x)-a
    /// ```
    ///
    /// with, `x` integer from `0` to `size`
    #[serde(rename = "exp")]
    Exp { size: usize, a: Float },

    #[serde(rename = "increasing")]
    Increasing {
        skin: SkinDepth,
        m: usize,
        n: usize,
        b: usize,
    },

    /// Unimplemented.
    #[serde(rename = "file")]
    File { path: PathBuf },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SkinDepth {
    #[serde(rename = "one")]
    One,

    #[serde(rename = "two_pi")]
    TwoPi,
}

/**
# Configuration of the Spin of the Body.

## Default

```yaml
period: 0
axis: [0, 0, 1]
obliquity: 0
spin0: 0
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Spin {
    /// Sidereal rotation period (in seconds).
    /// Time to make one rotation around its spin axis.
    /// Default is `0.0`.
    #[serde(default)]
    pub period: Float,

    /// Spin axis in body-fixed frame.
    /// Default is `[0.0, 0.0, 1.0]`  (+Z axis).
    #[serde(default = "default_spin_axis")]
    pub axis: Vec3,

    /// Tilt of the spin axis (in degrees).
    /// Default is `0.0`.
    #[serde(default)]
    pub obliquity: Float,

    /// Pre-rotation already done around spin axis (in degrees).
    /// Default is `0.0`.
    #[serde(default)]
    pub spin0: Float,
}

impl Default for Spin {
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

/**
# Configuration of the State of the Body

Position vectors are expected in km.

[Three `type`s are available][CfgState#variants]:

- [`cartesian`][CfgStateCartesian]:
  [configuring position and/or orientation matrix from cartesian coordinates][CfgStateCartesian]
- [`orbit`][CfgOrbitKepler]: [defining an orbit][CfgOrbitKepler]
- `file`: reading state from a file (unimplemented)

## Do Not Forget

The configuration of the state cannot detect automatically which type to serialize.
It needs the mention of the type.
Use the field `type` followed by one of the three options, and then the actual values.

### Examples

#### Type Manual

```yaml
type: cartesian
position: [1.0, 0.0, 0.0]
```

#### Type Orbit

```yaml
type: orbit
a: 1
e: 0.5
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum State {
    #[serde(rename = "cartesian")]
    Cartesian(StateCartesian),

    #[serde(rename = "equatorial")]
    Equatorial(Equatorial),

    #[serde(rename = "orbit")]
    Orbit(StateOrbit),

    #[serde(rename = "file")]
    File,

    #[serde(rename = "spice")]
    Spice,

    #[serde(rename = "spice_state")]
    SpiceState(SpiceState),
}

impl Default for State {
    fn default() -> Self {
        Self::Cartesian(StateCartesian::default())
    }
}

/**
# Manual Configuration of Position and Orientation of Body from Cartesian Coordinates

Position vectors are expected in km.

## Default

```yaml
position: [0, 0, 0]
orientation: [
  1, 0, 0,
  0, 1, 0,
  0, 0, 1
]
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateCartesian {
    /// Position of the body.
    /// Default is `[0.0, 0.0, 0.0]`
    #[serde(default)]
    pub position: Vec3,

    /// Orientation matrix of the body.
    /// Default is: `[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]`
    #[serde(default = "default_orientation")]
    pub orientation: Mat3,

    #[serde(default)]
    pub reference: Option<String>,
}

impl Default for StateCartesian {
    fn default() -> Self {
        Self {
            position: Vec3::zeros(),
            orientation: default_orientation(),
            reference: None,
        }
    }
}

fn default_orientation() -> Mat3 {
    Mat3::identity()
}

/**
# Configuration of the Orbit of the Body

## Default

```yaml
a: 1
e: 0,
i: 0,
peri: 90,
node: 0,
tp: 0,
frame: sun,
control: None,
```

## Note

For thermophysical modelling, what usually matters is the illumination.
For that, just the obliquity with respect to ecliptic and the relative direction of the Sun are enough.
In this sense, just [`a`][CfgOrbitKepler::a] and [`e`][CfgOrbitKepler::e] are useful parameters here if
[the `obliquity`][CfgSpin::obliquity] is correctly set in [the configuration of the spin of the body][CfgSpin].

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateOrbit {
    /// Heliocentric distance.
    /// The units are determined automatically depending on the center of the frame:
    ///
    /// - in AU if in Sun-centered frame
    /// - in km if in body-centered frame
    ///
    /// Default is `1.0`.
    #[serde(default = "default_orbit_a")]
    pub a: Float,

    /// Eccentricity. Default is `0.0`.
    #[serde(default)]
    pub e: Float,

    /// Inclination (in degrees). Default is `0.0`.
    #[serde(default)]
    pub i: Float,

    /// Argument of the periapsis (in degrees). Default is `90.0`.
    #[serde(default = "default_orbit_peri")]
    pub peri: Float,

    /// Longitude of the ascending node (in degrees). Default is `0.0`.
    #[serde(default)]
    pub node: Float,

    /// Time of passage at periapsis (in seconds). Default is `0.0`.
    #[serde(default)]
    pub tp: Float,

    /// Center of frame. Default is [the `Sun`][CfgFrameCenter::Sun].
    #[serde(default)]
    pub frame: FrameCenter,

    /// Configuration of the orbital speed of the body.
    /// [Default is the mass of the frame center][CfgOrbitSpeedControl#default].
    #[serde(default)]
    pub control: OrbitSpeedControl,
}

impl Default for StateOrbit {
    fn default() -> Self {
        Self {
            a: default_orbit_a(),
            e: 0.0,
            i: 0.0,
            peri: default_orbit_peri(),
            node: 0.0,
            tp: 0.0,
            frame: FrameCenter::default(),
            control: OrbitSpeedControl::default(),
        }
    }
}

fn default_orbit_a() -> Float {
    1.0
}

fn default_orbit_peri() -> Float {
    90.0
}

/**
# Configuration of the Framce Center of the Orbit of the Body.

Default is [the `Sun`][CfgFrameCenter::Sun].
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FrameCenter {
    #[serde(rename = "sun")]
    Sun,

    /// [ID][CfgBody::id] of a body.
    #[serde(rename = "body")]
    Body(String),
}

impl Default for FrameCenter {
    fn default() -> Self {
        Self::Sun
    }
}

/**
# Configuration of the Orbit Speed of the Body

## Default

Default is [the mass][CfgOrbitSpeedControl::Mass] but without value (`None`).
It will look for the mass of the center of frame:

- if it is [the `Sun`][CfgFrameCenter::Sun], it is the mass of the Sun
  ([actually using gravitational acceleration of the Sun][crate::MU_SUN])
- or if it is a body, look for the mass of the center body in its definition

## With Given Mass

```yaml
mass: 1e12
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OrbitSpeedControl {
    /// If mass is mentioned, in kg.
    /// This is used to compute GM of orbit and orbital speed.
    /// In configuration, use `mass`.
    #[serde(rename = "mass")]
    Mass(Option<Float>),

    /// ! unimplemented !
    /// Orbital period (in seconds).
    /// In configuration, use `period`.
    #[serde(rename = "period")]
    Period(Float),
}

impl Default for OrbitSpeedControl {
    fn default() -> Self {
        Self::Mass(None)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileSetup {
    #[serde(default)]
    pub path: Option<PathBuf>,

    #[serde(default)]
    pub behavior: Option<FileBehavior>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FileColumns {
    Time,
    Sun,
    BodyPos(usize),
    BodyRot(usize),
}

impl FileColumns {
    pub fn range(&self) -> Range<usize> {
        match self {
            Self::Time => 0..1,
            Self::Sun => 1..4,
            Self::BodyPos(index) => {
                let start = 4 + index * 12;
                start..(start + 3)
            }
            Self::BodyRot(index) => {
                let start = 7 + index * 12;
                start..(start + 9)
            }
        }
    }

    pub fn get(&self, row: &[f64]) -> FileColumnsOut {
        match &self {
            Self::Time => FileColumnsOut::Scalar(row[self.range()][0]),
            Self::Sun | Self::BodyPos(_) => {
                FileColumnsOut::Vec(Vec3::from_row_slice(&row[self.range()]))
            }
            Self::BodyRot(_) => FileColumnsOut::Mat(Mat3::from_row_slice(&row[self.range()])),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FileColumnsOut {
    Scalar(f64),
    Vec(Vec3),
    Mat(Mat3),
}

impl FileColumnsOut {
    pub fn scalar(self) -> f64 {
        match self {
            Self::Scalar(v) => v,
            _ => panic!("Impossible to extract a scalar from {:?}", self),
        }
    }

    pub fn vec(self) -> Vec3 {
        match self {
            Self::Vec(v) => v,
            _ => panic!("Impossible to extract a vector from {:?}", self),
        }
    }

    pub fn mat(self) -> Mat3 {
        match self {
            Self::Mat(m) => m,
            _ => panic!("Impossible to extract a matrice from {:?}", self),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FileBehavior {
    #[serde(rename = "stop")]
    Stop,

    #[serde(rename = "loop")]
    Loop,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SpiceState {
    #[serde(default)]
    pub position: Option<SpicePosition>,

    #[serde(default)]
    pub frame_to: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SpicePosition {
    #[serde(default)]
    pub origin: Option<String>,

    #[serde(default)]
    pub abcorr: Option<String>,
}

/**
# Configuration of the Initialisation of the Temperature of the Body.

## Default

Default is zero everywhere ([`Scalar`][CfgTemperatureInit::Scalar] of `0`).

## A Different Value

```yaml
150
```

## Effective Temperature

For the ratio `1/4`:

```yaml
[1, 4]
```

*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TemperatureInit {
    /// A uniform scalar value.
    /// In configuration, use `scalar`.
    #[serde(rename = "scalar")]
    Scalar(Float),

    /// Ratio between in & out surface area for the formula of effective temperature.
    /// In configuration, use `effective`.
    #[serde(rename = "effective")]
    Effective(Option<(usize, usize)>),

    /// From a File: unimplemented!
    #[serde(rename = "path")]
    File(PathBuf),
}

impl Default for TemperatureInit {
    fn default() -> Self {
        Self::Scalar(0.0)
    }
}

/**
# Configuration of Record for data of Body

Data for a body are stored in a 2D matrix of number of surface faces the number of rows, and number of columns
the number of depth elements. So column is a 1D depth column at specific face, a row is a layer of all faces at
specific depth, and a cell is an element of the 2D matrix (flattened index).

## Default

By default, nothing is record.

## Record Example

To record data on [`faces`][CfgRecord::faces], provide a list of indices:

```yaml
faces: [0, 1, 10, 100]
```

Idem for [`columns`][CfgRecord::columns], [`rows`][CfgRecord::rows] and [`cells`][CfgRecord::cells].
You can also mention multiple fields at the same time if you want to record some columns and specific cells.

*/
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Record {
    #[serde(default)]
    pub faces: Vec<usize>,

    #[serde(default)]
    pub columns: Vec<usize>,

    #[serde(default)]
    pub rows: Vec<usize>,

    #[serde(default)]
    pub cells: Vec<usize>,

    #[serde(default)]
    pub mesh: bool,

    #[serde(default)]
    pub depth: bool,
}

fn default_mesh_factor() -> Vec3 {
    vec3(1.0, 1.0, 1.0)
}

fn default_vec3_zeros() -> Vec3 {
    Vec3::zeros()
}

fn default_mat3_identity() -> Mat3 {
    Mat3::identity()
}
