use directories::UserDirs;

use crate::config::Config;

use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn find_new_name_run<P: AsRef<Path>>(path_runs: P) -> Option<PathBuf> {
    let path_runs = path_runs.as_ref();

    for ii in 1.. {
        let path = path_runs.join(format!("run #{}", ii));
        if !path.exists() {
            return Some(path);
        }
    }

    None
}

pub struct FoldersRun {
    pub path: PathBuf,
}

impl FoldersRun {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn from_cfg(config: &Config) -> Self {
        let path = config.preferences.path_runs.clone().unwrap_or({
            let dirs = UserDirs::new().unwrap();
            dirs.desktop_dir().unwrap().join("kalast-runs")
        });

        let path = find_new_name_run(path).unwrap();
        Self::new(path)
    }

    pub fn cfg(&self) -> PathBuf {
        self.path.join("cfg")
    }

    pub fn simu(&self) -> PathBuf {
        self.path.join("simu")
    }

    pub fn simu_rec(&self) -> PathBuf {
        self.simu().join("rec")
    }

    pub fn simu_body(&self, id: &str) -> PathBuf {
        self.simu().join(format!("{}", id))
    }

    pub fn simu_rec_time(&self, elapsed: usize) -> PathBuf {
        self.simu_rec().join(format!("{}", elapsed))
    }

    pub fn simu_rec_time_frames(&self, elapsed: usize) -> PathBuf {
        self.simu_rec_time(elapsed).join("frames")
    }

    pub fn simu_rec_time_body(&self, elapsed: usize, name: &str) -> PathBuf {
        self.simu_rec_time(elapsed).join(format!("{}", name))
    }

    pub fn simu_rec_time_body_state(&self, elapsed: usize, id: &str) -> PathBuf {
        self.simu_rec_time_body(elapsed, id).join("state")
    }

    pub fn simu_rec_time_body_temperatures(&self, elapsed: usize, id: &str) -> PathBuf {
        self.simu_rec_time_body(elapsed, id).join("temperatures")
    }

    pub fn save_cfg(&mut self, config: &Config) {
        let new_cfg_folder = self.cfg();
        let new_cfg_file = new_cfg_folder.join("cfg.toml");
        fs::create_dir_all(&new_cfg_folder).unwrap();
        fs_extra::file::copy(
            "cfg/cfg.toml",
            &new_cfg_file,
            &fs_extra::file::CopyOptions::new(),
        )
        .unwrap();

        let cfg_str = toml::to_string(config).expect("Could not encode config value for export.");
        fs::write(new_cfg_folder.join("full.toml"), cfg_str).expect("Could not write to file.");
    }
    pub fn save_src<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if !path.exists() {
            return;
        }

        // fs::create_dir_all(&self.path).unwrap();
        // fs::copy(path, &self.path).unwrap();
    }
}
