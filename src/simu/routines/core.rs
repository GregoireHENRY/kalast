use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    find_ref_orbit, matrix_orientation_obliquity, matrix_spin, position_in_inertial_frame,
    update_colormap_scalar, util::*, AirlessBody, BodyData, Cfg, CfgBody, CfgCamera,
    CfgCameraDirection, CfgCameraPosition, CfgFrameCenter, CfgScalar, CfgState, CfgStateCartesian,
    CfgSun, CfgSunPosition, FoldersRun, MovementMode, Time, Window, WindowScene, DEFAULT_FRAME,
};

use downcast_rs::{impl_downcast, DowncastSync};
use itertools::{izip, Itertools};
use polars::prelude::*;

pub trait Routines: DowncastSync {
    fn load(&mut self, _body: &AirlessBody, _cb: &CfgBody) {}

    fn init(&mut self, _cfg: &Cfg, _bodies: &mut [AirlessBody], _time: &Time, _win: &mut Window) {}

    fn fn_update_scene_core(&self, cfg: &Cfg, time: &Time, scene: &mut WindowScene) {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        if cfg.preferences.debug {
            println!("Iteration: {}", time.iteration());
        }

        let mut sun = match &cfg.scene.sun.position {
            CfgSunPosition::Cartesian(p) => *p,
            CfgSunPosition::Equatorial(coords) => {
                coords.xyz_with_distance(coords.distance.unwrap_or(CfgSun::default_distance()))
            }
            CfgSunPosition::Spice => {
                #[cfg(not(feature = "spice"))]
                {
                    panic!("Feature `spice` is not enabled. The feature is required to compute the position of the Sun.")
                }

                #[cfg(feature = "spice")]
                {
                    if cfg.is_spice_loaded() {
                        if let Some(body) = cfg.bodies.first() {
                            let (position, _lt) = spice::spkpos(
                                "Sun",
                                elapsed_from_start as f64,
                                &cfg.spice
                                    .frame
                                    .as_ref()
                                    .cloned()
                                    .unwrap_or(DEFAULT_FRAME.to_string()),
                                "none",
                                &body.id,
                            );
                            Vec3::from_row_slice(&position)
                        } else {
                            panic!("A body must be loaded to compute the position of the Sun.")
                        }
                    } else {
                        panic!("Spice is not being used and is needed to compute the position of the Sun. Try loading a spice kernel to enable spice.")
                    }
                }
            }
            CfgSunPosition::FromBody => {
                if let Some(body) = cfg.bodies.first() {
                    match &body.state {
                        CfgState::Orbit(orbit) => match &orbit.frame {
                            CfgFrameCenter::Sun => -position_in_inertial_frame(
                                orbit.a * AU,
                                orbit.e,
                                orbit.i * RPD,
                                orbit.node * RPD,
                                orbit.peri * RPD,
                                elapsed_from_start as Float,
                                orbit.tp,
                                MU_SUN,
                            ),
                            CfgFrameCenter::Body(_) => {
                                if time.iteration() == 0 {
                                    println!("Warning: The Sun is set to be configured from the state of the primary body but only works if the state is an orbit centered on the Sun.");
                                }
                                CfgSun::default_position()
                            }
                        },
                        CfgState::Cartesian(_)
                        | CfgState::Equatorial(_)
                        | CfgState::File(_)
                        | CfgState::Spice(_) => {
                            if time.iteration() == 0 {
                                println!("Warning: The Sun is set to be configured from the state of the primary body but only works if the state is an orbit centered on the Sun.");
                            }

                            CfgSun::default_position()
                        }
                    }
                } else {
                    panic!("A body must be loaded to compute the position of the Sun.")
                }
            }
        };

        let camera = match &cfg.scene.camera.position {
            CfgCameraPosition::Cartesian(p) => *p,
            CfgCameraPosition::FromSun => {
                sun.normalize()
                    * cfg
                        .scene
                        .camera
                        .distance_origin
                        .unwrap_or(CfgCamera::default_distance())
            }
            CfgCameraPosition::Spice(_name) => {
                if let Some(_body) = cfg.bodies.first() {
                    #[cfg(not(feature = "spice"))]
                    {
                        panic!("Feature `spice` is not enabled. The feature is required to compute the position of the camera from Earth direction.")
                    }

                    #[cfg(feature = "spice")]
                    {
                        if cfg.is_spice_loaded() {
                            let (position, _lt) = spice::spkpos(
                                _name,
                                elapsed_from_start as f64,
                                &cfg.spice
                                    .frame
                                    .as_ref()
                                    .cloned()
                                    .unwrap_or(DEFAULT_FRAME.to_string()),
                                "none",
                                &_body.id,
                            );
                            let mut position = Vec3::from_row_slice(&position);

                            if let Some(distance) = cfg.scene.camera.distance_origin {
                                position = position.normalize() * distance;
                            }

                            position
                        } else {
                            panic!("Spice is not being used and is needed to compute the position of the camera from Earth direction. Try loading a spice kernel to enable spice.")
                        }
                    }
                } else {
                    panic!("A body must be loaded to compute the position of the camera from Earth direction. Visualisation is centered on body")
                }
            }
            CfgCameraPosition::Reference => {
                if let Some(body) = cfg.bodies.first() {
                    match &body.state {
                        CfgState::Equatorial(coords) => {
                            let position = -coords.xyz_with_distance(
                                coords.distance.unwrap_or(CfgCamera::default_distance()),
                            );
                            sun += position;
                            position
                        }
                        _ => panic!("Camera on reference mode only work with primary body state equatorial."),
                    }
                } else {
                    panic!("No body has been loaded to compute camera position.")
                }
            }
        };

        if cfg.preferences.debug {
            println!("camera: {:?}", camera.as_slice());
            println!("sun: {:?}", sun.as_slice());
        }

        scene.light.position = sun;
        scene.camera.position = camera;

        if scene.camera.movement_mode == MovementMode::Lock
            && cfg.scene.camera.direction == CfgCameraDirection::TargetAnchor
        {
            scene.camera.target_anchor();
        }

        if time.iteration == 0 {
            scene.camera.direction = match cfg.scene.camera.direction {
                CfgCameraDirection::Cartesian(v) => v,
                CfgCameraDirection::TargetAnchor => -scene.camera.position.normalize(),
            };
            scene.camera.up = cfg.scene.camera.up;
            scene.camera.up_world = cfg.scene.camera.up;
            scene.camera.projection = cfg.scene.camera.projection;

            if let Some(near) = cfg.scene.camera.near {
                scene.camera.near = Some(near);
            }

            if let Some(far) = cfg.scene.camera.far {
                scene.camera.far = Some(far);
            }
        }
    }

    fn fn_update_scene(&self, cfg: &Cfg, time: &Time, scene: &mut WindowScene) {
        self.fn_update_scene_core(cfg, time, scene);
    }

    fn fn_update_body_matrix_model(
        &self,
        cfg: &Cfg,
        body: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        time: &Time,
    ) -> Mat4 {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        let mut matrix_model_reference = Mat4::identity();
        let mut matrix_orientation = Mat4::identity();
        let mut matrix_translation = Mat4::identity();

        let elapsed = time.elapsed_seconds();
        let np_elapsed = if cfg.bodies[body].spin.period == 0.0 {
            0.0
        } else {
            elapsed as Float / cfg.bodies[body].spin.period
        };
        let spin_angle = (TAU * np_elapsed + cfg.bodies[body].spin.spin0 * RPD) % TAU;
        let matrix_spin = matrix_spin(spin_angle, cfg.bodies[body].spin.axis);

        let matrix_spin_tilt =
            matrix_orientation_obliquity(0.0, cfg.bodies[body].spin.obliquity * RPD);

        let other_bodies = cfg
            .bodies
            .iter()
            .enumerate()
            .filter(|(ii, _)| *ii != body)
            .map(|(_, cb)| cb)
            .collect_vec();

        match &cfg.bodies[body].state {
            CfgState::Cartesian(CfgStateCartesian {
                position,
                orientation,
                reference,
            }) => {
                matrix_translation = Mat4::new_translation(position);
                matrix_orientation *= glm::mat3_to_mat4(orientation);

                if let Some(reference) = reference {
                    let ref_id = cfg
                        .body_index(reference)
                        .expect(&format!("No body loaded with this id {}", reference));

                    // matrix_orientation = matrix_orientation * bodies_data[ref_id].orientation;

                    matrix_model_reference =
                        bodies_data[ref_id].translation * bodies_data[ref_id].orientation;
                }

                if cfg.preferences.debug {
                    println!("Body state with manual cartesian");
                    println!("position: {:?}", position.as_slice());
                    println!("rotation: {}", orientation);
                    println!("matrix model reference: {}", matrix_model_reference);
                }
            }
            CfgState::Equatorial(_) => {}
            CfgState::Orbit(orbit) => {
                let (mu_ref, factor) = find_ref_orbit(&orbit, &other_bodies);
                if mu_ref != MU_SUN {
                    let pos = position_in_inertial_frame(
                        orbit.a * factor,
                        orbit.e,
                        orbit.i * RPD,
                        orbit.node * RPD,
                        orbit.peri * RPD,
                        elapsed_from_start as Float,
                        orbit.tp,
                        mu_ref,
                    );
                    matrix_translation = Mat4::new_translation(&(pos * 1e-3));
                }

                match &orbit.frame {
                    CfgFrameCenter::Sun => {}
                    CfgFrameCenter::Body(id) => {
                        for (pre, cb) in izip!(bodies_data.iter_mut(), &cfg.bodies) {
                            if cb.id == *id {
                                matrix_model_reference = pre.orientation;
                                break;
                            }
                        }
                    }
                }
            }
            CfgState::File(path) => {
                // Each loop iteration is reading corresponding row index of state file.
                // So taking in consideration that kalast first iteration is initialization,
                // the first line of state can be repeated by user in second line if needed.
                let row_index = time.iteration();

                let df = match CsvReader::from_path(&path) {
                    Ok(reader) => reader.has_header(false).finish().unwrap(),
                    Err(e) => panic!("{}", e),
                };

                let row = df
                    .get_row(row_index)
                    .unwrap()
                    .0
                    .into_iter()
                    .map(|v| {
                        v.cast(&DataType::Float64)
                            .unwrap()
                            .try_extract::<Float>()
                            .unwrap()
                    })
                    .collect_vec();

                // position is first 1x3
                // rotation is next 1x9
                matrix_translation = Mat4::new_translation(&Vec3::from_row_slice(&row[..3]));
                matrix_orientation = glm::mat3_to_mat4(&Mat3::from_row_slice(&row[3..]));
            }
            CfgState::Spice(_spice) => {
                #[cfg(not(feature = "spice"))]
                panic!("Feature `spice` is not enabled. The feature is required to compute the position of the camera from Earth direction.");

                #[cfg(feature = "spice")]
                {
                    let position = {
                        if let Some(origin) = &_spice.origin.as_ref().or(cfg.spice.origin.as_ref())
                        {
                            let frame_to = _spice
                                .into_frame
                                .as_ref()
                                .or(cfg.spice.frame.as_ref())
                                .cloned()
                                .unwrap_or(DEFAULT_FRAME.to_string());
                            let (position, _lt) = spice::spkpos(
                                &cfg.bodies[body].id,
                                elapsed_from_start as f64,
                                &frame_to,
                                "none",
                                &origin,
                            );
                            Vec3::from_row_slice(&position)
                        } else {
                            Vec3::zeros()
                        }
                    };

                    let rotation = {
                        if let Some(frame) = &_spice.frame {
                            let frame_to = _spice
                                .into_frame
                                .as_ref()
                                .or(cfg.spice.frame.as_ref())
                                .cloned()
                                .unwrap_or(DEFAULT_FRAME.to_string());
                            let rotation =
                                spice::pxform(&frame, &frame_to, elapsed_from_start as f64);
                            Mat3::from_row_slice(&rotation.iter().cloned().flatten().collect_vec())
                        } else {
                            Mat3::identity()
                        }
                    };

                    if cfg.preferences.debug {
                        println!("Body state computed with spice.");
                        println!("position: {:?}", position.as_slice());
                        println!("rotation: {}", rotation);
                    }

                    matrix_translation = Mat4::new_translation(&position);
                    matrix_orientation = glm::mat3_to_mat4(&rotation);
                }
            }
        };

        bodies_data[body].translation = matrix_translation;
        bodies_data[body].orientation = matrix_orientation * matrix_spin_tilt;

        bodies[body].matrix_model =
            bodies_data[body].translation * bodies_data[body].orientation * matrix_spin;
        matrix_model_reference * bodies[body].matrix_model
    }

    fn fn_update_body_data(
        &mut self,
        _cfg: &Cfg,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _bodies_data: &mut [BodyData],
        _time: &Time,
        _scene: &WindowScene,
    ) {
    }

    fn fn_update_body_colormap(
        &self,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &[BodyData],
        cfg: &Cfg,
        win: &Window,
    ) {
        let scalars = match &cfg.window.colormap.scalar {
            Some(CfgScalar::AngleIncidence) => compute_cosine_incidence_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                &win.scene.borrow().light.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AngleEmission) => compute_cosine_emission_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                &win.scene.borrow().camera.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AnglePhase) => compute_cosine_phase_angle(
                &bodies[body],
                &win.scene.borrow().camera.position.normalize(),
                &win.scene.borrow().light.position.normalize(),
            )
            .map(|a| a.acos() * DPR),
            None => return,
            _ => unreachable!(),
        };

        update_colormap_scalar(win, cfg, scalars.as_slice(), &mut bodies[body], body);
    }

    fn fn_update_body(
        &mut self,
        cfg: &Cfg,
        body: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        time: &Time,
        window: &Window,
    ) {
        self.fn_update_body_matrix_model(cfg, body, bodies, bodies_data, time);
        self.fn_update_body_data(cfg, body, bodies, bodies_data, time, &window.scene.borrow());
        self.fn_update_body_colormap(body, bodies, bodies_data, cfg, window);
    }

    fn fn_export_iteration(
        &self,
        _body: usize,
        _cfg: &Cfg,
        _time: &Time,
        _folders: &FoldersRun,
        _is_first_it: bool,
    ) {
    }

    fn fn_export_iteration_period(
        &self,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _cfg: &Cfg,
        _folders: &FoldersRun,
        _exporting_started_elapsed: i64,
        _is_first_it_export: bool,
    ) {
    }

    fn fn_iteration_finish(
        &mut self,
        cfg: &Cfg,
        _bodies: &mut [AirlessBody],
        time: &Time,
        win: &Window,
    ) {
        if cfg.simulation.pause_after_first_iteration && time.iteration() == 0
            || time.time_step() == 0
        {
            win.toggle_pause();
        }
    }

    fn fn_render(
        &mut self,
        _cfg: &Cfg,
        bodies: &mut [AirlessBody],
        _time: &Time,
        win: &mut Window,
    ) {
        // self.win.update_vaos(self.bodies.iter_mut().map(|b| &mut b.asteroid_mut().surface));
        win.render_asteroids(&bodies);
        win.swap_window();
    }
}

impl_downcast!(sync Routines);
