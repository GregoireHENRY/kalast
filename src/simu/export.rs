use crate::{
    config::CfgTimeExport, config::Config, util::*, AirlessBody, BodyData, FoldersRun, Routines,
    Time, Window, WindowScene,
};

use polars::prelude::{df, CsvWriter, NamedFrom, SerWriter};
use std::fs;

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
        cfg: &Config,
        bodies: &mut [AirlessBody],
        body_data: &[BodyData],
        time: &mut Time,
        win: &Window,
        folders: &FoldersRun,
        routines: &dyn Routines,
    ) {
        let dt = time.used_time_step();
        let elapsed = time.elapsed_seconds();

        if !folders.path.exists() {
            fs::create_dir_all(&folders.path).unwrap();
        }

        for body in 0..cfg.bodies.len() {
            if body > 0 {
                // print!(" ");
            }
            // let np_elapsed = elapsed as Float / cb.spin.period;
            // print!("{:8.4}", np_elapsed);

            routines.fn_export_iteration(body, cfg, time, folders, self.is_first_it);
        }
        // print!(">");

        if !self.exporting {
            if !self.is_first_it {
                self.cooldown_export -= dt as i64;
            }
            // print!(" cooldown export({})..", self.cooldown_export);

            // if self.cooldown_export <= 0 && dt > 0 {
            if self.cooldown_export <= 0 {
                // print!(" began exporting..");
                self.exporting = true;
                self.exporting_started_elapsed = elapsed as _;
                self.remaining_duration_export = cfg.simulation.export.duration as _;
                println!("Detected export time.");
                println!("Simulation time step: {}", time.time_step);
                time.set_time_step(cfg.simulation.export.step);
                println!("Export time step: {}", time.time_step);
            } else if self.cooldown_export - (dt as i64) < 0 {
                // So export does not really start here, but the time step is adapted to not miss the beginning of export
                // (in case export time step is smaller than simulation time step).
                println!("Detected pre-export time.");
                println!("Simulation time step: {}", time.time_step);
                time.set_time_step(cfg.simulation.export.step);
                println!("Export time step: {}", time.time_step);
            }
        }

        if self.exporting {
            if !self.is_first_it_export {
                self.remaining_duration_export -= dt as i64;
            }

            // print!(" remaining duration export({})..", self.remaining_duration_export);

            if cfg.window.export_frames {
                let path = folders
                    .simu_rec_time_frames(self.exporting_started_elapsed as _)
                    .join(format!("{}.png", elapsed));
                win.export_frame(path);
            }

            for body in 0..cfg.bodies.len() {
                if self.is_first_it_export {
                    self.iteration_body_export_start_generic(
                        cfg,
                        body,
                        bodies,
                        body_data,
                        time,
                        &win.scene.borrow(),
                        folders,
                    );
                }
                routines.fn_export_iteration_period(
                    body,
                    bodies,
                    cfg,
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
                self.cooldown_export =
                    (cfg.simulation.export.period - cfg.simulation.export.duration) as _;
                println!("End of export.");
                println!("Export time step: {}", time.time_step);
                time.set_time_step(cfg.simulation.step);
                println!("Simulation time step: {}", time.time_step);

                // let _cvg = kalast::simu::converge::check_all(&mut bodies, &folder_tpm, &cfg.time.export);
            }
        }

        if self.is_first_it {
            self.is_first_it = false;
        }
    }

    pub fn iteration_body_export_start_generic(
        &self,
        config: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        body_data: &[BodyData],
        time: &Time,
        scene: &WindowScene,
        folders: &FoldersRun,
    ) {
        let elapsed = time.elapsed_seconds();
        let np_elapsed = time.elapsed_seconds() as Float / config.bodies[body].spin.period;
        let jd = time.jd();

        let folder_state = folders.simu_rec_time_body_state(
            self.exporting_started_elapsed as _,
            &config.bodies[body].name,
        );
        let folder_img = folders.simu_rec_time_frames(self.exporting_started_elapsed as _);
        fs::create_dir_all(&folder_state).unwrap();
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
            "sunpos" => scene.light.position.as_slice(),
        )
        .unwrap();
        let mut file = std::fs::File::options()
            .append(true)
            .create(true)
            .open(folder_state.join("vectors.csv"))
            .unwrap();
        CsvWriter::new(&mut file).finish(&mut df).unwrap();

        let mut df = df!(
            "translation" => body_data[body].translation.as_slice(),
            "orientation" => body_data[body].orientation.as_slice(),
            "model" => bodies[body].matrix_model.as_slice(),
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
