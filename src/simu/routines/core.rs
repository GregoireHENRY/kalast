use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    find_matrix_spin, find_matrix_translation, find_reference_matrix_orientation, simu::Scene,
    update_colormap_scalar, util::*, AirlessBody, Cfg, CfgBody, CfgScalar, FoldersRun,
    PreComputedBody, Time, Window,
};

#[cfg(feature = "spice")]
use crate::CfgCamera;

#[cfg(not(feature = "spice"))]
use crate::{find_ref_orbit, position_in_inertial_frame, CfgState};

use downcast_rs::{impl_downcast, DowncastSync};

#[cfg(not(feature = "spice"))]
use itertools::Itertools;

pub trait RoutinesData {
    fn new(asteroid: &AirlessBody, _cb: &CfgBody, _scene: &Scene) -> Self;
}

pub trait Routines: DowncastSync {
    fn fn_update_scene(&self, cfg: &Cfg, time: &Time, scene: &mut Scene) {
        let elapsed_from_start = time.elapsed_seconds_from_start();

        #[cfg(not(feature = "spice"))]
        if let Some(body) = cfg.bodies.first() {
            let other_bodies = cfg.bodies.iter().skip(1).collect_vec();

            match &body.state {
                CfgState::Equatorial(coords) => {
                    let distance = cfg.scene.camera.as_earth().unwrap();
                    let position = coords.xyz(distance);
                    dbg!(position);
                    scene.cam_pos = -position;

                    let coords_sun = cfg.scene.sun.as_equatorial().unwrap();
                    let position_sun = coords_sun.xyz(distance);
                    dbg!(position_sun);
                    scene.sun_pos = scene.cam_pos + position_sun;
                }
                CfgState::Orbit(orb) => {
                    let (mu_ref, factor) = find_ref_orbit(&orb, &other_bodies);
                    let pos = position_in_inertial_frame(
                        orb.a * factor,
                        orb.e,
                        orb.i * RPD,
                        orb.node * RPD,
                        orb.peri * RPD,
                        elapsed_from_start as Float,
                        orb.tp,
                        mu_ref,
                    );
                    if mu_ref == MU_SUN {
                        scene.sun_pos = -pos;
                    }
                }
                _ => {}
            };
        }

        #[cfg(feature = "spice")]
        if let Some(body) = cfg.bodies.first() {
            let context = match cfg.scene.camera {
                CfgCamera::Earth(distance) => ("Earth", "J2000", distance),
                _ => panic!("Expecting Earth with distance for camera settings"),
            };
            let (position, _lt) = spice::spkpos(
                context.0,
                elapsed_from_start as f64,
                context.1,
                "none",
                &body.id,
            );
            let position = Vec3::from_row_slice(&position);

            let (position_sun, _lt_sun) = spice::spkpos(
                "Sun",
                elapsed_from_start as f64,
                context.1,
                "none",
                &body.id,
            );
            let position_sun = Vec3::from_row_slice(&position_sun);

            scene.cam_pos = position.normalize() * context.2;
            scene.sun_pos = position_sun.normalize() * context.2;
        }
    }

    fn fn_update_matrix_model(
        &self,
        cfg: &Cfg,
        body: usize,
        bodies: &mut [AirlessBody],
        pre_computed_bodies: &mut [PreComputedBody],
        time: &Time,
        _scene: &Scene,
    ) {
        let mat_orient_ref = find_reference_matrix_orientation(&cfg, body, pre_computed_bodies);
        let mat_spin = find_matrix_spin(cfg, body, time);
        let mat_translation = find_matrix_translation(cfg, body, time);

        bodies[body].matrix_model =
            mat_orient_ref * mat_translation * pre_computed_bodies[body].mat_orient * mat_spin;
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
        if cfg.simulation.pause_after_first_iteration {
            win.toggle_pause();
        }
    }
}

impl_downcast!(sync Routines);
