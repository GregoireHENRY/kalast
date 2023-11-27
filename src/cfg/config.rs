use crate::{CfgBody, CfgBodyError, CfgPreferences, CfgScene, CfgSimulation, CfgWindow};

use itertools::Itertools;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::{self, Value};
use snafu::{prelude::*, Location};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

const ID_RESERVED: usize = std::usize::MAX;
pub const CFG_THERMAL_STR: &str = include_str!("../../examples/thermal/cfg/cfg.yaml");
pub const CFG_THERMAL_BINARY_STR: &str = include_str!("../../examples/thermal-binary/cfg/cfg.yaml");
pub const CFG_THERMAL_TRIANGLE_STR: &str =
    include_str!("../../examples/thermal-triangle/cfg/cfg.yaml");
pub const CFG_VIEWER_STR: &str = include_str!("../../examples/viewer/cfg/cfg.yaml");
pub const CFG_VIEWER_PICKER_STR: &str = include_str!("../../examples/viewer-picker/cfg/cfg.yaml");
pub const CFG_VIEWER_SMOOTH_STR: &str = include_str!("../../examples/viewer-smooth/cfg/cfg.yaml");

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Cfgs {
    #[serde(rename = "empty")]
    Empty,

    #[serde(rename = "thermal")]
    Thermal,

    #[serde(rename = "thermal_binary")]
    ThermalBinary,

    #[serde(rename = "thermal_triangle")]
    ThermalTriangle,

    #[serde(rename = "viewer")]
    Viewer,

    #[serde(rename = "viewer_picker")]
    ViewerPicker,

    #[serde(rename = "viewer_smooth")]
    ViewerSmooth,
}

impl Cfgs {
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Empty => "",
            Self::Thermal => CFG_THERMAL_STR,
            Self::ThermalBinary => CFG_THERMAL_BINARY_STR,
            Self::ThermalTriangle => CFG_THERMAL_TRIANGLE_STR,
            Self::Viewer => CFG_VIEWER_STR,
            Self::ViewerPicker => CFG_VIEWER_PICKER_STR,
            Self::ViewerSmooth => CFG_VIEWER_SMOOTH_STR,
        }
    }

    pub fn load(&self) -> Cfg {
        serde_yaml::from_str(self.as_str()).unwrap()
    }
}

pub fn cfg_viewer() -> Cfg {
    serde_yaml::from_str(CFG_VIEWER_STR).unwrap()
}

pub type CfgResult<T, E = CfgError> = std::result::Result<T, E>;

/// Errors related to Kalast config.
#[derive(Debug, Snafu)]
pub enum CfgError {
    CfgFileNotFound {
        source: io::Error,
        path: PathBuf,
    },
    CfgReading {
        source: serde_yaml::Error,
        path: PathBuf,
    },
    CfgParsingEquatorial {
        source: CfgBodyError,
        location: Location,
    },
}

pub fn read_cfg<P, C>(path: P) -> CfgResult<C>
where
    P: AsRef<Path>,
    C: Configuration,
{
    let path = path.as_ref().to_path_buf();
    let file = std::fs::File::options()
        .read(true)
        .open(&path)
        .context(CfgFileNotFoundSnafu { path: &path })?;
    let cfg = serde_yaml::from_reader(file).context(CfgReadingSnafu { path: &path })?;
    Ok(cfg)
}

pub fn read_cfg_if_exists<P, C>(path: P) -> Option<CfgResult<C>>
where
    P: AsRef<Path>,
    C: Configuration,
{
    let path = path.as_ref();
    path.exists().then(|| read_cfg(path))
}

pub fn path_cfg<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("cfg.yaml")
}

pub fn path_pref_global() -> PathBuf {
    Path::new("./preferences.yaml").to_path_buf()
}

pub fn path_pref_local<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("preferences.yaml")
}

pub fn path_window<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("window.yaml")
}

pub fn path_simulation<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("simulation.yaml")
}

pub fn path_scene<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("scene.yaml")
}

pub fn path_bodies_dir<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("bodies")
}

pub fn path_bodies<P: AsRef<Path>>(p: P) -> Vec<PathBuf> {
    match path_bodies_dir(p).read_dir() {
        Ok(dir) => dir.map(|e| e.unwrap().path()).collect_vec(),
        Err(_) => vec![],
    }
}

pub trait Configuration: Serialize + DeserializeOwned {}

/**
# Configuration

For the moment, no high-level documentation of `Cfg`.
Read [the existing examples][examples] and adapt them with the definition of `Cfg`.

You can read [`CfgBody`] for preliminary documentation of the configuration for the bodies.

[examples]: https://github.com/GregoireHENRY/kalast/tree/main/examples
*/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cfg {
    #[serde(default)]
    base: Option<CfgBase>,

    #[serde(default)]
    pub bodies: Vec<CfgBody>,

    #[serde(default)]
    pub scene: CfgScene,

    #[serde(default)]
    pub simulation: CfgSimulation,

    #[serde(default)]
    pub window: CfgWindow,

    #[serde(default)]
    pub preferences: CfgPreferences,

    #[serde(flatten)]
    extra: HashMap<String, Value>,

    #[serde(default = "default_id_last")]
    id_last: usize,
}

impl Cfg {
    pub fn new_from<P: AsRef<Path>>(path: P) -> CfgResult<Self> {
        let path = path.as_ref();
        let cfg_path = path_cfg(path);

        let mut cfg = if let Some(cfg) = read_cfg_if_exists::<_, Cfg>(&cfg_path) {
            match cfg {
                Ok(mut cfg) => {
                    // TODO: finish base argument
                    if let Some(base) = cfg.base.as_ref() {
                        let _base_cfg = match base {
                            CfgBase::Cfg(base) => base.load(),
                            CfgBase::Path(_p) => unimplemented!(), //Self::new_from(p)?
                        };
                    }

                    // Check body IDs to make them unique from loading order if necessary.
                    for indices_bodies in (0..cfg.bodies.len()).permutations(cfg.bodies.len()) {
                        let (&ii_body, ii_other_bodies) = indices_bodies.split_first().unwrap();

                        if cfg.bodies[ii_body].id == super::body::default_body_id() {
                            if cfg.id_last == ID_RESERVED {
                                cfg.id_last = 0;
                            } else {
                                cfg.id_last += 1;
                            }

                            loop {
                                for &ii_other_body in ii_other_bodies {
                                    if cfg.id_last.to_string() == cfg.bodies[ii_other_body].id {
                                        cfg.id_last += 1;
                                        break;
                                    }
                                }

                                break;
                            }

                            cfg.bodies[ii_body].id = cfg.id_last.to_string();
                        }
                    }

                    cfg
                }
                Err(e) => panic!("{}", e),
            }
        } else {
            Self::default()
        };

        let cfg_pref_path_global = path_pref_global();
        let cfg_pref_path_local = path_pref_local(path);
        let cfg_win_path = path_window(path);
        let cfg_simu_path = path_simulation(path);
        let cfg_scene_path = path_scene(path);
        let cfg_bodies_paths = path_bodies(path);

        if let Some(Ok(pref)) = read_cfg_if_exists(&cfg_pref_path_global) {
            cfg.preferences = pref;
        }

        if let Some(Ok(pref)) = read_cfg_if_exists(&cfg_pref_path_local) {
            cfg.preferences = pref;
        }

        if let Some(Ok(win)) = read_cfg_if_exists(&cfg_win_path) {
            cfg.window = win;
        }

        if let Some(Ok(simu)) = read_cfg_if_exists(&cfg_simu_path) {
            cfg.simulation = simu;
        }

        if let Some(Ok(scene)) = read_cfg_if_exists(&cfg_scene_path) {
            cfg.scene = scene;
        }

        for p in &cfg_bodies_paths {
            if let Some(Ok(mut body)) = read_cfg_if_exists::<_, CfgBody>(p) {
                if body.id == super::body::default_body_id() {
                    body.id = p.file_stem().unwrap().to_str().unwrap().to_string();
                }

                for (ii, oth_body) in cfg.bodies.iter().enumerate() {
                    if oth_body.id == body.id {
                        cfg.bodies.remove(ii);
                        break;
                    }
                }
                cfg.bodies.push(body);
            }
        }

        let angle = cfg.scene.sun.as_equatorial().unwrap();
        dbg!(angle);

        Ok(cfg)
    }

    pub fn new() -> CfgResult<Self> {
        let file = Path::new(file!());
        let parent = file.parent().unwrap();
        Self::new_from(parent)
    }

    pub fn extra(&self) -> &HashMap<String, Value> {
        &self.extra
    }
}

impl Configuration for Cfg {}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            base: None,
            bodies: vec![],
            scene: CfgScene::default(),
            simulation: CfgSimulation::default(),
            window: CfgWindow::default(),
            preferences: CfgPreferences::default(),
            extra: HashMap::new(),
            id_last: ID_RESERVED,
        }
    }
}

fn default_id_last() -> usize {
    ID_RESERVED
}

// #[serde(untagged)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CfgBase {
    #[serde(rename = "cfg")]
    Cfg(Cfgs),

    #[serde(rename = "path")]
    Path(PathBuf),
}

impl Default for CfgBase {
    fn default() -> Self {
        Self::Cfg(Cfgs::Empty)
    }
}
