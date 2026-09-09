//! The Rust twin of `main.py`: the same scene, driven by callbacks.
//!
//! ```sh
//! cargo run --release --example crater_main     # on its own
//! ```
//!
//! ...and the same file, unchanged, runs *inside* the editor: open it there
//! and press Play. Nothing here is written for that -- no entry point, no
//! attributes, no second target. The editor compiles this file into a library
//! through a wrapper it generates, loads it, and calls this `main`.
//!
//! Callbacks rather than a `while` loop is a choice, not a requirement:
//! `step.rs` beside this owns its loop and hosts just as well. This shape is
//! here because it is what `main.py` does, and the two are meant to read
//! alike.

use std::cell::RefCell;
use std::rc::Rc;

use kalast::app::App;
use kalast::app::config::Hud;
use kalast::app::simulation::Simulation;
use kalast::{Float, Mat4, Vec3};

/// Build the scene and install the per-iteration work.
///
/// Everything this example is. Called with the editor's app when hosted, and
/// with a fresh one from `main`.
pub fn scene(app: &mut App) {
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

    // Before the frame: where the Sun is for this iteration.
    app.set_tick(|sim: &mut Simulation, _dt: Float| {
        let a = sim.state.iteration as Float * 0.005;
        sim.sun.pos = Vec3::new(0.0, 20.0 * a.sin(), 20.0 * a.cos());
    });

    // After it: read the shadow map back and say how much is lit. Insolation,
    // not occlusion -- a facet with nothing between it and the Sun is still
    // dark if it faces away.
    app.set_after_render(|sim: &mut Simulation, _dt: Float| {
        let lit = match sim.facet_illumination(0) {
            Some(illum) if !illum.is_empty() => {
                illum.iter().filter(|&&i| i > 0.0).count() as Float / illum.len() as Float
            }
            _ => 0.0,
        };
        let it = sim.state.iteration;
        sim.huds[0].borrow_mut().text = format!("it={it}  lit {:.1} %", lit * 100.0);
    });
}

fn main() {
    let mut app = App::new();
    scene(&mut app);
    app.start();
}
