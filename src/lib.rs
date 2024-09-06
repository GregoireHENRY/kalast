/*!
# Thermophysical Model for Binary Asteroids

Latest executables of **kalast** [can be downloaded here][releases].

**kalast** can also be used as a library in Rust.
Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
kalast = "0.4"
```

and have a look at [the examples][examples] or [the different modules here][crate#modules].

Information on the configuration of your **kalast** scenarios are located at the documentation of
[the module `cfg`][mod@cfg].

[examples]: https://github.com/GregoireHENRY/kalast/tree/main/examples
[releases]: https://github.com/GregoireHENRY/kalast/releases
*/

pub mod body;
pub mod config;
pub mod gui;
pub mod simu;
pub mod thermal;
pub mod util;
pub mod win;

pub use body::*;
pub use gui::*;
pub use simu::*;
pub use thermal::*;
pub use util::*;
pub use win::*;

use config as cg;

#[cfg(feature = "spice")]
pub use spice;

use snafu::prelude::*;
use std::{env, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Snafu)]
pub enum Error {
    ConfigError { source: cg::Error },
    SurfaceError { source: SurfaceError },
}

impl From<cg::Error> for Error {
    fn from(value: cg::Error) -> Self {
        Self::ConfigError { source: value }
    }
}

impl From<SurfaceError> for Error {
    fn from(value: SurfaceError) -> Self {
        Self::SurfaceError { source: value }
    }
}

pub fn path_cfg_folder() -> PathBuf {
    env::current_exe().unwrap().parent().unwrap().join("cfg")
}
