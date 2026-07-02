use std::{cell::RefCell, rc::Rc};

use kalast::{self, Mat4, Vec3};
// use kalast::Vec3;

fn main() {
    let config = kalast::app::config::Config {
        ambient_strength: 0.005,
        ..Default::default()
    };

    let mut app = kalast::app::App::new_with_config(config);

    {
        let mut sim = app.simulation.borrow_mut();

        sim.camera.pos = Vec3::new(0.0, 10.0, 0.0);
        sim.camera.up = Vec3::new(0.0, 0.0, 1.0);
        sim.camera.look_anchor();
        sim.camera.projection.fovy = 45.0 * kalast::util::RPD;
        sim.camera.projection.zfar = 100.0;

        sim.sun = Vec3::new(0.0, -10.0, 0.0);

        let mut mesh = kalast::mesh::Mesh::load("res/cube.obj", |x| x);
        mesh.flatten();
        let mat = Mat4::IDENTITY;
        // mat[0..3, 3] = [2.5, 0.0, 0.0]

        let instance = kalast::app::gpu::InstanceInput {
            mat: mat.to_cols_array_2d(),
            ..Default::default()
        };

        let body = kalast::app::body::Body {
            mesh: Some(Rc::new(RefCell::new(mesh))),
            instance,
            entity: None,
        };
        sim.bodies.push(Rc::new(RefCell::new(body)));
    }

    app.set_tick(|_sim| {});

    app.start();
}
