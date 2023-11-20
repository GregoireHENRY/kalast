/*!
# Configure your scenarios.

To configure the scenario of your simulations, you can use an existing configuration or write your own config file.
**kalast** will look for a folder named `cfg/` containing the configs files under the [yaml][url-yaml] format.

The configuration is done using the following strcuture:

- `cfg/bodies/ *.yaml` - [`CfgBody`]: the mesh, interior, materials, spin, orbit, ... for the body of your
  simulation.
  As there can be multiple bodies (for example two bodies for binary system of asteroids), the bodies are
  configured in a folder called `cfg/bodies/` inside the `cfg/` folder. Each config file will be considered as a body.
  The name of the body is its filename or can be forced by a variable called [`id`][CfgBody::id].
- `cfg/camera.yaml` - [`CfgCamera`]: options for the position of the camera.
- `cfg/sun.yaml` - [`CfgSun`]: options for the position of the Sun.
- `cfg/simulation.yaml` - [`CfgSimulation`]: to configure the simulation and time.
- `cfg/window.yaml` - [`CfgWindow`]: options for the window and rendering
- `preferences.yaml` - [`CfgPreferences`]: for general preferences, is configured outside of the folder
  `cfg/`, next to the executable.
- `cfg/cfg.yaml` - [`Cfg`]: if you want, you can write everything mentioned above in a single file.
  It is the parent config and regroup all of the above structure. For conflicts, this file is loaded first and the
  files above are loaded after and will overwrite parameters.


[url-yaml]: https://yaml.org
*/

pub mod body;
pub mod camera;
pub mod config;
pub mod preferences;
pub mod simu;
pub mod sun;
pub mod window;

pub use body::{
    CfgBody, CfgFrameCenter, CfgInterior, CfgInteriorGrid1D, CfgMesh, CfgMeshKind, CfgMeshSource,
    CfgOrbitKepler, CfgOrbitSpeedControl, CfgState, CfgTemperatureInit,
};
pub use camera::CfgCamera;
pub use config::{read_cfg, Cfg, CfgError};
pub use preferences::CfgPreferences;
pub use simu::{CfgRoutines, CfgSimulation, CfgTimeExport};
pub use sun::CfgSun;
pub use window::{CfgColormap, CfgScalar, CfgWindow};
