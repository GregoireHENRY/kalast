use crate::prelude::*;

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
    pub fn new(cfg: &Cfg) -> Self {
        let path = find_new_name_run(&cfg.pref.runs).unwrap();

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

    pub fn save_cfgs(&mut self, cfg: &Cfg) {
        fs::create_dir_all(&self.path).unwrap();
        fs_extra::dir::copy(&cfg.path, &self.path, &fs_extra::dir::CopyOptions::new()).unwrap();
    }
    pub fn save_src<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref();

        let path_mainrs = path.join("main.rs");

        if !path_mainrs.exists() {
            return;
        }

        fs::create_dir_all(&self.path).unwrap();
        fs::copy(path.join("main.rs"), self.path.join("main.rs")).unwrap();
    }
}
