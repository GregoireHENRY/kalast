use crate::prelude::*;

pub trait RoutinesData {
    fn new(asteroid: &Asteroid, _cb: &CfgBody, _scene: &Scene) -> Self;
}

pub trait Routines {
    fn fn_setup_body<B: Body>(&mut self, asteroid: Asteroid, cb: &CfgBody, _scene: &Scene) -> B {
        fn_setup_body_default(asteroid, cb)
    }

    fn fn_update_matrix_model<B: Body>(
        &self,
        body: &mut B,
        cb: &CfgBody,
        other_cbs: &[&CfgBody],
        time: &Time,
        mat_orient_ref: &Mat4,
    ) {
        fn_update_matrix_model_default(body, cb, other_cbs, time, mat_orient_ref);
    }

    fn fn_iteration_body<B: Body>(
        &mut self,
        ii_body: usize,
        ii_other_bodies: &[usize],
        _cb: &CfgBody,
        _other_cbs: &[&CfgBody],
        bodies: &mut [B],
        scene: &Scene,
        time: &Time,
    );

    fn fn_update_colormap<B: Body>(
        &self,
        win: &Window,
        cmap: &CfgColormap,
        ii_body: usize,
        body: &mut B,
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

    fn fn_export_iteration_period<B: Body>(
        &self,
        cb: &CfgBody,
        body: &B,
        ii_body: usize,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    );

    fn fn_end_of_iteration<B: Body>(
        &mut self,
        bodies: &mut [B],
        time: &Time,
        scene: &Scene,
        win: &Window,
    );
}

pub fn fn_setup_body_default<B: Body>(asteroid: Asteroid, cb: &CfgBody) -> B {
    B::new(asteroid, cb)
}

pub fn fn_update_matrix_model_default<B: Body>(
    body: &mut B,
    cb: &CfgBody,
    other_cbs: &[&CfgBody],
    time: &Time,
    mat_orient_ref: &Mat4,
) {
    let elapsed = time.elapsed_seconds();
    let elapsed_from_start = time.elapsed_seconds_from_start();

    let mat_spin = {
        if cb.spin.period == 0.0 {
            Mat4::identity()
        } else {
            let np_elapsed = elapsed as Float / cb.spin.period;
            let spin = (TAU * np_elapsed + cb.spin.spin0) % TAU;
            ast::matrix_spin(spin)
        }
    };

    let mat_translation = match &cb.state {
        CfgState::Position(pos) => Mat4::new_translation(pos),
        CfgState::Path(_p) => Mat4::identity(),
        CfgState::Orbit(orb) => {
            let (mu_ref, factor) = simu::find_ref_orbit(&orb, &other_cbs);
            let pos = orbit::position_in_inertial_frame(
                orb.a * factor,
                orb.e,
                orb.i * RPD,
                orb.node * RPD,
                orb.peri * RPD,
                elapsed_from_start as Float,
                orb.tp,
                mu_ref,
            );
            Mat4::new_translation(&(pos * 1e-3))
        }
    };

    body.asteroid_mut().matrix_model =
        mat_orient_ref * mat_translation * body.mat_orient() * mat_spin;
}
