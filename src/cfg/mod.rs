/*!
# Configuration

## To configure your scenarios.

A scenario configuration is a `cfg/` folder containing the configuration files using [yaml][url-yaml] format.
You can use an existing configuration from [the examples][examples] or write your own config.

The configuration can be done in a single file or split in different files. 

### Using a single file `cfg.yaml`

The name of the file must be `cfg.yaml`, inside the config folder (i.e., `cfg/cfg.yaml`).
The config can be customised with the [optional fields of the main `Cfg` structure][Cfg#fields].

### Splitting the config into multiple files

Each field of the main [`Cfg`] structure can written in a separate file. It leads to the following folder
strcture:

- `cfg/bodies/ *.yaml`: an additional folder called `bodies` in the config folder with as many config files inside
as you want. Each file will be a new body, given that they are defined with unique name, using the [`CfgBody`
configuration][CfgBody].
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
mod scene;
mod config;
mod preferences;
mod simu;
mod window;

pub use body::*;
pub use scene::*;
pub use config::*;
pub use preferences::*;
pub use simu::*;
pub use window::*;