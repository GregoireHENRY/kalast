use crate::prelude::*;

pub struct Scenario<B: Body, R: Routines> {
    pub cfg: Cfg,
    pub folders: FoldersRun,
    pub bodies: Vec<B>,
    pub win: Window,
    pub time: Time,
    pub scene: Scene,
    pub routines: R,
}

impl Scenario<BodyDefault, RoutinesViewerDefault> {
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let cfg = Cfg::new_from(&path.join("cfg"))?;

        let mut folders = FoldersRun::new(&cfg);
        folders.save_cfgs(&cfg);
        folders.save_src(&path);

        let bodies: Vec<BodyDefault> = vec![];
        let routines = simu::routines_viewer_default();

        let sun_pos = cfg.sun.position * AU;
        let cam_pos = match cfg.cam {
            CfgCamera::Position(p) => p,
            CfgCamera::SunDirection(d) => sun_pos.normalize() * d,
        };
        let scene = Scene { sun_pos, cam_pos };

        let win = Window::with_settings(|s| {
            s.width = cfg.win.width;
            s.height = cfg.win.height;
            if cfg.win.high_dpi {
                s.high_dpi();
            }
            s.colormap = cfg.win.colormap.name;
            s.shadows = cfg.win.shadows;
            s.ortho = cfg.win.orthographic;
            s.camera_speed = cfg.win.camera_speed;
            s.ambient_light_color = cfg.win.ambient;
            s.wireframe = cfg.win.wireframe;
            s.draw_normals = cfg.win.normals;
            s.normals_magnitude = cfg.win.normals_length;
        })
        .with_camera_position(&scene.cam_pos)
        .with_light_position(&scene.sun_pos_cubelight());

        let time_start = cfg.simu.jd0 * DAY as Float;
        let time = Time::new()
            .with_time_step(cfg.simu.step)
            .with_time_start(time_start as _);

        Ok(Self {
            cfg,
            folders,
            bodies,
            win,
            time,
            scene,
            routines,
        })
    }
}

impl<B, R> Scenario<B, R>
where
    B: Body,
    R: Routines,
{
    pub fn select_routines<R2>(self, routines: R2) -> Scenario<B, R2>
    where
        R2: Routines,
    {
        Scenario::<B, R2> {
            cfg: self.cfg,
            folders: self.folders,
            bodies: vec![],
            win: self.win,
            time: self.time,
            scene: self.scene,
            routines,
        }
    }

    pub fn select_body_type<B2>(self) -> Scenario<B2, R>
    where
        B2: Body,
    {
        Scenario::<B2, R> {
            cfg: self.cfg,
            folders: self.folders,
            bodies: vec![],
            win: self.win,
            time: self.time,
            scene: self.scene,
            routines: self.routines,
        }
    }

    pub fn load_bodies(&mut self) -> Result<()> {
        for (_ii, cb) in self.cfg.bodies.iter().enumerate() {
            let surface = simu::read_surface_main(cb)?;
            let asteroid = Asteroid::new(surface);

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

            // self.data.push(ThermalData::new(&asteroid, &cb, &self.scene));
            self.bodies
                .push(self.routines.fn_setup_body(asteroid, cb, &self.scene));
        }

        self.win
            .load_surfaces(self.bodies.iter().map(|b| &b.asteroid().surface));

        Ok(())
    }

    pub fn bodies_permutations_indices(&self) -> itertools::Permutations<std::ops::Range<usize>> {
        (0..self.bodies.len()).permutations(self.bodies.len())
    }

    pub fn iterations(&mut self) -> Result<()> {
        self.iterations_with_fns(R::fn_iteration_body, R::fn_end_of_iteration)
    }

    pub fn iterations_with_fn_body<F>(&mut self, fn_body: F) -> Result<()>
    where
        F: Fn(&mut R, usize, &[usize], &CfgBody, &[&CfgBody], &mut [B], &Scene, &Time),
    {
        self.iterations_with_fns(fn_body, R::fn_end_of_iteration)
    }

    pub fn iterations_with_fn_end<F>(&mut self, fn_end: F) -> Result<()>
    where
        F: Fn(&mut R, &mut [B], &Time, &Scene, &Window),
    {
        self.iterations_with_fns(R::fn_iteration_body, fn_end)
    }

    pub fn iterations_with_fns<F1, F2>(&mut self, fn_body: F1, fn_end: F2) -> Result<()>
    where
        F1: Fn(&mut R, usize, &[usize], &CfgBody, &[&CfgBody], &mut [B], &Scene, &Time),
        F2: Fn(&mut R, &mut [B], &Time, &Scene, &Window),
    {
        if self.bodies.is_empty() {
            self.load_bodies()?;
        }

        let mut paused_stop = true;
        let mut export = Export::new(&self.cfg.simu.export);

        'main_loop: loop {
            let event = self.win.events();

            self.scene.cam_pos = self.win.camera_position();

            match event {
                FrameEvent::Exit => break 'main_loop,
                _ => (),
            };

            if self.win.is_paused() {
                self.win
                    .render_asteroids(&self.bodies.iter().map(|b| b.asteroid()).collect_vec());
                self.win.swap_window();
                continue;
            }

            if !export.is_first_it {
                self.time.next_iteration();
            }

            let it = self.time.iteration();
            let elapsed = self.time.elapsed_seconds();
            let jd = self.time.jd();

            for (indices_bodies, cbs_permut) in izip!(
                self.bodies_permutations_indices(),
                self.cfg
                    .bodies
                    .clone()
                    .iter()
                    .permutations(self.bodies.len())
            ) {
                if indices_bodies.is_empty() {
                    break;
                }

                let (&ii_body, ii_other_bodies) = indices_bodies.split_first().unwrap();
                let (cb, other_cbs) = cbs_permut.split_first().unwrap();

                let other_bodies = ii_other_bodies
                    .iter()
                    .map(|&ii| &self.bodies[ii])
                    .collect_vec();

                let mat_orient_ref = simu::find_reference_matrix_orientation(cb, &other_bodies);

                self.routines.fn_update_matrix_model(
                    &mut self.bodies[ii_body],
                    cb,
                    &other_cbs,
                    &self.time,
                    &mat_orient_ref,
                );

                fn_body(
                    &mut self.routines,
                    ii_body,
                    ii_other_bodies,
                    cb,
                    other_cbs,
                    &mut self.bodies,
                    &self.scene,
                    &self.time,
                );

                self.routines.fn_update_colormap(
                    &self.win,
                    &self.cfg.win.colormap,
                    ii_body,
                    &mut self.bodies[ii_body],
                    &self.scene,
                );
            }

            self.win.set_light_direction(&self.scene.sun_dir());

            // self.win.update_vaos(self.bodies.iter_mut().map(|b| &mut b.asteroid_mut().surface));
            self.win
                .render_asteroids(&self.bodies.iter().map(|b| b.asteroid()).collect_vec());
            self.win.swap_window();

            export.iteration(
                &mut self.time,
                &self.folders,
                &self.cfg,
                &self.bodies,
                &self.routines,
                &self.scene,
                &self.win,
            );

            fn_end(
                &mut self.routines,
                &mut self.bodies,
                &self.time,
                &self.scene,
                &self.win,
            );

            if elapsed > self.cfg.simu.duration {
                let time_calc = Utc::now().time() - *self.time.real_time();
                println!(
                    "\nSimulation finished at JD: {}.\nComputation time: {:.3}s ({}it).",
                    jd,
                    time_calc.num_milliseconds() as f64 * 1e-3,
                    it
                );

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
