use crate::{
    check_if_latest_version, read_surface_main, util::*, AirlessBody, BodyData, Cfg, CfgInterior,
    CfgInteriorGrid1D, CfgRoutines, Export, FoldersRun, FrameEvent, Result, Routines,
    RoutinesThermalDefault, RoutinesViewerDefault, Time, Window,
};

use chrono::Utc;
use itertools::Itertools;
use std::{env, path::Path};

pub struct Scenario {
    pub cfg: Cfg,
    pub bodies: Vec<AirlessBody>,
    pub time: Time,
    pub win: Window,
    pub folders: FoldersRun,
    pub routines: Box<dyn Routines>,
    pub pre_computed_bodies: Vec<BodyData>,
}

impl Scenario {
    pub fn new() -> Result<Self> {
        let path_exe = env::current_exe().unwrap();
        let path = path_exe.parent().unwrap();
        Self::new_with(path)
    }

    pub fn new_with<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let path_cfg = path.join("cfg");
        let path_mainrs = path.join("main.rs");

        println!(
            "kalast<{}> (built on {} with rustc<{}>)",
            version(),
            DATETIME,
            RUSTC_VERSION
        );

        let cfg = Cfg::new_from(&path_cfg)?;

        println!(
            "Config initialized at {}",
            dunce::canonicalize(&path).unwrap().to_str().unwrap()
        );

        if !cfg.preferences.do_not_check_latest_version {
            check_if_latest_version(&cfg);
        }

        let mut folders = FoldersRun::new(&cfg);
        folders.save_cfgs(&path_cfg);
        folders.save_src(&path_mainrs);

        let bodies = vec![];
        let pre_computed_bodies = vec![];

        let routines = match &cfg.simulation.routines {
            CfgRoutines::Viewer => Box::new(RoutinesViewerDefault::new()) as Box<dyn Routines>,
            CfgRoutines::Thermal => Box::new(RoutinesThermalDefault::new()) as Box<dyn Routines>,
        };

        #[cfg(feature = "spice")]
        {
            if let Some(path) = &cfg.spice.kernel {
                spice::kclear();
                if cfg.preferences.debug {
                    println!("SPICE: Cleared kernel pool.");
                }

                let path_str = path.to_str().unwrap();
                spice::furnsh(path_str);
                if cfg.preferences.debug {
                    println!("SPICE: Loaded kernel {}.", path_str);
                }
            }
        }

        let win = Window::with_settings(|s| {
            s.width = cfg.window.width;
            s.height = cfg.window.height;
            s.background_color = cfg.window.background;
            if cfg.window.high_dpi {
                s.high_dpi();
            }
            s.colormap = cfg.window.colormap.name;
            s.shadows = cfg.window.shadows;
            s.ambient_light_color = cfg.window.ambient;
            s.wireframe = cfg.window.wireframe;
            s.draw_normals = cfg.window.normals;
            s.normals_magnitude = cfg.window.normals_length;
            s.debug = cfg.preferences.debug;
        });

        let time_start = match cfg.simulation.start.seconds() {
            Ok(seconds) => seconds,
            Err(e) => panic!("{e} Spice is required to convert the starting date of the simulation to ephemeris time."),
        } as usize;

        let time = Time::new()
            .with_time_step(cfg.simulation.step)
            .with_time_start(time_start);

        Ok(Self {
            cfg,
            bodies,
            time,
            win,
            folders,
            routines,
            pre_computed_bodies,
        })
    }

    pub fn change_routines<R: Routines>(&mut self, routines: R) {
        self.routines = Box::new(routines);
    }

    pub fn load_bodies(&mut self) -> Result<()> {
        for (_ii, cb) in self.cfg.bodies.iter().enumerate() {
            let surface = read_surface_main(cb)?;
            let asteroid = AirlessBody::new(surface);

            let asteroid = match &cb.interior {
                None => asteroid,
                Some(interior) => match interior {
                    CfgInterior::Grid1D(grid) => match grid {
                        CfgInteriorGrid1D::Linear { size, a } => {
                            asteroid.with_interior_grid_fn_linear(*size, *a)
                        }
                        CfgInteriorGrid1D::Pow { size, a, n } => {
                            asteroid.with_interior_grid_fn_pow(*size, *a, *n)
                        }
                        CfgInteriorGrid1D::Exp { size, a } => {
                            asteroid.with_interior_grid_fn_exp(*size, *a)
                        }
                        CfgInteriorGrid1D::File { path } => {
                            asteroid.with_interior_grid_from_file(path)
                        }
                    },
                },
            };

            self.routines.load(&asteroid, &cb);
            self.pre_computed_bodies.push(BodyData::new(&asteroid, &cb));
            self.bodies.push(asteroid);
        }

        self.win
            .load_surfaces(self.bodies.iter().map(|b| &b.surface));

        Ok(())
    }

    pub fn bodies_permutations_indices(&self) -> itertools::Permutations<std::ops::Range<usize>> {
        (0..self.bodies.len()).permutations(self.bodies.len())
    }

    pub fn iterations(&mut self) -> Result<()> {
        if self.bodies.is_empty() {
            self.load_bodies()?;
        }

        let mut paused_stop = true;
        let mut export = Export::new(&self.cfg.simulation.export);

        self.routines
            .init(&self.cfg, &mut self.bodies, &self.time, &mut self.win);

        'main_loop: loop {
            // Register keyboard and mouse interactions.
            let event = self.win.events();

            match event {
                FrameEvent::Exit => break 'main_loop,
                _ => (),
            };

            if self.win.is_paused() {
                self.routines
                    .fn_render(&self.cfg, &mut self.bodies, &self.time, &mut self.win);
                continue;
            }

            if !export.is_first_it {
                self.time.next_iteration();
            }

            let it = self.time.iteration();
            let elapsed = self.time.elapsed_seconds();
            let jd = self.time.jd();

            self.routines
                .fn_update_scene(&self.cfg, &self.time, &mut self.win.scene.borrow_mut());

            for body in 0..self.bodies.len() {
                self.routines.fn_update_body(
                    &self.cfg,
                    body,
                    &mut self.bodies,
                    &mut self.pre_computed_bodies,
                    &self.time,
                    &self.win,
                );
            }

            self.routines
                .fn_render(&self.cfg, &mut self.bodies, &self.time, &mut self.win);

            export.iteration(
                &self.cfg,
                &mut self.bodies,
                &self.pre_computed_bodies,
                &mut self.time,
                &self.win,
                &self.folders,
                self.routines.as_ref(),
            );

            self.routines
                .fn_iteration_finish(&self.cfg, &mut self.bodies, &self.time, &self.win);

            if elapsed > self.cfg.simulation.duration {
                let time_calc = Utc::now().time() - *self.time.real_time();
                println!(
                    "\nSimulation finished at JD: {}.\nComputation time: {:.3}s ({}it).",
                    jd,
                    time_calc.num_milliseconds() as f64 * 1e-3,
                    it
                );

                #[cfg(feature = "spice")]
                spice::kclear();

                if paused_stop {
                    paused_stop = false;
                    self.win.toggle_pause();
                    continue;
                }

                break 'main_loop;
            }
        }

        Ok(())
    }
}
