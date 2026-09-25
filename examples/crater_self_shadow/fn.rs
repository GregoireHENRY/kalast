use std::cell::RefCell;
use std::rc::Rc;

use kalast::app::App;
use kalast::app::config::Hud;
use kalast::app::simulation::Simulation;
use kalast::{Float, Mat4, Vec3};

fn main() {
    let mut app = App::new();

    {
        let config = app.sim_config();
        let mut c = config.borrow_mut();
        c.axes.style = kalast::app::axes::AxesStyle::Gizmo;
        c.shading.render_back_face = true;
        c.light.cube_show = true;
        c.shadows.access_shadow_map = true;
        c.wireframe.mode = 2;
        c.wireframe.color = wgpu::Color {
            r: 0.05,
            g: 0.05,
            b: 0.05,
            a: 1.0,
        };
    }

    {
        let mut sim = app.simulation.borrow_mut();

        let mut hud = Hud::new("");
        hud.size = 16.0;
        sim.huds = vec![Rc::new(RefCell::new(hud))];

        sim.camera.pos = Vec3::new(1.5778934, 1.9384689, 1.5082116);
        sim.camera.look_anchor();

        sim.load_mesh(
            "res/plane_crater_1024-5000_h=0.437.obj",
            Mat4::IDENTITY,
            false,
        );
    }

    // Using closures for before_ and after_render but can be written in plain
    // separated functions like main.py could have been lambda functions aswell.

    app.set_before_render(|sim: &mut Simulation, _dt: Float| {
        let a = sim.state.iteration as Float * 0.005;
        sim.sun.pos = Vec3::new(0.0, 20.0 * a.sin(), 20.0 * a.cos());
    });

    app.set_after_render(|sim: &mut Simulation, _dt: Float| {
        let lit = match sim.facet_illumination(0) {
            Some(illum) if !illum.is_empty() => {
                illum.iter().filter(|&&i| i > 0.0).count() as Float / illum.len() as Float
            }
            _ => 0.0,
        };
        let it = sim.state.iteration;
        sim.huds[0].borrow_mut().text = format!("lit {:.1} %", lit * 100.0);
    });

    app.start();
}
