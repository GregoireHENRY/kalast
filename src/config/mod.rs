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
mod restart;
mod scene;
mod simulation;
mod spice;
mod window;

use std::{collections::HashMap, io, path::PathBuf};

pub use body::*;
pub use preferences::*;
pub use restart::*;
pub use scene::*;
pub use simulation::*;
pub use spice::*;
pub use window::*;

use crate::util::*;

use figment::value::Value;
use snafu::prelude::*;

use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};

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

    pub restart: Option<Restart>,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

impl Config {
    pub fn index_body(&self, name: &str) -> Option<usize> {
        self.bodies.iter().position(|body| body.name == name)
    }

    pub fn maybe_restarting(self) -> Self {
        if let Some(restart) = self.restart.clone() {
            let new = self;

            // Config can be restarted from a recorded run.
            let mut builder = Figment::from(Serialized::defaults(Config::default()));

            let path = restart.path.as_ref().unwrap().join("cfg/cfg.toml");
            if path.exists() {
                builder = builder.merge(Toml::file(path));
            } else {
                panic!("Config for restart not found.")
            }

            let mut config: Config = builder.extract().unwrap();

            // The cfg of the recorded run become the cfg but we apply and save restart settings to it.
            //
            // Depth restart settings:
            // Depth rescaling cannot be applied directly here as an interpolation is needed when
            // loading cfg0 temperature and material parameters over depth. We keep cfg0 depth settings
            // and apply restart settings later.
            // Depth TODO

            // Restart parameters that are already in main config without restart.

            for body in 0..new.bodies.len() {
                if let Some(new_record) = new.bodies[body].record.clone() {
                    if let Some(record) = config.bodies[body].record.as_mut() {
                        if let Some(faces) = new_record.faces {
                            record.faces = Some(faces);
                        }
                        if let Some(columns) = new_record.columns {
                            record.columns = Some(columns);
                        }
                        if let Some(rows) = new_record.rows {
                            record.rows = Some(rows);
                        }
                        if let Some(cells) = new_record.cells {
                            record.cells = Some(cells);
                        }
                        if let Some(mesh) = new_record.mesh {
                            record.mesh = Some(mesh);
                        }
                        if let Some(depth) = new_record.depth {
                            record.depth = Some(depth);
                        }
                    } else {
                        config.bodies[body].record = Some(new_record);
                    }
                }

                if let Some(new_material) = new.bodies[body].material.clone() {
                    if let Some(material) = config.bodies[body].material.as_mut() {
                        material.albedo = new_material.albedo;
                        material.emissivity = new_material.emissivity;
                        material.thermal_inertia = new_material.thermal_inertia;
                        material.density = new_material.density;
                        material.heat_capacity = new_material.heat_capacity;

                        // if let Some(emissivity) = new_material.emissivity {
                        //     material.emissivity = Some(emissivity);
                        // }
                    } else {
                        config.bodies[body].material = Some(new_material);
                    }
                }
            }

            if let Some(elapsed) = new.simulation.elapsed {
                config.simulation.elapsed = Some(elapsed);
            }

            if let Some(step) = new.simulation.step {
                config.simulation.step = Some(step);
            }

            if let Some(duration) = new.simulation.duration {
                config.simulation.duration = Some(duration);
            }

            if let Some(pause) = new.simulation.pause_first_it {
                config.simulation.pause_first_it = Some(pause);
            }

            if let Some(shadow) = new.simulation.self_shadowing {
                config.simulation.self_shadowing = Some(shadow);
            }

            if let Some(shadow) = new.simulation.mutual_shadowing {
                config.simulation.mutual_shadowing = Some(shadow);
            }

            if let Some(heating) = new.simulation.self_heating {
                config.simulation.self_heating = Some(heating);
            }

            if let Some(heating) = new.simulation.mutual_heating {
                config.simulation.mutual_heating = Some(heating);
            }

            if let Some(new_export) = new.simulation.export {
                if let Some(export) = config.simulation.export.as_mut() {
                    if let Some(step) = new_export.step {
                        export.step = Some(step);
                    }
                    if let Some(duration) = new_export.duration {
                        export.duration = Some(duration);
                    }
                    if let Some(period) = new_export.period {
                        export.period = Some(period);
                    }
                    if let Some(cooldown) = new_export.cooldown_start {
                        export.cooldown_start = Some(cooldown);
                    }
                } else {
                    config.simulation.export = Some(new_export);
                }
            }

            if let Some(p) = new.scene.camera.position.clone() {
                config.scene.camera.position = Some(p);
            }

            if let Some(projection) = new.scene.camera.projection {
                config.scene.camera.projection = Some(projection);
            }

            if let Some(near) = new.scene.camera.near {
                config.scene.camera.near = Some(near);
            }

            if let Some(far) = new.scene.camera.far {
                config.scene.camera.far = Some(far);
            }

            if let Some(width) = new.window.width {
                config.window.width = Some(width);
            }

            if let Some(height) = new.window.height {
                config.window.height = Some(height);
            }

            if let Some(fullscreen) = new.window.fullscreen {
                config.window.fullscreen = Some(fullscreen);
            }

            if let Some(background) = new.window.background {
                config.window.background = Some(background);
            }

            if let Some(high_dpi) = new.window.high_dpi {
                config.window.high_dpi = Some(high_dpi);
            }

            if let Some(shadow_dpi) = new.window.shadow_dpi {
                config.window.shadow_dpi = Some(shadow_dpi);
            }

            if let Some(shadows) = new.window.shadows {
                config.window.shadows = Some(shadows);
            }

            if let Some(ambient) = new.window.ambient {
                config.window.ambient = Some(ambient);
            }

            if let Some(wireframe) = new.window.wireframe {
                config.window.wireframe = Some(wireframe);
            }

            if let Some(normals) = new.window.normals {
                config.window.normals = Some(normals);
            }

            if let Some(normals_length) = new.window.normals_length {
                config.window.normals_length = Some(normals_length);
            }

            if let Some(export_frames) = new.window.export_frames {
                config.window.export_frames = Some(export_frames);
            }

            if let Some(color_selection) = new.window.color_selection {
                config.window.color_selection = Some(color_selection);
            }

            if let Some(new_cmap) = new.window.colormap {
                if let Some(cmap) = config.window.colormap.as_mut() {
                    if let Some(name) = new_cmap.name {
                        cmap.name = Some(name);
                    }
                    if let Some(vmin) = new_cmap.vmin {
                        cmap.vmin = Some(vmin);
                    }
                    if let Some(vmax) = new_cmap.vmax {
                        cmap.vmax = Some(vmax);
                    }
                    if let Some(scalar) = new_cmap.scalar {
                        cmap.scalar = Some(scalar);
                    }
                    if let Some(reverse) = new_cmap.reverse {
                        cmap.reverse = Some(reverse);
                    }
                } else {
                    config.window.colormap = Some(new_cmap);
                }
            }

            // Restart parameters to be applied at the end.

            if let Some(duration_more) = restart.duration_more {
                if let Some(duration) = config.simulation.duration.as_mut() {
                    *duration += duration_more;
                }
            }

            if let Some(factor) = restart.time_step_factor {
                if let Some(step) = config.simulation.step.as_mut() {
                    *step = (*step as Float * factor) as usize;
                }
            }

            if let Some(factor) = restart.time_step_export_factor {
                if let Some(export) = config.simulation.export.as_mut() {
                    if let Some(step) = export.step.as_mut() {
                        *step = (*step as Float * factor) as usize;
                    }
                }
            }

            config.restart = Some(restart);
            config.preferences = new.preferences.clone();

            config
        } else {
            self
        }
    }
}
