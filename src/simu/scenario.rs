use crate::{
    check_if_latest_version,
    config::{self, CfgRoutines, Config, InteriorGrid1D},
    path_cfg_folder, read_surface_main, thermal_skin_depth_one, thermal_skin_depth_two_pi,
    util::*,
    AirlessBody, BodyData, Export, FoldersRun, FrameEvent, Result, Routines,
    RoutinesThermalDefault, RoutinesViewerDefault, Time, Window, KEY_BACKWARD, KEY_FORWARD,
    KEY_LEFT, KEY_RIGHT, SENSITIVITY,
};

use chrono::Utc;
use config::SkinDepth;
use itertools::Itertools;
use sdl2::keyboard::Keycode;

pub struct Scenario {
    pub config: Config,
    pub bodies: Vec<AirlessBody>,
    pub time: Time,
    pub win: Window,
    pub folders: FoldersRun,
    pub routines: Box<dyn Routines>,
    pub pre_computed_bodies: Vec<BodyData>,
}

impl Scenario {
    // pub fn new() -> Result<Self> {
    //     let path_exe = env::current_exe().unwrap();
    //     let path = path_exe.parent().unwrap();
    //     Self::new_with(path)
    // }

    pub fn new(config: Config) -> Result<Self> {
        let path_cfg = path_cfg_folder();

        println!(
            "kalast<{}> (built on {} with rustc<{}>)",
            version(),
            DATETIME,
            RUSTC_VERSION
        );

        println!(
            "Config initialized at {}",
            dunce::canonicalize(&path_cfg).unwrap().to_str().unwrap()
        );

        if let Some(true) = config.preferences.debug.config {
            println!("{:#?}", config);
        }

        if let Some(false) = config.preferences.do_not_check_latest_version {
            check_if_latest_version(&config);
        }

        let mut folders = FoldersRun::new(&config);
        // folders.save_cfgs(&path_cfg);
        folders.save_cfgs("cfg");

        // folders.save_src(&path_mainrs);

        let bodies = vec![];
        let pre_computed_bodies = vec![];

        let routines = match &config.simulation.routines {
            CfgRoutines::Viewer => Box::new(RoutinesViewerDefault::new()) as Box<dyn Routines>,
            CfgRoutines::Thermal => Box::new(RoutinesThermalDefault::new()) as Box<dyn Routines>,
        };

        #[cfg(feature = "spice")]
        {
            if let Some(path) = &config.spice.kernel {
                spice::kclear();
                if let Some(true) = config.preferences.debug.general {
                    println!("SPICE: Cleared kernel pool.");
                }

                let path_str = path.to_str().unwrap();
                spice::furnsh(path_str);
                if let Some(true) = config.preferences.debug.general {
                    println!("SPICE: Loaded kernel {}.", path_str);
                }
            }
        }

        dbg!(&config.window.shadows);

        let win = Window::with_settings(|s| {
            s.width = config.window.width;
            s.height = config.window.height;
            s.background_color = config.window.background;
            if config.window.high_dpi {
                s.high_dpi();
            }
            s.colormap = config.window.colormap.name;
            s.shadows = config.window.shadows;
            s.ambient_light_color = config.window.ambient;
            s.wireframe = config.window.wireframe;
            s.draw_normals = config.window.normals;
            s.normals_magnitude = config.window.normals_length;
            s.debug = config.preferences.debug.window.unwrap_or_default();
            s.sensitivity = config.preferences.sensitivity.unwrap_or(SENSITIVITY);
            s.forward = config
                .preferences
                .keys
                .forward
                .as_ref()
                .and_then(|s| Keycode::from_name(&s))
                .unwrap_or(KEY_FORWARD);
            s.left = config
                .preferences
                .keys
                .left
                .as_ref()
                .and_then(|s| Keycode::from_name(&s))
                .unwrap_or(KEY_LEFT);
            s.backward = config
                .preferences
                .keys
                .backward
                .as_ref()
                .and_then(|s| Keycode::from_name(&s))
                .unwrap_or(KEY_BACKWARD);
            s.right = config
                .preferences
                .keys
                .right
                .as_ref()
                .and_then(|s| Keycode::from_name(&s))
                .unwrap_or(KEY_RIGHT);
            s.touchpad_controls = config.preferences.touchpad_controls.unwrap_or_default();
        });

        let time_start = match config.simulation.start.seconds() {
            Ok(seconds) => seconds,
            Err(e) => panic!("{e} Spice is required to convert the starting date of the simulation to ephemeris time."),
        } as usize;

        let time = Time::new()
            .with_time_step(config.simulation.step)
            .with_time_start(time_start);

        Ok(Self {
            config,
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
        for (_ii, cb) in self.config.bodies.iter().enumerate() {
            let surface = read_surface_main(cb)?;
            let asteroid = AirlessBody::new(surface);

            let asteroid = match &cb.interior {
                None => asteroid,
                Some(interior) => match interior {
                    config::Interior::Grid1D(grid) => match grid {
                        InteriorGrid1D::Linear { size, a } => {
                            asteroid.with_interior_grid_fn_linear(*size, *a)
                        }
                        InteriorGrid1D::Pow { size, a, n } => {
                            asteroid.with_interior_grid_fn_pow(*size, *a, *n)
                        }
                        InteriorGrid1D::Exp { size, a } => {
                            asteroid.with_interior_grid_fn_exp(*size, *a)
                        }
                        InteriorGrid1D::File { path } => {
                            asteroid.with_interior_grid_from_file(path)
                        }
                        InteriorGrid1D::Increasing { skin, m, n, b } => {
                            let diffusivity =
                                asteroid.surface.faces[0].vertex.material.diffusivity();
                            let period = cb.spin.period;
                            let zs = match skin {
                                SkinDepth::One => thermal_skin_depth_one(diffusivity, period),
                                SkinDepth::TwoPi => thermal_skin_depth_two_pi(diffusivity, period),
                            };
                            let zmax = zs * *b as Float;

                            let mut z = vec![0.0];
                            let mut dz = zs / *m as Float;
                            let mut ii = 0;
                            while z[ii] < zmax {
                                dz = dz * (1.0 + 1.0 / *n as Float);
                                z.push(z[ii] + dz);
                                ii += 1;
                            }
                            asteroid.with_interior_grid_depth(z)
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
        let mut export = Export::new(&self.config.simulation.export);

        self.routines
            .init(&self.config, &mut self.bodies, &self.time, &mut self.win);

        for body in 0..self.bodies.len() {
            self.routines.fn_export_body_once(
                &self.config,
                body,
                &mut self.bodies,
                &self.pre_computed_bodies,
                &self.folders,
            )
        }

        'main_loop: loop {
            // Register keyboard and mouse interactions.
            let event = self.win.events();

            match event {
                FrameEvent::Exit => break 'main_loop,
                _ => (),
            };

            if self.win.is_paused() {
                self.routines
                    .fn_render(&self.config, &mut self.bodies, &self.time, &mut self.win);
                continue;
            }

            if !export.is_first_it {
                self.time.next_iteration();
            }

            let it = self.time.iteration();
            let elapsed = self.time.elapsed_seconds();
            let jd = self.time.jd();

            self.routines
                .fn_update_file_index(&self.config, &mut self.time);

            self.routines.fn_update_scene(
                &self.config,
                &self.time,
                &mut self.win.scene.borrow_mut(),
            );

            for body in 0..self.bodies.len() {
                self.routines.fn_update_body(
                    &self.config,
                    body,
                    &mut self.bodies,
                    &mut self.pre_computed_bodies,
                    &self.time,
                    &self.win,
                );
            }

            self.routines
                .fn_render(&self.config, &mut self.bodies, &self.time, &mut self.win);

            export.iteration(
                &self.config,
                &mut self.bodies,
                &self.pre_computed_bodies,
                &mut self.time,
                &self.win,
                &self.folders,
                self.routines.as_ref(),
            );

            self.routines.fn_iteration_finish(
                &self.config,
                &mut self.bodies,
                &self.time,
                &self.win,
            );

            if elapsed > self.config.simulation.duration {
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
