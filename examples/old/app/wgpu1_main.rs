use kalast;
use kalast::Vec3;

fn main() {
    // wgpu event logger
    env_logger::init();
    let ev = winit::event_loop::EventLoop::new().unwrap();
    ev.set_control_flow(winit::event_loop::ControlFlow::Poll);

    // let sun_au = 1.0;
    let sun_dir = Vec3::new(1.0, 0.0, 1.0).normalize();
    let light_dist = 10.0;

    let config = kalast::gpu::config::Config {
        width: 1440,
        height: 1080,
        render_light: true,
        enable_back_face: true,
        camera_pos: Vec3::new(5.0, 0.0, 0.0),
        camera_dir: Vec3::new(-1.0, 0.0, 0.0),
        camera_up: Vec3::new(0.0, 0.0, 1.0),
        camera_fovy: 10.0,
        camera_znear: 0.1,
        camera_zfar: 100.0,
        camera_speed: 4.0,
        camera_sensitivity: 0.4,
        light_pos: sun_dir * light_dist,
        light_color: Vec3::new(1.0, 1.0, 1.0),
        start_paused: true,

        ambient_strength: 0.5,

        // Not working?? looks like specular result instead of diffuse
        diffuse_enable: false,

        // Not working?? does nothing
        specular_enable: false,

        models: vec![kalast::gpu::config::ConfigModel {
            path: "res/plane_crater_1024-5000_h=0.437.obj".to_string(),
            flat: true,
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut app = kalast::gpu::win::App::new(config);

    // app.run_blocked(ev);
    ev.run_app(&mut app).unwrap();
}
