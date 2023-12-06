use crate::{
    check_if_latest_version, read_surface_main, simu::Scene, util::*, AirlessBody, BodyData, Cfg,
    CfgCamera, CfgInterior, CfgInteriorGrid1D, CfgRoutines, CfgSun, Export, FoldersRun, FrameEvent,
    Result, Routines, RoutinesThermalDefault, RoutinesViewerDefault, Time, Window,
};

use chrono::Utc;
use itertools::Itertools;
use std::{env, path::Path};

pub struct Scenario {
    pub cfg: Cfg,
    pub bodies: Vec<AirlessBody>,
    pub time: Time,
    pub scene: Scene,
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
            spice::kclear();

            if let Some(path) = &cfg.spice.kernel {
                spice::furnsh(path.to_str().unwrap());
            }
        }

        let scene = Scene {
            sun: CfgSun::default_position(),
            camera: CfgCamera::default_position(),
        };

        let win = Window::with_settings(|s| {
            s.width = cfg.window.width;
            s.height = cfg.window.height;
            s.background_color = cfg.window.background;
            if cfg.window.high_dpi {
                s.high_dpi();
            }
            s.colormap = cfg.window.colormap.name;
            s.shadows = cfg.window.shadows;
            s.ortho = cfg.window.orthographic;
            s.camera_speed = cfg.window.camera_speed;
            s.ambient_light_color = cfg.window.ambient;
            s.wireframe = cfg.window.wireframe;
            s.draw_normals = cfg.window.normals;
            s.normals_magnitude = cfg.window.normals_length;
        })
        .with_camera_position(&scene.camera)
        .with_light_position(&scene.sun_pos_cubelight());

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
            scene,
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

    /*
    pub fn iterations<R: Routines>(&mut self) -> Result<()> {
        self.iterations_with_fns(R::fn_iteration_body, R::fn_end_of_iteration)
    }

    pub fn iterations_with_fn_body<R: Routines, F>(&mut self, fn_body: F) -> Result<()>
    where
        F: Fn(&mut R, usize, &[usize], &CfgBody, &[&CfgBody], &mut [Body], &Scene, &Time),
    {
        self.iterations_with_fns(fn_body, R::fn_end_of_iteration)
    }

    pub fn iterations_with_fn_end<R: Routines, F>(&mut self, fn_end: F) -> Result<()>
    where
        F: Fn(&mut R, &mut [Body], &Time, &Scene, &Window),
    {
        self.iterations_with_fns(R::fn_iteration_body, fn_end)
    }

    pub fn iterations_with_fns<R: Routines, F1, F2>(&mut self, fn_body: F1, fn_end: F2) -> Result<()>
    where
        F1: Fn(&mut R, usize, &[usize], &CfgBody, &[&CfgBody], &mut [Body], &Scene, &Time),
        F2: Fn(&mut R, &mut [Body], &Time, &Scene, &Window),
    {
    */

    pub fn iterations(&mut self) -> Result<()> {
        if self.bodies.is_empty() {
            self.load_bodies()?;
        }

        let mut paused_stop = true;
        let mut export = Export::new(&self.cfg.simulation.export);

        'main_loop: loop {
            // Register keyboard and mouse interactions.
            let event = self.win.events();

            // Update camera data from keyboard movements.
            self.scene.camera = self.win.camera_position();

            match event {
                FrameEvent::Exit => break 'main_loop,
                _ => (),
            };

            if self.win.is_paused() {
                self.win.render_asteroids(&self.bodies);
                self.win.swap_window();
                continue;
            }

            if !export.is_first_it {
                self.time.next_iteration();
            }

            let it = self.time.iteration();
            let elapsed = self.time.elapsed_seconds();
            let jd = self.time.jd();

            self.scene = self
                .routines
                .fn_update_scene(&self.cfg, &self.time, &self.scene);

            for body in 0..self.bodies.len() {
                self.bodies[body].matrix_model = self.routines.fn_update_matrix_model(
                    &self.cfg,
                    body,
                    &mut self.pre_computed_bodies,
                    &self.time,
                    &mut self.scene,
                );
                self.routines.fn_iteration_body(
                    body,
                    &mut self.bodies,
                    &mut self.pre_computed_bodies,
                    &self.time,
                    &mut self.scene,
                );

                self.routines.fn_update_colormap(
                    body,
                    &mut self.bodies,
                    &self.pre_computed_bodies,
                    &self.cfg,
                    &self.scene,
                    &self.win,
                );
            }

            self.win.set_camera_position(&self.scene.camera);
            self.win.set_light_direction(&self.scene.sun_dir());

            // self.win.update_vaos(self.bodies.iter_mut().map(|b| &mut b.asteroid_mut().surface));
            self.win.render_asteroids(&self.bodies);
            self.win.swap_window();

            export.iteration(
                &self.cfg,
                &mut self.bodies,
                &self.pre_computed_bodies,
                &mut self.time,
                &self.scene,
                &self.win,
                &self.folders,
                self.routines.as_ref(),
            );

            self.routines.fn_end_of_iteration(
                &self.cfg,
                &mut self.bodies,
                &self.time,
                &self.scene,
                &self.win,
            );

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

            // println!();
        }

        Ok(())
    }
}
