use crate::{
    compute_cosine_emission_angle, compute_cosine_incidence_angle, compute_cosine_phase_angle,
    fn_setup_body_default, simu::Scene, update_colormap_scalar, util::*, Asteroid, Body, CfgBody,
    CfgColormap, CfgScalar, FoldersRun, Routines, RoutinesData, Time, Window,
};

use itertools::Itertools;

pub struct ViewerData {}

impl RoutinesData for ViewerData {
    fn new(_asteroid: &Asteroid, _cb: &CfgBody, _scene: &Scene) -> Self {
        Self {}
    }
}

impl ViewerData {}

pub trait RoutinesViewer: Routines {}

pub struct RoutinesViewerDefault {
    pub data: Vec<ViewerData>,
}

impl Routines for RoutinesViewerDefault {
    fn fn_setup_body(&mut self, asteroid: Asteroid, cb: &CfgBody, scene: &Scene) -> Body {
        self.data.push(ViewerData::new(&asteroid, cb, scene));
        fn_setup_body_default(asteroid, cb)
    }

    fn fn_iteration_body(
        &mut self,
        _ii_body: usize,
        ii_other_bodies: &[usize],
        _cb: &CfgBody,
        _other_cbs: &[&CfgBody],
        bodies: &mut [Body],
        _scene: &Scene,
        time: &Time,
    ) {
        let _dt = time.used_time_step();
        let _other_bodies = ii_other_bodies.iter().map(|&ii| &bodies[ii]).collect_vec();
    }

    fn fn_update_colormap(
        &self,
        win: &Window,
        cmap: &CfgColormap,
        ii_body: usize,
        body: &mut Body,
        scene: &Scene,
    ) {
        fn_update_colormap_viewer_default(&self.data[ii_body], win, cmap, ii_body, body, scene);
    }

    fn fn_export_iteration(
        &self,
        cb: &CfgBody,
        ii_body: usize,
        time: &Time,
        folders: &FoldersRun,
        is_first_it: bool,
    ) {
        fn_export_iteration_viewer_default(&self.data[ii_body], cb, time, folders, is_first_it);
    }

    fn fn_export_iteration_period(
        &self,
        cb: &CfgBody,
        body: &Body,
        ii_body: usize,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    ) {
        fn_export_iteration_period_viewer_default(
            &self.data[ii_body],
            cb,
            body,
            folders,
            exporting_started_elapsed,
            is_first_it_export,
        );
    }

    fn fn_end_of_iteration(
        &mut self,
        _bodies: &mut [Body],
        _time: &Time,
        _scene: &Scene,
        _win: &Window,
    ) {
    }
}

impl RoutinesViewer for RoutinesViewerDefault {}

pub fn routines_viewer_default() -> RoutinesViewerDefault {
    RoutinesViewerDefault { data: vec![] }
}

pub fn fn_update_colormap_viewer_default<D: RoutinesData>(
    _data: &D,
    win: &Window,
    cmap: &CfgColormap,
    ii_body: usize,
    body: &mut Body,
    scene: &Scene,
) {
    let scalars = match &cmap.scalar {
        Some(CfgScalar::AngleIncidence) => {
            compute_cosine_incidence_angle(body, &body.normals, scene).map(|a| a.acos() * DPR)
        }
        Some(CfgScalar::AngleEmission) => {
            compute_cosine_emission_angle(body, &body.normals, scene).map(|a| a.acos() * DPR)
        }
        Some(CfgScalar::AnglePhase) => {
            compute_cosine_phase_angle(body, scene).map(|a| a.acos() * DPR)
        }
        None => return,
        _ => unreachable!(),
    };

    update_colormap_scalar(win, cmap, scalars.as_slice(), &mut body.asteroid, ii_body);
}

pub fn fn_export_iteration_viewer_default<D: RoutinesData>(
    _data: &D,
    cb: &CfgBody,
    time: &Time,
    _folders: &FoldersRun,
    _is_first_it: bool,
) {
    let _np_elapsed = time.elapsed_seconds() as Float / cb.spin.period;
}

pub fn fn_export_iteration_period_viewer_default<D: RoutinesData>(
    _data: &D,
    _cb: &CfgBody,
    _body: &Body,
    _folders: &FoldersRun,
    _exporting_started_elapsed: i64,
    _is_first_it_export: bool,
) {
}
