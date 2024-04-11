use crate::{
    config::{CfgTimeExport, Config},
    util::*,
    AirlessBody, BodyData, FoldersRun, Interior, Routines, Time, Window,
};

use itertools::Itertools;
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
            cooldown_export: cfg.cooldown_start.unwrap_or_default() as _,
        }
    }

    fn init_body(
        &mut self,
        config: &Config,
        body: usize,
        bodies: &[AirlessBody],
        _bodies_data: &[BodyData],
        folders: &FoldersRun,
    ) {
        if config.bodies[body].record.mesh {
            let faces = bodies[body].surface.faces.clone();
            let sph = faces.iter().map(|f| f.vertex.sph()).collect_vec();
            let mut df = df!(
                "x" => faces.iter().map(|f| f.vertex.position.x).collect_vec(),
                "y" => faces.iter().map(|f| f.vertex.position.y).collect_vec(),
                "z" => faces.iter().map(|f| f.vertex.position.z).collect_vec(),
                "lon" => sph.iter().map(|sph| sph[1]).collect_vec(),
                "lat" => sph.iter().map(|sph| sph[2]).collect_vec(),
                "rad" => sph.iter().map(|sph| sph[0]).collect_vec(),
            )
            .unwrap();

            let folder_simu = folders.simu_body(&config.bodies[body].name);
            fs::create_dir_all(&folder_simu).unwrap();

            let mut file = std::fs::File::options()
                .append(true)
                .create(true)
                .open(folder_simu.join("mesh.csv"))
                .unwrap();
            CsvWriter::new(&mut file)
                .include_header(true)
                .finish(&mut df)
                .unwrap();
        }

        if config.bodies[body].record.depth {
            let mut depth = None;

            if let Some(interior) = bodies[body].interior.as_ref() {
                match interior {
                    Interior::Grid(grid) => {
                        depth = Some(grid.depth.clone());
                    }
                }
            }

            if let Some(depth) = depth {
                let mut df = df!(
                    "depth" => depth,
                )
                .unwrap();

                let folder_simu = folders.simu_body(&config.bodies[body].name);
                fs::create_dir_all(&folder_simu).unwrap();

                let mut file = std::fs::File::options()
                    .append(true)
                    .create(true)
                    .open(folder_simu.join("depth.csv"))
                    .unwrap();
                CsvWriter::new(&mut file)
                    .include_header(true)
                    .finish(&mut df)
                    .unwrap();
            }
        }
    }

    pub fn iteration(
        &mut self,
        config: &Config,
        bodies: &mut [AirlessBody],
        bodies_data: &[BodyData],
        sun: &Vec3,
        time: &mut Time,
        win: Option<&Window>,
        folders: &FoldersRun,
        routines: &dyn Routines,
    ) {
        let dt = time.used_time_step();
        let elapsed = time.elapsed_seconds();

        if !folders.path.exists() {
            fs::create_dir_all(&folders.path).unwrap();
        }

        if self.is_first_it {
            for body in 0..config.bodies.len() {
                self.init_body(config, body, bodies, bodies_data, folders);
            }
        }

        for body in 0..config.bodies.len() {
            routines.fn_export_iteration(body, config, time, folders, self.is_first_it);
        }

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
                self.remaining_duration_export = config.simulation.export.duration as _;
                println!("Start export time.");
                // println!("Simulation time step: {}", time.time_step);
                time.set_time_step(config.simulation.export.step);
                // println!("Export time step: {}", time.time_step);
            } else if self.cooldown_export - (dt as i64) < 0 {
                // So export does not really start here, but the time step is adapted to not miss the beginning of export
                // (in case export time step is smaller than simulation time step).
                println!("Start pre-export time.");
                // println!("Simulation time step: {}", time.time_step);
                time.set_time_step(config.simulation.export.step);
                // println!("Export time step: {}", time.time_step);
            }
        }

        if self.exporting {
            if !self.is_first_it_export {
                self.remaining_duration_export -= dt as i64;
            }

            // print!(" remaining duration export({})..", self.remaining_duration_export);

            if config.window.export_frames {
                let path = folders
                    .simu_rec_time_frames(self.exporting_started_elapsed as _)
                    .join(format!("{}.png", elapsed));

                if let Some(win) = win {
                    win.export_frame(path);
                }
            }

            for body in 0..config.bodies.len() {
                if self.is_first_it_export {
                    self.iteration_body_export_start_generic(
                        config,
                        body,
                        bodies,
                        bodies_data,
                        sun,
                        time,
                        folders,
                    );
                }
                routines.fn_export_iteration_period(
                    body,
                    bodies,
                    config,
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
                    (config.simulation.export.period - config.simulation.export.duration) as _;
                println!("End of export.");
                // println!("Export time step: {}", time.time_step);
                time.set_time_step(config.simulation.step);
                // println!("Simulation time step: {}", time.time_step);

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
        sun: &Vec3,
        time: &Time,
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
            "sunpos" => sun.as_slice(),
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
