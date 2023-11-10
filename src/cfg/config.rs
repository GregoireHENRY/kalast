use crate::prelude::*;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_yaml::{self, Value};

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
    path_bodies_dir(p)
        .read_dir()
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect_vec()
}

pub trait Configuration: Serialize + DeserializeOwned {}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Cfg {
    #[serde(skip)]
    pub path: PathBuf,

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
    pub fn new_empty<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            ..Default::default()
        }
    }

    pub fn new_from<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let cfg_pref_path = path_pref(path);
        let cfg_win_path = path_win(path);
        let cfg_simu_path = path_simu(path);
        let cfg_sun_path = path_sun(path);
        let cfg_cam_path = path_cam(path);

        let pref: CfgPreferences = read_cfg(&cfg_pref_path)?;
        let win: CfgWindow = read_cfg(&cfg_win_path)?;
        let simu: CfgSimulation = read_cfg(&cfg_simu_path)?;
        let sun: CfgSun = read_cfg(&cfg_sun_path)?;
        let cam: CfgCamera = read_cfg(&cfg_cam_path)?;

        let cfg_bodies_path_iter = path_bodies(path);
        let bodies: Vec<CfgBody> = cfg_bodies_path_iter
            .iter()
            .map(|p| {
                let mut body: CfgBody = read_cfg(&p).unwrap();

                if body.id == "!empty" {
                    body.id = p.file_stem().unwrap().to_str().unwrap().to_string();
                }

                body
            })
            .collect_vec();

        /*
        dbg!(cfg_window);
        dbg!(cfg_simu);
        dbg!(cfg_sun);
        dbg!(cfg_camera);
        dbg!(cfg_body);
        dbg!(cfg_moon);
        */

        Ok(Self {
            path: path.to_path_buf(),
            pref,
            win,
            simu,
            sun,
            cam,
            bodies,
        })
    }

    pub fn new() -> Result<Self> {
        let file = Path::new(file!());
        let parent = file.parent().unwrap();
        Self::new_from(parent)
    }

    pub fn path_pref(&self) -> PathBuf {
        path_pref(&self.path)
    }

    pub fn path_win(&self) -> PathBuf {
        path_win(&self.path)
    }

    pub fn path_simu(&self) -> PathBuf {
        path_simu(&self.path)
    }

    pub fn path_sun(&self) -> PathBuf {
        path_sun(&self.path)
    }

    pub fn path_cam(&self) -> PathBuf {
        path_cam(&self.path)
    }

    pub fn path_bodies(&self) -> Vec<PathBuf> {
        path_bodies(&self.path)
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.path_win(),
            self.path_simu(),
            self.path_sun(),
            self.path_cam(),
        ];
        paths.extend(self.path_bodies());
        paths
    }
}
