use kalast::prelude::*;

fn main() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap();
    let sc = Scenario::new(path)?;

    let mut sc = sc.select_routines(RoutinesViewerCustom::new());
    sc.load_bodies()?;

    let faces: Vec<usize> =
        serde_yaml::from_value(sc.cfg.bodies[0].extra["faces"].clone()).unwrap();
    update_surf_face(&mut sc.routines, &mut sc.bodies, &sc.win, &faces, 0);

    sc.iterations()?;

    Ok(())
}

fn update_surf_face<B: Body>(
    routines: &mut RoutinesViewerCustom,
    bodies: &mut [B],
    win: &Window,
    faces: &[usize],
    ii_body: usize,
) {
    let surf = &mut bodies[ii_body].asteroid_mut().surface;

    for &ii_face in faces {
        if !routines.faces.contains(&ii_face) {
            routines.faces.push(ii_face);
            surf.faces[ii_face].vertex.color_mode = ColorMode::Color;
            surf.faces[ii_face].vertex.color = vec3(1.0, 1.0, 0.0);
        } else {
            routines
                .faces
                .remove(routines.faces.iter().position(|&v| v == ii_face).unwrap());
            surf.faces[ii_face].vertex.color_mode = ColorMode::DiffuseLight;
            surf.faces[ii_face].vertex.color = vec3(1.0, 1.0, 1.0);
        }
    }
    surf.apply_facedata_to_vertices();
    win.update_vao(ii_body, surf);
    println!("{:?}", routines.faces);
}

pub struct RoutinesViewerCustom {
    pub default: RoutinesViewerDefault,
    pub faces: Vec<usize>,
}

impl Routines for RoutinesViewerCustom {
    fn fn_setup_body<B: Body>(&mut self, asteroid: Asteroid, cb: &CfgBody, scene: &Scene) -> B {
        self.default.fn_setup_body(asteroid, cb, scene)
    }
    fn fn_update_matrix_model<B: Body>(
        &self,
        body: &mut B,
        cb: &CfgBody,
        other_cbs: &[&CfgBody],
        time: &Time,
        mat_orient_ref: &Mat4,
    ) {
        self.default
            .fn_update_matrix_model(body, cb, other_cbs, time, mat_orient_ref)
    }

    fn fn_iteration_body<B: Body>(
        &mut self,
        ii_body: usize,
        ii_other_bodies: &[usize],
        cb: &CfgBody,
        other_cbs: &[&CfgBody],
        bodies: &mut [B],
        scene: &Scene,
        time: &Time,
    ) {
        self.default
            .fn_iteration_body(ii_body, ii_other_bodies, cb, other_cbs, bodies, scene, time)
    }

    fn fn_update_colormap<B: Body>(
        &self,
        win: &Window,
        cmap: &CfgColormap,
        ii_body: usize,
        body: &mut B,
        scene: &Scene,
    ) {
        self.default
            .fn_update_colormap(win, cmap, ii_body, body, scene)
    }

    fn fn_export_iteration(
        &self,
        cb: &CfgBody,
        ii_body: usize,
        time: &Time,
        folders: &FoldersRun,
        is_first_it: bool,
    ) {
        self.default
            .fn_export_iteration(cb, ii_body, time, folders, is_first_it)
    }

    fn fn_export_iteration_period<B: Body>(
        &self,
        cb: &CfgBody,
        body: &B,
        ii_body: usize,
        folders: &FoldersRun,
        exporting_started_elapsed: i64,
        is_first_it_export: bool,
    ) {
        self.default.fn_export_iteration_period(
            cb,
            body,
            ii_body,
            folders,
            exporting_started_elapsed,
            is_first_it_export,
        )
    }

    fn fn_end_of_iteration<B: Body>(
        &mut self,
        bodies: &mut [B],
        _time: &Time,
        _scene: &Scene,
        win: &Window,
    ) {
        if let Some((ii_face, ii_body)) = win.picked() {
            update_surf_face(self, bodies, win, &vec![ii_face], ii_body);
        }
    }
}

impl RoutinesViewerCustom {
    pub fn new() -> Self {
        Self {
            default: simu::routines_viewer_default(),
            faces: vec![],
        }
    }
}
