use crate::{
    check_if_latest_version,
    config::{self, CfgRoutines, Config, InteriorGrid1D},
    read_surface_low, read_surface_main, thermal_skin_depth_one, thermal_skin_depth_two_pi,
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
    pub bodies_data: Vec<BodyData>,
    pub time: Time,
    pub win: Option<Window>,
    pub folders: FoldersRun,
    pub routines: Box<dyn Routines>,
    pub sun: Vec3,
}

impl Scenario {
    // pub fn new() -> Result<Self> {
    //     let path_exe = env::current_exe().unwrap();
    //     let path = path_exe.parent().unwrap();
    //     Self::new_with(path)
    // }

    pub fn new(mut config: Config) -> Result<Self> {
        if let Some(true) = config.preferences.debug.config {
            println!("{:#?}", config);
        }

        if let Some(false) = config.preferences.do_not_check_latest_version {
            check_if_latest_version(&config);
        }

        let mut folders = FoldersRun::from_cfg(&config);
        // folders.save_cfgs(&path_cfg);
        folders.save_cfg(&config);

        // folders.save_src(&path_mainrs);

        let bodies = vec![];
        let bodies_data = vec![];

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

        let mut win = None;

        if !config.preferences.no_window.unwrap_or_default() {
            win = Some(Window::with_settings(|s| {
                s.width = config.window.width;
                s.height = config.window.height;
                s.background_color = config.window.background;
                if config.window.high_dpi {
                    s.high_dpi();
                }
                if let Some(cmap) = config.window.colormap.as_ref() {
                    s.colormap = cmap.name.unwrap_or(crate::Colormap::default());
                }
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
            }));
        }

        let time_start = {
            let mut time_start = match config.simulation.start.seconds() {
                Ok(seconds) => seconds,
                Err(e) => panic!("{e} Spice is required to convert the starting date of the simulation to ephemeris time."),
            };

            time_start = (time_start as isize + config.simulation.start_offset) as usize;

            time_start
        };

        if let Some(restart) = config.restart.as_ref() {
            config.simulation.step = (config.simulation.step as Float
                * restart.time_step_factor.unwrap_or(1.0))
                as usize;
        }

        let mut time = Time::new()
            .with_time_step(config.simulation.step)
            .with_time_start(time_start);

        time.elapsed_time = config.simulation.elapsed;

        Ok(Self {
            config,
            bodies,
            bodies_data,
            time,
            win,
            folders,
            routines,
            sun: Vec3::zeros(),
        })
    }

    pub fn change_routines<R: Routines>(&mut self, routines: R) {
        self.routines = Box::new(routines);
    }

    pub fn load_bodies(&mut self) -> Result<()> {
        for (_ii, cb) in self.config.bodies.iter().enumerate() {
            let surface = read_surface_main(cb)?;
            let surface_lowres = read_surface_low(cb)?;
            let asteroid = AirlessBody::new(surface).with_lowres(surface_lowres);

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
            self.bodies_data.push(BodyData::new(&asteroid, &cb));
            self.bodies.push(asteroid);
        }

        if let Some(win) = self.win.as_mut() {
            win.load_surfaces(self.bodies.iter().map(|b| &b.surface));
        }

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

        'main_loop: loop {
            if let Some(win) = self.win.as_mut() {
                let event = win.events();

                match event {
                    FrameEvent::Exit => break 'main_loop,
                    _ => (),
                };

                if win.is_paused() {
                    self.routines.fn_render(
                        &self.config,
                        &mut self.bodies,
                        &mut self.bodies_data,
                        win,
                        &self.time,
                        &export,
                    );
                    continue;
                }
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
                &mut self.sun,
                &self.time,
                self.win.as_mut(),
                &export,
            );

            for body in 0..self.bodies.len() {
                self.routines.fn_update_body(
                    &self.config,
                    body,
                    &mut self.bodies,
                    &mut self.bodies_data,
                    &self.sun,
                    &self.time,
                    self.win.as_ref(),
                );
            }

            if let Some(win) = self.win.as_mut() {
                self.routines.fn_render(
                    &self.config,
                    &mut self.bodies,
                    &mut self.bodies_data,
                    win,
                    &self.time,
                    &export,
                );
            }

            // is_first_it turned to false at the end of this routine after the first iteration.
            export.iteration(
                &self.config,
                &mut self.bodies,
                &self.bodies_data,
                &self.sun,
                &mut self.time,
                self.win.as_ref(),
                &self.folders,
                self.routines.as_ref(),
            );

            self.routines.fn_iteration_finish(
                &self.config,
                &mut self.bodies,
                &mut self.time,
                self.win.as_ref(),
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

                if let Some(win) = self.win.as_ref() {
                    if paused_stop {
                        paused_stop = false;
                        win.toggle_pause();
                        continue;
                    }
                }

                break 'main_loop;
            }
        }

        Ok(())
    }
}
