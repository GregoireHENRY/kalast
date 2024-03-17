use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    config::Body, config::CfgCamera, config::CfgCameraDirection, config::CfgCameraPosition,
    config::CfgScalar, config::CfgSun, config::CfgSunPosition, config::Config,
    config::FileBehavior, config::FileColumns, config::FileColumnsOut, config::FileSetup,
    config::FrameCenter, config::SpicePosition, config::SpiceState, config::State,
    config::StateCartesian, config::DEFAULT_ABCORR, config::DEFAULT_FRAME, find_ref_orbit,
    matrix_orientation_obliquity, matrix_spin, position_in_inertial_frame, update_colormap_scalar,
    util::*, AirlessBody, BodyData, FoldersRun, MovementMode, Time, Window, WindowScene,
};

use downcast_rs::{impl_downcast, DowncastSync};
use itertools::{izip, Itertools};
use polars::prelude::*;

pub trait Routines: DowncastSync {
    fn load(&mut self, _body: &AirlessBody, _cb: &Body) {}

    fn init(
        &mut self,
        _cfg: &Config,
        _bodies: &mut [AirlessBody],
        _time: &Time,
        _win: &mut Window,
    ) {
    }

    fn fn_update_scene_core(&self, config: &Config, time: &Time, scene: &mut WindowScene) {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        if let Some(true) = config.preferences.debug.simulation {
            println!("Iteration: {}", time.iteration());
        }

        let mut sun = match &config.scene.sun.position {
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
                    if config.spice.is_loaded() {
                        if let Some(body) = config.bodies.first() {
                            let (position, _lt) = spice::spkpos(
                                "Sun",
                                elapsed_from_start as f64,
                                &config
                                    .spice
                                    .frame
                                    .as_ref()
                                    .cloned()
                                    .unwrap_or(DEFAULT_FRAME.to_string()),
                                "none",
                                &body.name,
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
            CfgSunPosition::Origin => {
                if let Some(body) = config.bodies.first() {
                    match &body.state {
                        State::Orbit(orbit) => match &orbit.frame {
                            FrameCenter::Sun => -position_in_inertial_frame(
                                orbit.a * AU,
                                orbit.e,
                                orbit.i * RPD,
                                orbit.node * RPD,
                                orbit.peri * RPD,
                                elapsed_from_start as Float,
                                orbit.tp,
                                MU_SUN,
                            ),
                            FrameCenter::Body(_) => {
                                if time.iteration() == 0 {
                                    println!("Warning: The Sun is set to be configured from the state of the primary body but only works if the state is an orbit centered on the Sun.");
                                }
                                CfgSun::default_position()
                            }
                        },
                        State::Cartesian(_)
                        | State::Equatorial(_)
                        | State::File
                        | State::Spice
                        | State::SpiceState(_) => {
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
            CfgSunPosition::File => {
                let mut out = read_state_file(
                    config.simulation.file.as_ref().unwrap(),
                    time,
                    &vec![FileColumns::Sun],
                );
                out.pop().unwrap().vec()
            }
        };

        let camera = match &config.scene.camera.position {
            CfgCameraPosition::Cartesian(p) => *p,
            CfgCameraPosition::FromSun => {
                sun.normalize()
                    * config
                        .scene
                        .camera
                        .distance_origin
                        .unwrap_or(CfgCamera::default_distance())
            }
            CfgCameraPosition::Spice => {
                let name = config.scene.camera.name.as_ref().unwrap();
                let frame = config
                    .spice
                    .frame
                    .as_ref()
                    .and_then(|s| Some(s.as_str()))
                    .unwrap_or(DEFAULT_FRAME);
                let abcorr = config
                    .spice
                    .abcorr
                    .as_ref()
                    .and_then(|s| Some(s.as_str()))
                    .unwrap_or(DEFAULT_ABCORR);

                let body = config.bodies.first();
                let body = body.unwrap();
                let origin = &body.name;

                let (position, _lt) =
                    spice::spkpos(&name, elapsed_from_start as f64, frame, &abcorr, &origin);
                let mut position = Vec3::from_row_slice(&position);

                if let Some(distance) = config.scene.camera.distance_origin {
                    position = position.normalize() * distance;
                }

                position
            }
            CfgCameraPosition::SpicePos(SpicePosition { origin, abcorr }) => {
                #[cfg(not(feature = "spice"))]
                {
                    panic!("Feature `spice` is not enabled. This is required to compute the position of the camera using SPICE.")
                }

                #[cfg(feature = "spice")]
                {
                    if !config.spice.is_loaded() {
                        panic!("No SPICE kernel has been loaded. This is required to compute the position of the camera using SPICE.");
                    }

                    let name = config.scene.camera.name.as_ref().unwrap();
                    if let Some(origin) = origin {
                        let frame = config
                            .spice
                            .frame
                            .as_ref()
                            .cloned()
                            .unwrap_or(DEFAULT_FRAME.to_string());
                        let abcorr = abcorr.clone().unwrap_or("NONE".to_string());

                        let (position, _lt) = spice::spkpos(
                            &name,
                            elapsed_from_start as f64,
                            &frame,
                            &abcorr,
                            &origin,
                        );
                        let mut position = Vec3::from_row_slice(&position);

                        if let Some(distance) = config.scene.camera.distance_origin {
                            position = position.normalize() * distance;
                        }

                        position
                    } else {
                        Vec3::zeros()
                    }
                }
            }
            CfgCameraPosition::Reference => {
                if let Some(body) = config.bodies.first() {
                    match &body.state {
                        State::Equatorial(coords) => {
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

        if let Some(true) = config.preferences.debug.simulation {
            println!("camera: {:?}", camera.as_slice());
            println!("sun: {:?}", sun.as_slice());
        }

        scene.light.position = sun;
        scene.camera.position = camera;

        if scene.camera.movement_mode == MovementMode::Lock
            && config.scene.camera.direction == CfgCameraDirection::TargetAnchor
        {
            scene.camera.target_anchor();
        }

        if time.iteration == 0 {
            scene.camera.direction = match config.scene.camera.direction {
                CfgCameraDirection::Cartesian(v) => v,
                CfgCameraDirection::TargetAnchor => -scene.camera.position.normalize(),
            };
            scene.camera.up = config.scene.camera.up;
            scene.camera.up_world = config.scene.camera.up;
            scene.camera.projection = config.scene.camera.projection;

            if let Some(near) = config.scene.camera.near {
                scene.camera.near = Some(near);
            }

            if let Some(far) = config.scene.camera.far {
                scene.camera.far = Some(far);
            }
        }
    }

    fn fn_update_scene(&self, cfg: &Config, time: &Time, scene: &mut WindowScene) {
        self.fn_update_scene_core(cfg, time, scene);
    }

    fn fn_update_body_matrix_model(
        &self,
        config: &Config,
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
        let np_elapsed = if config.bodies[body].spin.period == 0.0 {
            0.0
        } else {
            elapsed as Float / config.bodies[body].spin.period
        };
        let spin_angle = (TAU * np_elapsed + config.bodies[body].spin.spin0 * RPD) % TAU;
        let matrix_spin = matrix_spin(spin_angle, config.bodies[body].spin.axis);

        let matrix_spin_tilt =
            matrix_orientation_obliquity(0.0, config.bodies[body].spin.obliquity * RPD);

        let other_bodies = config
            .bodies
            .iter()
            .enumerate()
            .filter(|(ii, _)| *ii != body)
            .map(|(_, cb)| cb)
            .collect_vec();

        match &config.bodies[body].state {
            State::Cartesian(StateCartesian {
                position,
                orientation,
                reference,
            }) => {
                matrix_translation = Mat4::new_translation(position);
                matrix_orientation *= glm::mat3_to_mat4(orientation);

                if let Some(reference) = reference {
                    let ref_id = config
                        .index_body(reference)
                        .expect(&format!("No body loaded with this id {}", reference));

                    // matrix_orientation = matrix_orientation * bodies_data[ref_id].orientation;

                    matrix_model_reference =
                        bodies_data[ref_id].translation * bodies_data[ref_id].orientation;
                }

                if let Some(true) = config.preferences.debug.simulation {
                    println!("Body state with manual cartesian");
                    println!("position: {:?}", position.as_slice());
                    println!("rotation: {}", orientation);
                    println!("matrix model reference: {}", matrix_model_reference);
                }
            }
            State::Equatorial(_) => {}
            State::Orbit(orbit) => {
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
                    FrameCenter::Sun => {}
                    FrameCenter::Body(id) => {
                        for (pre, cb) in izip!(bodies_data.iter_mut(), &config.bodies) {
                            if cb.name == *id {
                                matrix_model_reference = pre.orientation;
                                break;
                            }
                        }
                    }
                }
            }
            State::File => {
                let mut out = read_state_file(
                    config.simulation.file.as_ref().unwrap(),
                    time,
                    &vec![FileColumns::BodyPos(body), FileColumns::BodyRot(body)],
                );

                matrix_orientation = glm::mat3_to_mat4(&out.pop().unwrap().mat());
                matrix_translation = Mat4::new_translation(&out.pop().unwrap().vec());
            }
            State::Spice => {
                let state = SpiceState::default();
                (matrix_translation, matrix_orientation) =
                    spice_state(&config, &state, body, elapsed_from_start as f64);
            }
            State::SpiceState(state) => {
                (matrix_translation, matrix_orientation) =
                    spice_state(&config, &state, body, elapsed_from_start as f64);
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
        _cfg: &Config,
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
        cfg: &Config,
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
        cfg: &Config,
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
        _cfg: &Config,
        _time: &Time,
        _folders: &FoldersRun,
        _is_first_it: bool,
    ) {
    }

    fn fn_export_iteration_period(
        &self,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _cfg: &Config,
        _folders: &FoldersRun,
        _exporting_started_elapsed: i64,
        _is_first_it_export: bool,
    ) {
    }

    fn fn_iteration_finish(
        &mut self,
        cfg: &Config,
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
        _cfg: &Config,
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

pub fn spice_state(config: &Config, state: &SpiceState, body: usize, et: f64) -> (Mat4, Mat4) {
    #[cfg(not(feature = "spice"))]
    panic!(
        "Feature `spice` is not enabled. The feature is required to compute the state of the body."
    );

    #[cfg(feature = "spice")]
    {
        let position = {
            let name = &config.bodies[body].name;
            let frame = config
                .spice
                .frame
                .as_ref()
                .and_then(|s| Some(s.as_str()))
                .unwrap_or(DEFAULT_FRAME);
            let origin = {
                if let Some(SpicePosition {
                    origin: Some(origin),
                    ..
                }) = state.position.as_ref()
                {
                    origin.as_str()
                } else {
                    config
                        .spice
                        .origin
                        .as_ref()
                        .and_then(|s| Some(s.as_str()))
                        .unwrap()
                }
            };
            let abcorr = {
                if let Some(SpicePosition {
                    abcorr: Some(abcorr),
                    ..
                }) = state.position.as_ref()
                {
                    abcorr.as_str()
                } else {
                    config
                        .spice
                        .abcorr
                        .as_ref()
                        .and_then(|s| Some(s.as_str()))
                        .unwrap_or(DEFAULT_ABCORR)
                }
            };
            let (position, _lt) = spice::spkpos(&name, et, &frame, &abcorr, &origin);
            Vec3::from_row_slice(&position)
        };

        let rotation = {
            if let (Some(frame_from), Some(frame_to)) = (
                config.bodies[body].frame.as_ref(),
                config.spice.frame.as_ref(),
            ) {
                let rotation = spice::pxform(&frame_from, &frame_to, et);
                Mat3::from_row_slice(&rotation.iter().cloned().flatten().collect_vec())
            } else {
                Mat3::identity()
            }
        };

        if let Some(true) = config.preferences.debug.simulation {
            println!("Body state computed with spice.");
            println!("position: {:?}", position.as_slice());
            println!("rotation: {}", rotation);
        }

        let matrix_translation = Mat4::new_translation(&position);
        let matrix_orientation = glm::mat3_to_mat4(&rotation);

        (matrix_translation, matrix_orientation)
    }
}

pub fn read_state_file(
    file: &FileSetup,
    time: &Time,
    columns: &[FileColumns],
) -> Vec<FileColumnsOut> {
    let mut row_index = time.iteration() * file.row_multiplier.unwrap_or(1);

    let df = match CsvReader::from_path(&file.path.as_ref().unwrap()) {
        Ok(reader) => reader.has_header(false).finish().unwrap(),
        Err(e) => panic!("{}", e),
    };

    if let Some(FileBehavior::Loop) = file.behavior {
        row_index = row_index % df.shape().0;
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

    // time elapsed: 1 column
    // sun position: 3 columns
    // loop i=0:N bodies:
    // - body i-th position: 3 columns
    // - body i-th orientation: 9 columns

    let mut out = vec![];

    for column in columns {
        out.push(column.get(&row));
    }

    out
}
