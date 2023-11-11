use crate::prelude::*;

pub struct Export {
    pub is_first_it: bool,
    pub is_first_it_export: bool,
    pub exporting: bool,
    pub exporting_started_elapsed: i64,
    pub remaining_duration_export: i64,
    pub cooldown_export: i64,
}

impl Export {
    pub fn new(cfg: &CfgTimeExport) -> Self {
        Self {
            is_first_it: true,
            is_first_it_export: true,
            exporting: false,
            exporting_started_elapsed: 0,
            remaining_duration_export: 0,
            cooldown_export: cfg.cooldown_start.unwrap_or(cfg.period) as _,
        }
    }

    pub fn iteration(
        &mut self,
        time: &mut Time,
        folders: &FoldersRun,
        cfg: &Cfg,
        bodies: &[Body],
        routines: &dyn Routines,
        scene: &Scene,
        win: &Window,
    ) {
        let dt = time.used_time_step();
        let elapsed = time.elapsed_seconds();

        for (ii_body, cb) in cfg.bodies.iter().enumerate() {
            if ii_body > 0 {
                // print!(" ");
            }
            // let np_elapsed = elapsed as Float / cb.spin.period;
            // print!("{:8.4}", np_elapsed);

            routines.fn_export_iteration(cb, ii_body, time, folders, self.is_first_it);
        }
        // print!(">");

        if !self.exporting {
            if !self.is_first_it {
                self.cooldown_export -= dt as i64;
            }
            // print!(" cooldown export({})..", self.cooldown_export);

            if self.cooldown_export <= 0 && dt > 0 {
                // print!(" began exporting..");
                self.exporting = true;
                self.exporting_started_elapsed = elapsed as _;
                self.remaining_duration_export = cfg.simu.export.duration as _;
                time.set_time_step(cfg.simu.export.step);
            } else if self.cooldown_export - (dt as i64) < 0 {
                // So export does not really start here, but the time step is adapted to not miss the beginning of export
                // (in case export time step is smaller than simulation time step).
                time.set_time_step(cfg.simu.export.step);
            }
        }

        if self.exporting {
            if !self.is_first_it_export {
                self.remaining_duration_export -= dt as i64;
            }

            // print!(" remaining duration export({})..", self.remaining_duration_export);

            if cfg.win.export_frames {
                let path = folders
                    .simu_rec_time_frames(self.exporting_started_elapsed as _)
                    .join(format!("{}.png", elapsed));
                win.export_frame(path);
            }

            for (ii_body, (body, cb)) in izip!(bodies, &cfg.bodies).enumerate() {
                if self.is_first_it_export {
                    self.iteration_body_export_start_generic(cb, body, time, folders, scene);
                }
                routines.fn_export_iteration_period(
                    cb,
                    body,
                    ii_body,
                    folders,
                    self.exporting_started_elapsed,
                    self.is_first_it_export,
                );
            }

            if self.is_first_it_export {
                self.is_first_it_export = false;
            }

            if self.remaining_duration_export <= 0 {
                // print!(" finished exporting..");
                self.exporting = false;
                self.is_first_it_export = true;
                self.cooldown_export = (cfg.simu.export.period - cfg.simu.export.duration) as _;
                time.set_time_step(cfg.simu.step);

                // let _cvg = kalast::simu::converge::check_all(&mut bodies, &folder_tpm, &cfg.time.export);
            }
        }

        if self.is_first_it {
            self.is_first_it = false;
        }
    }

    pub fn iteration_body_export_start_generic(
        &self,
        cb: &CfgBody,
        body: &Body,
        time: &Time,
        folders: &FoldersRun,
        scene: &Scene,
    ) {
        let elapsed = time.elapsed_seconds();
        let np_elapsed = time.elapsed_seconds() as Float / cb.spin.period;
        let jd = time.jd();

        let folder_state =
            folders.simu_rec_time_body_state(self.exporting_started_elapsed as _, &cb.id);
        let folder_tpm =
            folders.simu_rec_time_body_temperatures(self.exporting_started_elapsed as _, &cb.id);
        let folder_img = folders.simu_rec_time_frames(self.exporting_started_elapsed as _);
        fs::create_dir_all(&folder_state).unwrap();
        fs::create_dir_all(&folder_tpm).unwrap();
        fs::create_dir_all(&folder_img).unwrap();

        let mut df = df!(
            "elapsed" => [elapsed as i64],
            "jd" => [jd as i64],
            "np_elapsed" => [np_elapsed],
            // "spinang" => [body_info.spin],
        )
        .unwrap();
        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(folder_state.join("scalars.csv"))
            .unwrap();
        CsvWriter::new(&mut file).finish(&mut df).unwrap();

        let mut df = df!(
            "sunpos" => scene.sun_pos.as_slice(),
        )
        .unwrap();
        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(folder_state.join("vectors.csv"))
            .unwrap();
        CsvWriter::new(&mut file).finish(&mut df).unwrap();

        let mut df = df!(
            "spinrot" => body.mat_orient.as_slice(),
        )
        .unwrap();
        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(folder_state.join("matrices.csv"))
            .unwrap();
        CsvWriter::new(&mut file).finish(&mut df).unwrap();
    }
}
