use kalast::{
    simu::Scene, util::*, AirlessBody, ColorMode, Result, Routines, RoutinesViewer, Scenario, Time,
    Window,
};

fn main() -> Result<()> {
    let mut sc = Scenario::new()?;

    sc.change_routines(RoutinesViewerCustom::new());
    sc.load_bodies()?;

    let faces: Vec<usize> =
        serde_yaml::from_value(sc.cfg.bodies[0].extra()["faces"].clone()).unwrap();
    update_surf_face(
        sc.routines
            .downcast_mut::<RoutinesViewerCustom>()
            .as_mut()
            .unwrap(),
        &mut sc.bodies,
        &sc.win,
        &faces,
        0,
    );

    sc.iterations()?;

    Ok(())
}

fn update_surf_face(
    routines: &mut RoutinesViewerCustom,
    bodies: &mut [AirlessBody],
    win: &Window,
    faces: &[usize],
    ii_body: usize,
) {
    let surf = &mut bodies[ii_body].surface;

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
    pub faces: Vec<usize>,
}

impl RoutinesViewerCustom {
    pub fn new() -> Self {
        Self { faces: vec![] }
    }
}

impl Routines for RoutinesViewerCustom {
    fn fn_end_of_iteration(
        &mut self,
        bodies: &mut [AirlessBody],
        _time: &Time,
        _scene: &Scene,
        win: &Window,
    ) {
        if let Some((ii_face, ii_body)) = win.picked() {
            update_surf_face(self, bodies, win, &vec![ii_face], ii_body);
        }
    }
}

impl RoutinesViewer for RoutinesViewerCustom {}
