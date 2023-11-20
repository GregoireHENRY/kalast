use crate::prelude::*;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::{self, Value};

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

pub type Result<T, E = CfgError> = std::result::Result<T, E>;

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
}

pub fn read_cfg<P, C>(path: P) -> Result<C>
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

pub fn read_cfg_if_exists<P, C>(path: P) -> Option<Result<C>>
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

pub fn path_pref<P: AsRef<Path>>(_p: P) -> PathBuf {
    Path::new("./preferences.yaml").to_path_buf()
}

pub fn path_win<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("window.yaml")
}

pub fn path_simu<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("simulation.yaml")
}

pub fn path_sun<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("sun.yaml")
}

pub fn path_cam<P: AsRef<Path>>(p: P) -> PathBuf {
    p.as_ref().join("camera.yaml")
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Cfg {
    #[serde(default)]
    pub base: Option<CfgBase>,

    #[serde(default)]
    pub pref: CfgPreferences,

    #[serde(default)]
    pub win: CfgWindow,

    #[serde(default)]
    pub simu: CfgSimulation,

    #[serde(default)]
    pub sun: CfgSun,

    #[serde(default)]
    pub cam: CfgCamera,

    #[serde(default)]
    pub bodies: Vec<CfgBody>,

    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

impl Configuration for Cfg {}

impl Cfg {
    pub fn new_from<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let cfg_path = path_cfg(path);

        let mut cfg = if let Some(Ok(cfg)) = read_cfg_if_exists::<_, Cfg>(&cfg_path) {
            // TODO: finish base argument
            if let Some(base) = cfg.base.as_ref() {
                let _base_cfg = match base {
                    CfgBase::Cfg(base) => base.load(),
                    CfgBase::Path(_p) => unimplemented!(), //Self::new_from(p)?
                };
            }

            cfg
        } else {
            Self::default()
        };

        let cfg_pref_path = path_pref(path);
        let cfg_win_path = path_win(path);
        let cfg_simu_path = path_simu(path);
        let cfg_sun_path = path_sun(path);
        let cfg_cam_path = path_cam(path);
        let cfg_bodies_paths = path_bodies(path);

        if let Some(Ok(pref)) = read_cfg_if_exists(&cfg_pref_path) {
            cfg.pref = pref;
        }

        if let Some(Ok(win)) = read_cfg_if_exists(&cfg_win_path) {
            cfg.win = win;
        }

        if let Some(Ok(simu)) = read_cfg_if_exists(&cfg_simu_path) {
            cfg.simu = simu;
        }

        if let Some(Ok(sun)) = read_cfg_if_exists(&cfg_sun_path) {
            cfg.sun = sun;
        }

        if let Some(Ok(cam)) = read_cfg_if_exists(&cfg_cam_path) {
            cfg.cam = cam;
        }

        for p in &cfg_bodies_paths {
            if let Some(Ok(mut body)) = read_cfg_if_exists::<_, CfgBody>(p) {
                if body.id == "!empty" {
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

        Ok(cfg)
    }

    pub fn new() -> Result<Self> {
        let file = Path::new(file!());
        let parent = file.parent().unwrap();
        Self::new_from(parent)
    }
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
