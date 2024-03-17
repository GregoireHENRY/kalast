/*!
# Configuration

## To configure your scenarios.

A scenario configuration is a `cfg/` folder containing the configuration files using [yaml][url-yaml] format.
You can use an existing configuration from [the examples][examples] or write your own configuration.
The `cfg/` folder must be located next to the executable.

The configuration can be a single file or split in different files.

### Using a single file `cfg.yaml`

The name of the file must be `cfg.yaml`, inside the configuration folder (i.e., `cfg/cfg.yaml`).
The configuration can be customised with the [optional fields of the main `Cfg` structure][Cfg#fields].

### Split the configuration into multiple files

Each field of the main [`Cfg`] structure can written in a separate file. It leads to the following folder
strcture:

- `cfg/bodies/ *.yaml`: an additional folder called `bodies` in the configuration folder with as many files inside as
  you want for the configuration of the bodies.
  Each file will be a new body, given that they are defined with unique name, using the
  [`CfgBody` configuration][CfgBody].
- `cfg/scene.yaml`: a file using the [`CfgScene` configuration][CfgScene].
- `cfg/simulation.yaml`: a file using the [`CfgSimulation` configuration][CfgSimulation].
- `cfg/window.yaml`: a file using the [`CfgWindow` configuration][CfgWindow]
- `cfg/preferences.yaml`: a file using the [`CfgPreferences` configuration][CfgPreferences]

## General Preferences

If you install an executable of **kalast**, you can see that a file `preferences.yaml` is also shipped, next to
the executable. This serves as general preferences across your different configs. Local preferences will overwrite
general preferences.

## Also

If you try to run **kalast** without config (i.e., no `cfg/` folder with no config inside), the default config
will be used, defaulting each field of each structure.

Variant of Enums for options are in CamelCase but corresponding values in yaml config files are snake_case.
For example, see [`ColorMode`][crate::ColorMode].

## What Next?

Now you should read the documentation of the different config structures: [`Cfg`], [`CfgBody`], [`CfgScene`],
[`CfgSimulation`], [`CfgWindow`], [`CfgPreferences`].

[url-yaml]: https://yaml.org
[examples]: https://github.com/GregoireHENRY/kalast/tree/main/examples
*/

mod body;
mod preferences;
mod scene;
mod simulation;
mod spice;
mod window;

use std::{collections::HashMap, io, path::PathBuf};

pub use body::*;
pub use preferences::*;
pub use scene::*;
pub use simulation::*;
pub use spice::*;
pub use window::*;

use figment::value::Value;
use snafu::prelude::*;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum Error {
    FileNotFound {
        source: io::Error,
        path: PathBuf,
    },

    // CfgReading {
    //     source: serde_yaml::Error,
    //     path: PathBuf,
    // },
    // ParsingEquatorial {
    //     source: Error,
    //     location: Location,
    // },
    #[snafu(display("Feature `spice` is not enabled."))]
    FeatureSpiceNotEnabled {},
}

use serde::{Deserialize, Serialize};

/// # Configuration
///
/// For the moment, no high-level documentation of `Cfg`.
/// Read [the existing examples][examples] and adapt them with the definition of `Cfg`.
///
/// You can read [`CfgBody`] for preliminary documentation of the configuration for the bodies.
///
/// [examples]: https://github.com/GregoireHENRY/kalast/tree/main/examples
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    pub preferences: Preferences,
    pub bodies: Vec<Body>,
    pub scene: CfgScene,
    pub simulation: CfgSimulation,

    #[cfg(feature = "spice")]
    pub spice: CfgSpice,

    pub window: CfgWindow,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl Config {
    pub fn index_body(&self, name: &str) -> Option<usize> {
        self.bodies.iter().position(|body| body.name == name)
    }
}
