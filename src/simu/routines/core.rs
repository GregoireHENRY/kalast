use std::path::Path;

use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    config::{
        Body, CfgCamera, CfgCameraDirection, CfgCameraPosition, CfgScalar, CfgSun, CfgSunPosition,
        Config, FileBehavior, FileColumns, FileColumnsOut, FileSetup, FrameCenter, SpicePosition,
        SpiceState, State, StateCartesian, DEFAULT_ABCORR, DEFAULT_FRAME,
        SIMULATION_TIME_FREQUENCY,
    },
    find_ref_orbit, matrix_orientation_obliquity, matrix_spin, position_in_inertial_frame,
    update_colormap_scalar,
    util::*,
    AirlessBody, BodyData, ColorMode, Export, FoldersRun, MovementMode, ProjectionMode,
    SelectedFace, Time, Window,
};

use downcast_rs::{impl_downcast, DowncastSync};
use itertools::{izip, Itertools};
use polars::prelude::*;

pub trait Routines: DowncastSync {
    fn load(&mut self, _body: &AirlessBody, _cb: &Body) {}

    fn fn_update_file_index_core(
        &self,
        config: &Config,
        time: &mut Time,
        file: &Path,
        opt_loop: bool,
    ) {
        let df = match CsvReader::from_path(&file) {
            Ok(reader) => reader.has_header(true).finish().unwrap(),
            Err(e) => panic!("{}", e),
        };

        let elapsed = df.column("elapsed").unwrap();
        let mut time_elapsed = time.elapsed_seconds();
        let mut found = false;
        let mut current_time_file = None;
        let mut next_tried_time_file = None;

        if opt_loop {
            let last = elapsed
                .iter()
                .last()
                .unwrap()
                .try_extract::<usize>()
                .unwrap();
            time_elapsed = time_elapsed % last;
        }

        if config.simulation.read_file_data_only {
            time.file_row = Some(time.iteration);
            found = true;
        }

        if found {
            return;
        }

        if let Some(&(mut index)) = time.file_row.as_ref() {
            current_time_file = Some(elapsed.get(index).unwrap().try_extract::<usize>().unwrap());

            index += 1;

            if opt_loop {
                index = index % df.shape().0;
            }

            if let Ok(try_next) = elapsed.get(index) {
                let try_next = try_next.try_extract::<usize>().unwrap();
                next_tried_time_file = Some(try_next);

                if try_next == time_elapsed {
                    time.file_row = Some(index);
                    found = true;
                }
            }
        }

        // attention avec skip car si modulo applied alors on va tout skip sans trouver le bon au début
        // let skip = time.file_row.unwrap_or_default();
        let skip = 0;

        if found {
            return;
        }

        time.file_row = Some(
                elapsed
                    .iter()
                    .skip(skip)
                    .map(|t| t.try_extract::<usize>().unwrap())
                    .position(|t| time_elapsed == t)
                    .expect(&format!(
                        "Elapsed time is not synchronised with file. Current elapsed time is {}{}. \
                         From reading the file, it was impossible to find a time matching simulation time. \
                         For more info, current: {:?} and next tried: {:?}.",
                        time.elapsed_seconds(),
                        opt_loop.then(|| format!(
                            "(Looping on file, modulo applied to time elapsed: {})",
                            time_elapsed
                        )).unwrap_or("".to_string()),
                        current_time_file,
                        next_tried_time_file
                    )),
            );

        // println!(
        //     "{:?} {} {} {}",
        //     time.file_row,
        //     time.elapsed_seconds(),
        //     time_elapsed,
        //     time.time_step
        // );
    }

    fn fn_update_file_index(&self, config: &Config, time: &mut Time) {
        let mut files = vec![config.simulation.file.as_ref()];
        for body in &config.bodies {
            files.push(body.file_data.as_ref());
        }

        for file in &files {
            if let Some(file) = file {
                let opt_loop =
                    file.behavior.clone().unwrap_or(FileBehavior::Stop) == FileBehavior::Loop;
                self.fn_update_file_index_core(config, time, file.path.as_ref().unwrap(), opt_loop);
            }
        }
    }

    fn fn_update_scene_core_first_it(
        &self,
        config: &Config,
        _sun: &mut Vec3,
        _time: &Time,
        win: &Option<&mut Window>,
        // bodies: &mut [AirlessBody],
        // bodies_data: &mut [BodyData],
    ) {
        if let Some(win) = win {
            // scope for scene mut borrow
            {
                let mut scene = win.scene.borrow_mut();
                scene.camera.direction = match config.scene.camera.direction {
                    CfgCameraDirection::Cartesian(v) => v,
                    CfgCameraDirection::TargetAnchor => -scene.camera.position.normalize(),
                };
                scene.camera.up = config.scene.camera.up;
                scene.camera.up_world = config.scene.camera.up;
                let mut projection = config.scene.camera.projection;

                match &mut projection {
                    ProjectionMode::Perspective(fovy) => {
                        *fovy *= RPD;
                    }
                    _ => {}
                }

                scene.camera.projection = projection;

                if let Some(near) = config.scene.camera.near {
                    scene.camera.near = Some(near);
                }

                if let Some(far) = config.scene.camera.far {
                    scene.camera.far = Some(far);
                }
            }
        }
    }

    fn fn_update_scene_core(
        &self,
        config: &Config,
        sun: &mut Vec3,
        time: &Time,
        win: &Option<&mut Window>,
    ) {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        if let Some(true) = config.preferences.debug.simulation {
            println!("Iteration: {}", time.iteration());
        }

        let mut sun_pos = match &config.scene.sun.position {
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
                            FrameCenter::Sun => {
                                -position_in_inertial_frame(
                                    orbit.a * AU,
                                    orbit.e,
                                    orbit.i * RPD,
                                    orbit.node * RPD,
                                    orbit.peri * RPD,
                                    elapsed_from_start as Float,
                                    orbit.tp,
                                    MU_SUN,
                                ) * 1e-3
                            }
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

        let cam_pos = match &config.scene.camera.position {
            None => CfgCamera::default_position(),
            Some(CfgCameraPosition::Cartesian(p)) => *p,
            Some(CfgCameraPosition::FromSun) => {
                sun_pos.normalize()
                    * config
                        .scene
                        .camera
                        .distance_origin
                        .unwrap_or(CfgCamera::default_distance())
            }
            Some(CfgCameraPosition::Spice) => {
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
            Some(CfgCameraPosition::SpicePos(SpicePosition { origin, abcorr })) => {
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
            Some(CfgCameraPosition::Reference) => {
                if let Some(body) = config.bodies.first() {
                    match &body.state {
                        State::Equatorial(coords) => {
                            let position = -coords.xyz_with_distance(
                                coords.distance.unwrap_or(CfgCamera::default_distance()),
                            );
                            sun_pos += position;
                            position
                        }
                        _ => panic!(
                        "Camera on reference mode only work with primary body state equatorial."
                    ),
                    }
                } else {
                    panic!("No body has been loaded to compute camera position.")
                }
            }
        };

        if let Some(true) = config.preferences.debug.simulation {
            println!("camera: {:?}", cam_pos.as_slice());
            println!("sun: {:?}", sun_pos.as_slice());
        }

        *sun = sun_pos;
        if let Some(win) = win {
            let mut scene = win.scene.borrow_mut();

            scene.light.position = sun_pos;
            scene.camera.position = cam_pos;

            if scene.camera.movement_mode == MovementMode::Lock
                && config.scene.camera.direction == CfgCameraDirection::TargetAnchor
            {
                scene.camera.target_anchor();
            }
        }
    }

    fn fn_update_scene(
        &self,
        cfg: &Config,
        sun: &mut Vec3,
        time: &Time,
        win: Option<&mut Window>,
        export: &Export,
    ) {
        if export.is_first_it {
            self.fn_update_scene_core_first_it(cfg, sun, time, &win);
        }

        self.fn_update_scene_core(cfg, sun, time, &win);
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
                    ) * 1e-3;
                    matrix_translation = Mat4::new_translation(&pos);
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

    fn fn_update_body_data_core(
        &mut self,
        _config: &Config,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _bodies_data: &mut [BodyData],
        _sun: &Vec3,
        _time: &Time,
        _window: Option<&Window>,
    ) {
    }

    fn fn_update_body_data(
        &mut self,
        _config: &Config,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _bodies_data: &mut [BodyData],
        _sun: &Vec3,
        _time: &Time,
        _win: Option<&Window>,
    ) {
    }

    fn read_file_data(&self, config: &Config, body: usize, time: &Time) -> Vec<f64> {
        if let Some(file) = config.bodies[body].file_data.as_ref() {
            let df = match CsvReader::from_path(&file.path.as_ref().unwrap()) {
                Ok(reader) => reader.has_header(true).finish().unwrap(),
                Err(e) => panic!("{}", e),
            };

            let row_index = time.file_row.unwrap();
            df.get_row(row_index)
                .unwrap()
                .0
                .into_iter()
                .skip(1)
                .map(|v| {
                    v.cast(&DataType::Float64)
                        .unwrap()
                        .try_extract::<Float>()
                        .unwrap()
                })
                .collect_vec()
        } else {
            panic!("No file data found.");
        }
    }

    fn fn_update_body_colormap(
        &self,
        config: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &[BodyData],
        time: &Time,
        win: &Window,
    ) {
        if let Some(cmap) = config.window.colormap.as_ref() {
            let scalars = match cmap.scalar {
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
                Some(CfgScalar::File) => {
                    let v = self.read_file_data(config, body, time);
                    DRVector::from_row_slice(&v)
                }
                _ => unreachable!(),
            };
            update_colormap_scalar(win, config, scalars.as_slice(), &mut bodies[body], body);
        }
    }

    fn fn_update_body(
        &mut self,
        config: &Config,
        body: usize,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        sun: &Vec3,
        time: &Time,
        window: Option<&Window>,
    ) {
        self.fn_update_body_matrix_model(config, body, bodies, bodies_data, time);
        self.fn_update_body_data(config, body, bodies, bodies_data, sun, time, window);

        if let Some(window) = window {
            self.fn_update_body_colormap(config, body, bodies, bodies_data, time, window);
        }
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
        config: &Config,
        _bodies: &mut [AirlessBody],
        time: &mut Time,
        win: Option<&Window>,
    ) {
        if let Some(true) = config.preferences.debug.simulation_time {
            let freq = config
                .preferences
                .debug
                .simulation_time_frequency
                .unwrap_or(SIMULATION_TIME_FREQUENCY);

            let mut r =
                time.elapsed_seconds() as Float / config.simulation.duration as Float * 100.0;

            let s = numdigits_comma(freq);

            if s > 0 {
                let d = 10.0f64.powi(s as _);
                r = (r * d).floor() / d;
            }

            let last = time.last_debug_time.unwrap_or(0.0);

            if r >= last + freq {
                time.last_debug_time = Some(r);

                println!(
                    "Simulated {:.s$}% ({}it {}/{}).",
                    r,
                    time.iteration(),
                    time.elapsed_seconds(),
                    config.simulation.duration,
                );
            }
        }

        if let Some(win) = win {
            if config.simulation.pause_first_it.unwrap_or_default() && time.iteration() == 0
                || time.time_step() == 0
            {
                win.toggle_pause();
            }
        }
    }

    fn fn_render(
        &mut self,
        config: &Config,
        bodies: &mut [AirlessBody],
        bodies_data: &mut [BodyData],
        window: &mut Window,
        time: &Time,
        export: &Export,
    ) {
        if export.is_first_it {
            for body in 0..bodies.len() {
                let faces = &config.bodies[body].faces_selected;
                update_surf_selected_faces(config, bodies, bodies_data, window, faces, body);
            }
        }

        if let Some((ii_face, body)) = window.picked() {
            update_surf_selected_faces(config, bodies, bodies_data, window, &vec![ii_face], body);
        }

        window.update_vaos(bodies.iter_mut().map(|b| &mut b.surface));
        window.render_asteroids(&bodies);
        window.swap_window();
        if let Some(true) = config.preferences.debug.window {
            println!("New frame: {}.", time.elapsed_seconds());
        }
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
    let df = match CsvReader::from_path(&file.path.as_ref().unwrap()) {
        Ok(reader) => reader.has_header(true).finish().unwrap(),
        Err(e) => panic!("{}", e),
    };

    let row_index = time.file_row.unwrap();
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

pub fn update_surf_selected_faces(
    config: &Config,
    bodies: &mut [AirlessBody],
    bodies_data: &mut [BodyData],
    _win: &Window,
    faces: &[usize],
    ii_body: usize,
) {
    let selected = &mut bodies_data[ii_body].selected;

    for &face in faces {
        if !selected.iter().any(|f| f.index == face) {
            selected.push(SelectedFace::set(&bodies[ii_body], face));
            let v = &mut bodies[ii_body].surface.faces[face].vertex;
            v.color_mode = ColorMode::Color;
            v.color = config.window.color_selection;
        } else {
            let selected = selected.remove(selected.iter().position(|f| f.index == face).unwrap());
            let v = &mut bodies[ii_body].surface.faces[face].vertex;
            v.color_mode = selected.mode;
            match selected.mode {
                ColorMode::Color => v.color = selected.color,
                _ => {}
            };
        }
    }
    let s = &mut bodies[ii_body].surface;
    s.apply_facedata_to_vertices();
    // win.update_vao(ii_body, s);
    println!(
        "Selected faces: {:?}\n",
        selected.iter().map(|f| f.index).collect_vec()
    );
}
