use std::cell::RefCell;
use std::rc::Rc;

use kalast::app::App;
use kalast::app::config::Hud;
use kalast::{Mat4, Vec3};

fn main() {
    let mut app = App::new();

    {
        let config = app.sim_config();
        let mut c = config.borrow_mut();
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

        sim.camera.pos = Vec3::new(1.5, 2.0, 1.5);
        sim.camera.look_anchor();

        sim.load_mesh(
            "res/plane_crater_1024-5000_h=0.437.obj",
            Mat4::IDENTITY,
            false,
        );
    }

    while app.is_running() {
        let it = app.simulation.borrow().state.iteration;

        let a = it as f32 * 0.005;
        app.simulation.borrow_mut().sun.pos = Vec3::new(0.0, 20.0 * a.sin(), 20.0 * a.cos());

        app.step();

        // The borrow is scoped, because writing the HUD needs it back.
        let lit = {
            let sim = app.simulation.borrow();
            match sim.facet_illumination(0) {
                Some(illum) => {
                    let n = illum.iter().filter(|&&i| i > 0.0).count();
                    n as f32 / illum.len() as f32
                }
                None => 0.0,
            }
        };
        app.simulation.borrow().huds[0].borrow_mut().text = format!("lit {:.1} %", lit * 100.0);

        if it >= 10000 {
            app.close();
        }
    }
}
