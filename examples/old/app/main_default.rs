use kalast;
// use kalast::Vec3;

fn main() {
    env_logger::init();
    let ev = winit::event_loop::EventLoop::with_user_event()
        .build()
        .unwrap();

    let config = kalast::app::config::Config {
        debug_app: true,

        ..Default::default()
    };

    let mut app = kalast::app::core::App::new_with_config(config);
    ev.run_app(&mut app).unwrap();
}
