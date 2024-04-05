use figment::{
    providers::{Format, Serialized, Toml},
    Figment,
};

use kalast::{
    config::Config, util::*, AirlessBody, ColorMode, Result, Routines, RoutinesViewer,
    RoutinesViewerDefault, Scenario, Time, Window,
};

fn main() -> Result<()> {
    println!(
        "kalast<{}> (built on {} with rustc<{}>)",
        version(),
        DATETIME,
        RUSTC_VERSION
    );

    let config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file("preferences.toml"))
        .merge(Toml::file("cfg/cfg.toml"))
        .extract()
        .unwrap();

    let mut sc = Scenario::new(config)?;
    sc.change_routines(RoutinesViewerCustom::new());
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
    pub default: RoutinesViewerDefault,
}

impl RoutinesViewerCustom {
    pub fn new() -> Self {
        Self {
            faces: vec![],
            default: RoutinesViewerDefault::new(),
        }
    }
}

impl Routines for RoutinesViewerCustom {
    fn init(
        &mut self,
        cfg: &Cfg,
        bodies: &mut [AirlessBody],
        time: &Time,
        win: Option<&mut Window>,
    ) {
        self.default.init(cfg, bodies, time, win);
        let faces: Vec<_> = serde_yaml::from_value(cfg.bodies[0].extra()["faces"].clone())
            .expect("No value `faces` for body");
        update_surf_face(self, bodies, win, &faces, 0);
    }

    fn fn_render(&mut self, cfg: &Cfg, bodies: &mut [AirlessBody], time: &Time, win: &mut Window) {
        if let Some((ii_face, _)) = win.picked() {
            update_surf_face(self, bodies, win, &vec![ii_face], 0);
        }

        self.default.fn_render(cfg, bodies, time, win);
    }
}

impl RoutinesViewer for RoutinesViewerCustom {}
