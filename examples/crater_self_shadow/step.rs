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
        c.vsync = false;
        c.render_back_face = true;
        c.debug_light_cube_show = true;
        c.access_shadow_map = true;
        c.wireframe_mode = 2;
        c.wireframe_color = wgpu::Color {
            r: 0.05,
            g: 0.05,
            b: 0.05,
            a: 1.0,
        };
        // c.shadow_pcf = 8;
        // c.axes = "blender".to_string();
        // c.colorbar = true;
    }

    {
        let mut sim = app.simulation.borrow_mut();

        let mut hud = Hud::new("");
        hud.size = 16.0;
        sim.huds = vec![Rc::new(RefCell::new(hud))];

        sim.sun.pos = Vec3::new(0.0, 20.0, 5.0);
        sim.camera.pos = Vec3::new(1.5778934, 1.9384689, 1.5082116);
        sim.camera.dir = Vec3::new(-0.54051036, -0.6640262, -0.5166407);
        sim.camera.up = Vec3::new(-0.3261482, -0.40068075, 0.85620236);

        sim.load_mesh(
            "res/plane_crater_1024-5000_h=0.437.obj",
            Mat4::IDENTITY,
            true,
        );
    }

    while app.is_running() {
        let it = app.simulation.borrow().state.iteration;

        // Everything before `app.step()` is what `before_render` would do.
        let a = it as f32 * 0.005;
        app.simulation.borrow_mut().sun.pos = Vec3::new(0.0, 20.0 * a.sin(), 20.0 * a.cos());

        app.step();

        // ...and everything after it is `after_render`.
        //
        // Insolation, not occlusion: a facet with nothing between it and the
        // Sun is still dark if it faces away, which on this crater is most of
        // the far wall. `facet_shadow` alone answered the wrong question.
        //
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
