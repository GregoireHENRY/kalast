use crate::{
    find_ref_orbit, find_reference_matrix_orientation, matrix_spin, position_in_inertial_frame,
    simu::Scene, util::*, Asteroid, Body, CfgBody, CfgColormap, CfgState, CfgStateManual,
    FoldersRun, Time, Window,
};

use downcast_rs::{impl_downcast, DowncastSync};
use itertools::Itertools;

pub trait RoutinesData {
    fn new(asteroid: &Asteroid, _cb: &CfgBody, _scene: &Scene) -> Self;
}

pub trait Routines: DowncastSync {
    fn fn_setup_body(&mut self, asteroid: Asteroid, cb: &CfgBody, _scene: &Scene) -> Body {
        fn_setup_body_default(asteroid, cb)
    }

    fn fn_update_matrix_model(
        &self,
        ii_body: usize,
        ii_other_bodies: &[usize],
        cb: &CfgBody,
        other_cbs: &[&CfgBody],
        bodies: &mut [Body],
        time: &Time,
        scene: &mut Scene,
    ) {
        fn_update_matrix_model_default(
            ii_body,
            ii_other_bodies,
            cb,
            other_cbs,
            bodies,
            time,
            scene,
        );
    }

    fn fn_iteration_body(
        &mut self,
        ii_body: usize,
        ii_other_bodies: &[usize],
        _cb: &CfgBody,
        _other_cbs: &[&CfgBody],
        bodies: &mut [Body],
        scene: &Scene,
        time: &Time,
    );

    fn fn_update_colormap(
        &self,
        win: &Window,
        cmap: &CfgColormap,
        ii_body: usize,
        body: &mut Body,
        scene: &Scene,
    );

    fn fn_export_iteration(
        &self,
        cb: &CfgBody,
        ii_body: usize,
        time: &Time,
        folders: &FoldersRun,
        is_first_it: bool,
    );

    fn fn_export_iteration_period(
        &self,
        cb: &CfgBody,
        body: &Body,
        ii_body: usize,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    );

    fn fn_end_of_iteration(
        &mut self,
        bodies: &mut [Body],
        time: &Time,
        scene: &Scene,
        win: &Window,
    );
}

impl_downcast!(sync Routines);

pub fn fn_setup_body_default(asteroid: Asteroid, cb: &CfgBody) -> Body {
    Body::new(asteroid, cb)
}

pub fn fn_update_matrix_model_default(
    ii_body: usize,
    ii_other_bodies: &[usize],
    cb: &CfgBody,
    other_cbs: &[&CfgBody],
    bodies: &mut [Body],
    time: &Time,
    scene: &mut Scene,
) {
    let elapsed = time.elapsed_seconds();
    let elapsed_from_start = time.elapsed_seconds_from_start();

    let other_bodies = ii_other_bodies.iter().map(|&ii| &bodies[ii]).collect_vec();

    let mat_orient_ref = find_reference_matrix_orientation(cb, &other_bodies);

    let mat_spin = {
        if cb.spin.period == 0.0 {
            Mat4::identity()
        } else {
            let np_elapsed = elapsed as Float / cb.spin.period;
            let spin = (TAU * np_elapsed + cb.spin.spin0 * RPD) % TAU;
            matrix_spin(spin)
        }
    };

    let mat_translation = match &cb.state {
        CfgState::Manual(CfgStateManual {
            position,
            orientation: _orientation,
        }) => Mat4::new_translation(position),
        CfgState::File(_p) => Mat4::identity(),
        CfgState::Orbit(orb) => {
            let (mu_ref, factor) = find_ref_orbit(&orb, &other_cbs);
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
                Mat4::identity()
            } else {
                Mat4::new_translation(&(pos * 1e-3))
            }
        }
    };

    bodies[ii_body].asteroid.matrix_model =
        mat_orient_ref * mat_translation * bodies[ii_body].mat_orient * mat_spin;
}
