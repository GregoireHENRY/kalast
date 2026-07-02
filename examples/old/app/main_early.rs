use kalast::{self, Vec3};
// use kalast::Vec3;

fn main() {
    let config = kalast::app::config::Config {
        width: 800,
        height: 600,

        background: wgpu::Color::BLACK,

        ..Default::default()
    };

    let mut app = kalast::app::App::new_with_config(config);

    {
        let mut sim = app.simulation.borrow_mut();
        sim.camera.pos = Vec3::new(0.0, 1.0, 2.0);
        sim.camera.up = Vec3::new(0.0, 1.0, 0.0);
        sim.camera.look_anchor();
        sim.camera.projection.fovy = 45.0 * kalast::util::RPD;
    }

    app.set_tick(|sim| {
        if sim.state.iteration == 100 {
            sim.camera.projection.fovy = 20.0 * kalast::util::RPD;

            println!("#100",);
        }
    });

    app.start();
}
