use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    find_ref_orbit, matrix_spin, position_in_inertial_frame, simu::Scene, update_colormap_scalar,
    util::*, AirlessBody, Cfg, CfgBody, CfgCamera, CfgCameraFrom, CfgCameraFromOptions,
    CfgFrameCenter, CfgScalar, CfgState, CfgStateCartesian, CfgSun, CfgSunFrom, FoldersRun,
    PreComputedBody, Time, Window,
};

#[cfg(feature = "spice")]
use crate::CfgStateSpice;

use downcast_rs::{impl_downcast, DowncastSync};
use itertools::{izip, Itertools};

pub trait RoutinesData {
    fn new(asteroid: &AirlessBody, _cb: &CfgBody, _scene: &Scene) -> Self;
}

pub trait Routines: DowncastSync {
    fn fn_update_scene(&self, cfg: &Cfg, time: &Time, _scene: &Scene) -> Scene {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        let sun = match &cfg.scene.sun {
            CfgSun::Position(p) => *p,
            CfgSun::Equatorial(coords) => coords.xyz(CfgSun::default_distance()),
            CfgSun::From(from) => match from {
                CfgSunFrom::Spice => {
                    if cfg.using_spice() {
                        #[cfg(not(feature = "spice"))]
                        {
                            panic!("Feature `spice` is not enabled. The feature is required to compute the position of the Sun.")
                        }

                        #[cfg(feature = "spice")]
                        {
                            if let Some(body) = cfg.bodies.first() {
                                let (position, _lt) = spice::spkpos(
                                    "Sun",
                                    elapsed_from_start as f64,
                                    "ECLIPJ2000",
                                    "none",
                                    &body.id,
                                );
                                Vec3::from_row_slice(&position)
                            } else {
                                panic!("A body must be loaded to compute the position of the Sun.")
                            }
                        }
                    } else {
                        panic!("Spice is not being used and is needed to compute the position of the Sun. Try loading a spice kernel to enable spice.")
                    }
                }
                CfgSunFrom::OrbitBody => {
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
                                CfgFrameCenter::Body(_) => CfgSun::default_position(),
                            },
                            CfgState::Cartesian(_)
                            | CfgState::Equatorial(_)
                            | CfgState::File(_)
                            | CfgState::Spice(_) => CfgSun::default_position(),
                        }
                    } else {
                        panic!("A body must be loaded to compute the position of the Sun.")
                    }
                }
            },
        };

        let camera = match &cfg.scene.camera {
            CfgCamera::Position(p) => *p,
            CfgCamera::From(CfgCameraFrom {
                from,
                distance_origin,
            }) => match &from {
                CfgCameraFromOptions::Sun => sun.normalize() * *distance_origin,
                CfgCameraFromOptions::Earth => {
                    if let Some(_body) = cfg.bodies.first() {
                        if cfg.using_spice() {
                            #[cfg(not(feature = "spice"))]
                            {
                                panic!("Feature `spice` is not enabled. The feature is required to compute the position of the camera from Earth direction.")
                            }

                            #[cfg(feature = "spice")]
                            {
                                let (position, _lt) = spice::spkpos(
                                    "Earth",
                                    elapsed_from_start as f64,
                                    "ECLIPJ2000",
                                    "none",
                                    &_body.id,
                                );
                                let position = Vec3::from_row_slice(&position);

                                position.normalize() * *distance_origin
                            }
                        } else {
                            panic!("Spice is not being used and is needed to compute the position of the camera from Earth direction. Try loading a spice kernel to enable spice.")
                        }
                    } else {
                        panic!("A body must be loaded to compute the position of the camera from Earth direction. Visualisation is centered on body")
                    }
                }
            },
        };

        Scene { camera, sun }
    }

    fn fn_update_matrix_model(
        &self,
        cfg: &Cfg,
        body: usize,
        pre_computed_bodies: &mut [PreComputedBody],
        time: &Time,
        _scene: &Scene,
    ) -> Mat4 {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        match &cfg.bodies[body].state {
            CfgState::Spice(_from) => {
                #[cfg(not(feature = "spice"))]
                panic!("Feature `spice` is not enabled. The feature is required to compute the position of the camera from Earth direction.");

                #[cfg(feature = "spice")]
                {
                    let CfgStateSpice {
                        origin,
                        frame,
                        frame_to,
                    } = _from;
                    let frame = frame.clone().unwrap_or("J2000".to_string());

                    let position = {
                        if let Some(origin) = origin {
                            let (position, _lt) = spice::spkpos(
                                &cfg.bodies[body].id,
                                elapsed_from_start as f64,
                                &frame,
                                "none",
                                &origin,
                            );
                            Vec3::from_row_slice(&position)
                        } else {
                            Vec3::zeros()
                        }
                    };

                    let rotation = {
                        if let Some(frame_to) = frame_to {
                            let rotation =
                                spice::pxform(&frame, frame_to, elapsed_from_start as f64);
                            Mat3::from_row_slice(&rotation.iter().cloned().flatten().collect_vec())
                        } else {
                            Mat3::identity()
                        }
                    };

                    let mut matrix_model = Mat4::new_translation(&position);

                    for (e, new) in izip!(
                        matrix_model.fixed_view_mut::<3, 3>(1, 1).iter_mut(),
                        rotation.iter()
                    ) {
                        *e = *new;
                    }

                    matrix_model
                }
            }
            anything_else => {
                let mut matrix_orientation_reference = Mat4::identity();

                let mut matrix_orientation = pre_computed_bodies[body].mat_orient;

                let elapsed = time.elapsed_seconds();
                let np_elapsed = if cfg.bodies[body].spin.period == 0.0 {
                    0.0
                } else {
                    elapsed as Float / cfg.bodies[body].spin.period
                };
                let spin_angle = (TAU * np_elapsed + cfg.bodies[body].spin.spin0 * RPD) % TAU;
                let matrix_spin = matrix_spin(spin_angle, cfg.bodies[body].spin.axis);

                let mut matrix_translation = Mat4::identity();

                let other_bodies = cfg
                    .bodies
                    .iter()
                    .enumerate()
                    .filter(|(ii, _)| *ii != body)
                    .map(|(_, cb)| cb)
                    .collect_vec();

                match &anything_else {
                    CfgState::Cartesian(CfgStateCartesian {
                        position,
                        orientation,
                    }) => {
                        matrix_orientation = glm::mat3_to_mat4(orientation);
                        matrix_translation = Mat4::new_translation(position);
                    }
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
                                for (pre, cb) in izip!(pre_computed_bodies.iter_mut(), &cfg.bodies)
                                {
                                    if cb.id == *id {
                                        matrix_orientation_reference = pre.mat_orient;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    CfgState::Equatorial(_) | CfgState::File(_) => {}
                    _ => panic!("tempo"),
                };

                let matrix_body = matrix_translation * matrix_orientation * matrix_spin;
                matrix_orientation_reference * matrix_body
            }
        }
    }

    fn fn_iteration_body(
        &mut self,
        _body: usize,
        _bodies: &mut [AirlessBody],
        _pre_computed_bodies: &mut [PreComputedBody],
        _time: &Time,
        _scene: &Scene,
    ) {
    }

    fn fn_update_colormap(
        &self,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &[PreComputedBody],
        cfg: &Cfg,
        scene: &Scene,
        win: &Window,
    ) {
        let scalars = match &cfg.window.colormap.scalar {
            Some(CfgScalar::AngleIncidence) => compute_cosine_incidence_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                scene,
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AngleEmission) => compute_cosine_emission_angle(
                &bodies[body],
                &pre_computed_bodies[body].normals,
                scene,
            )
            .map(|a| a.acos() * DPR),
            Some(CfgScalar::AnglePhase) => {
                compute_cosine_phase_angle(&bodies[body], scene).map(|a| a.acos() * DPR)
            }
            None => return,
            _ => unreachable!(),
        };

        update_colormap_scalar(win, cfg, scalars.as_slice(), &mut bodies[body], body);
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

    fn fn_end_of_iteration(
        &mut self,
        cfg: &Cfg,
        _bodies: &mut [AirlessBody],
        _time: &Time,
        _scene: &Scene,
        win: &Window,
    ) {
        if cfg.simulation.pause_after_first_iteration || cfg.simulation.step == 0 {
            win.toggle_pause();
        }
    }
}

impl_downcast!(sync Routines);
