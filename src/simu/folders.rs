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
    pub fn new(config: &Config) -> Self {
        let path = config.preferences.path_runs.clone().unwrap_or({
            let dirs = UserDirs::new().unwrap();
            dirs.desktop_dir().unwrap().join("kalast-runs")
        });

        let path = find_new_name_run(path).unwrap();

        Self { path }
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

    pub fn simu_rec_time_body(&self, elapsed: usize, id: &str) -> PathBuf {
        self.simu_rec_time(elapsed).join(format!("{}", id))
    }

    pub fn simu_rec_time_body_state(&self, elapsed: usize, id: &str) -> PathBuf {
        self.simu_rec_time_body(elapsed, id).join("state")
    }

    pub fn simu_rec_time_body_temperatures(&self, elapsed: usize, id: &str) -> PathBuf {
        self.simu_rec_time_body(elapsed, id).join("temperatures")
    }

    pub fn save_cfgs<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if !path.exists() {
            return;
        }

        fs::create_dir_all(&self.path).unwrap();
        fs_extra::dir::copy(path, &self.path, &fs_extra::dir::CopyOptions::new()).unwrap();
    }
    pub fn save_src<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();
        if !path.exists() {
            return;
        }

        fs::create_dir_all(&self.path).unwrap();
        fs::copy(path, &self.path).unwrap();
    }
}
